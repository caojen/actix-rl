use std::rc::Rc;
use std::sync::Arc;
use actix_web::{HttpRequest, HttpResponse, HttpResponseBuilder};
use actix_web::body::EitherBody;
use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::http::StatusCode;
use futures_util::future::{LocalBoxFuture, Ready, ready};
use crate::error::Error;
use crate::store::{Store, Value};

type FromRequestFunc<I> = Box<dyn Fn(&HttpRequest) -> I>;

type FromRequestOnError<E, R> = Box<dyn Fn(&HttpRequest, E) -> R>;

pub struct RateLimit<T: Store> {
    inner: Arc<RateLimitInner<T>>,
}

struct RateLimitInner<T: Store> {
    pub store: T,
    pub max: <<T as Store>::Value as Value>::Count,
    pub fn_find_identifier: FromRequestFunc<T::Key>,
    pub fn_on_rate_limit_error: FromRequestOnError<Error, HttpResponse>,
    pub fn_on_store_error: FromRequestOnError<<T as Store>::Error, HttpResponse>,
}

impl<T, S, B> Transform<S, ServiceRequest> for RateLimit<T>
    where
        T: Store + 'static,
        S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
        S::Future: 'static,
        B: 'static,
        <T as Store>::Key: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = S::Error;
    type Transform = RateLimitService<T, S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RateLimitService {
            inner: self.inner.clone(),
            service: Rc::new(service),
        }))
    }
}

pub struct RateLimitService<T, S>
    where T: Store,
{
    inner: Arc<RateLimitInner<T>>,
    service: Rc<S>,
}

impl<T, S, B> Service<ServiceRequest> for RateLimitService<T, S>
    where
        T: Store + 'static,
        S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
        S::Future: 'static,
        B: 'static,
        <T as Store>::Key: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = S::Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, svc: ServiceRequest) -> Self::Future {
        let service = self.service.clone();
        let identifier = (self.inner.fn_find_identifier)(svc.request());
        let inner = self.inner.clone();

        Box::pin(async move {
            let req = svc.request();
            match inner.store.incr(identifier).await {
                Err(e) => {
                    let body = (inner.fn_on_store_error)(req, e);
                    return Ok(ServiceResponse::new(req.clone(), body.map_into_right_body()));
                },
                Ok(value) => if value.count() > inner.max {
                    let body = (inner.fn_on_rate_limit_error)(req, Error::RateLimited(value.expire_date()));
                    return Ok(ServiceResponse::new(req.clone(), body.map_into_right_body()));
                },
            }

            let res = service.call(svc).await?.map_into_left_body();
            Ok(res)
        })
    }
}

impl<T: Store> RateLimit<T>{
    /// create a new [RateLimit] middleware, with all custom functions.
    pub fn new(
        store: T,
        max: <<T as Store>::Value as Value>::Count,
        find_identifier: FromRequestFunc<T::Key>,
        on_rate_limit_error: FromRequestOnError<Error, HttpResponse>,
        on_store_error: FromRequestOnError<<T as Store>::Error, HttpResponse>,
    ) -> Self {
        Self {
            inner: Arc::new(RateLimitInner {
                store,
                max,
                fn_find_identifier: find_identifier,
                fn_on_rate_limit_error: on_rate_limit_error,
                fn_on_store_error: on_store_error,
            })
        }
    }
}

impl<T: Store<Key = String> + 'static> RateLimit<T> {
    /// create a new [RateLimit] middleware with default functions.
    pub fn new_default(store: T, max: <<T as Store>::Value as Value>::Count) -> Self {
        Self::new(
            store,
            max,
            Box::new(default_find_identifier),
            Box::new(default_on_rate_limit_error),
            Box::new(default_on_store_error::<T>),
        )
    }
}

fn default_find_identifier(req: &HttpRequest) -> String {
    req.peer_addr()
        .map(|addr| addr.ip().to_string())
        .unwrap_or("<Unknown Source IP>".to_string())
}

const RATE_LIMITED_UNTIL_HEADER: &str = "X-Rate-Limited-Until";

fn default_on_rate_limit_error(_: &HttpRequest, error: Error) -> HttpResponse {
    match error {
        Error::RateLimited(until) => {
            let mut builder = HttpResponseBuilder::new(StatusCode::TOO_MANY_REQUESTS);

            if let Some(until) = until {
                builder.insert_header((RATE_LIMITED_UNTIL_HEADER, until.timestamp().to_string()));
            }

            builder.finish()
        }
    }
}

fn default_on_store_error<T: Store>(_: &HttpRequest, _: T::Error) -> HttpResponse {
    HttpResponse::new(StatusCode::INTERNAL_SERVER_ERROR)
}


#[cfg(test)]
mod tests {
    use actix_web::{App, test, web};
    use chrono::{Utc};
    use tokio::time::Instant;
    use crate::store::MemStore;
    use super::*;

    async fn empty() -> HttpResponse {
        HttpResponse::new(StatusCode::NO_CONTENT)
    }

    #[tokio::test]
    async fn test_middleware() -> anyhow::Result<()> {
        let store = MemStore::new(1024, chrono::Duration::seconds(60));

        let app = test::init_service(
            App::new()
                .wrap(RateLimit::new_default(
                    store,
                    10,
                ))
                .route("/", web::get().to(empty))
        ).await;

        for _ in 0..10 {
            let req = test::TestRequest::get().to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        }

        // then, rate limited...
        let mut wait_until = 0i64;
        for _ in 0..10 {
            let req = test::TestRequest::get().to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
            let ts = resp.headers().get(RATE_LIMITED_UNTIL_HEADER).unwrap().to_str().unwrap_or_default();
            wait_until = ts.parse().unwrap();
        }

        println!("rate limited until: {}", wait_until);
        tokio::time::sleep_until(
            Instant::now() + chrono::Duration::seconds(wait_until - Utc::now().timestamp() + 1).to_std().unwrap()
        ).await;

        // ok...
        for _ in 0..5 {
            let req = test::TestRequest::get().to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        }

        Ok(())
    }
}
