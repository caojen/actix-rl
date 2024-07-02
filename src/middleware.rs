use std::sync::Arc;
use actix_web::{HttpRequest, HttpResponse, HttpResponseBuilder};
use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::http::StatusCode;
use futures_util::future::{LocalBoxFuture, Ready, ready};
use crate::error::Error;
use crate::store::Store;

type FromRequestFunc<I> = Box<dyn Fn(&HttpRequest) -> I>;

type FromRequestOnError<E, R> = Box<dyn Fn(&HttpRequest, E) -> R>;

pub struct RateLimit<T: Store> {
    inner: Arc<RateLimitInner<T>>,
}

struct RateLimitInner<T: Store> {
    pub store: T,
    pub fn_find_identifier: FromRequestFunc<T::Key>,
    pub fn_on_rate_limit_error: FromRequestOnError<Error, HttpResponse>,
    pub fn_on_store_error: FromRequestOnError<<T as Store>::Error, HttpResponse>,
}

impl<T, S> Transform<S, ServiceRequest> for RateLimit<T>
    where
        T: Store + 'static,
        S: Service<ServiceRequest, Response = HttpResponse, Error = actix_web::Error>,
        S::Future: 'static,
        <T as Store>::Key: 'static,
{
    type Response = HttpResponse;
    type Error = S::Error;
    type Transform = RateLimitService<T, S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RateLimitService {
            inner: self.inner.clone(),
            service,
        }))
    }
}

pub struct RateLimitService<T, S>
    where T: Store,
{
    inner: Arc<RateLimitInner<T>>,
    service: S,
}

impl<T, S> Service<ServiceRequest> for RateLimitService<T, S>
    where
        T: Store + 'static,
        S: Service<ServiceRequest, Response = HttpResponse, Error = actix_web::Error>,
        S::Future: 'static,
        <T as Store>::Key: 'static,
{
    type Response = HttpResponse;
    type Error = S::Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, svc: ServiceRequest) -> Self::Future {
        let req = svc.request().clone();
        let identifier = (self.inner.fn_find_identifier)(&req);
        let inner = self.inner.clone();

        let fut = self.service.call(svc);

        Box::pin(async move {
            match inner.store.incr(identifier).await {
                Err(e) => Ok(inner.fn_on_store_error(&req, e)),
                Ok(value) => todo!(),
            }

            let res = fut.await?;
            Ok(res)
        })
    }
}

impl<T: Store> RateLimit<T>{
    /// create a new [RateLimit] middleware, with all custom functions.
    pub fn new(
        store: T,
        find_identifier: FromRequestFunc<T::Key>,
        on_rate_limit_error: FromRequestOnError<Error, HttpResponse>,
        on_store_error: FromRequestOnError<<T as Store>::Error, HttpResponse>,
    ) -> Self {
        Self {
            inner: Arc::new(RateLimitInner {
                store,
                fn_find_identifier: find_identifier,
                fn_on_rate_limit_error: on_rate_limit_error,
                fn_on_store_error: on_store_error,
            })
        }
    }
}

impl<T: Store<Key = String> + 'static> RateLimit<T> {
    /// create a new [RateLimit] middleware with default functions.
    pub fn new_default(store: T) -> Self {
        Self::new(
            store,
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

fn default_on_rate_limit_error(_: &HttpRequest, error: Error) -> HttpResponse {
    match error {
        Error::RateLimited(until) => HttpResponseBuilder::new(StatusCode::TOO_MANY_REQUESTS)
            .insert_header(("X-Rate-Limit-Until", until.timestamp().to_string()))
            .finish()
    }
}

fn default_on_store_error<T: Store>(_: &HttpRequest, _: T::Error) -> HttpResponse {
    HttpResponse::new(StatusCode::INTERNAL_SERVER_ERROR)
}


#[cfg(test)]
mod tests {
    use actix_web::{App, test, web};
    use crate::store::MemStore;
    use super::*;

    async fn empty() -> HttpResponse {
        HttpResponse::new(StatusCode::NO_CONTENT)
    }

    #[tokio::test]
    async fn test_middleware() -> anyhow::Result<()> {
        // let app = test::init_service(
        //     App::new()
        //         .wrap(RateLimit::new_default(MemStore::new(0, chrono::Duration::seconds(60))))
        //         .route("/", web::get().to(empty))
        // ).await;
        //
        // let req = test::TestRequest::get().to_request();
        // let resp = test::call_service(&app, req).await;
        // assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        Ok(())
    }
}
