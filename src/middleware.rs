use std::rc::Rc;
use std::sync::Arc;
use actix_web::body::{BoxBody, EitherBody, MessageBody};
use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use futures_util::future::{LocalBoxFuture, Ready, ready};
use crate::controller::{Controller, default_do_rate_limit, default_on_rate_limit_error, default_on_store_error};
use crate::error::Error;
use crate::store::{Store, Value};
use crate::utils::RateLimitByPass;

/// [RateLimit] is the rate-limit middleware.
///
/// Params [T]: the [Store];
/// Params [CB]: the response body for [Controller]. (ControllerBody)
#[derive(Clone)]
pub struct RateLimit<T: Store, CB: MessageBody = BoxBody> {
    inner: Arc<RateLimitInner<T, CB>>,
}

#[derive(Clone)]
struct RateLimitInner<T: Store, CB: MessageBody = BoxBody> {
    pub store: T,
    pub max: <<T as Store>::Value as Value>::Count,
    pub controller: Controller<T, CB>,
}

impl<T, CB, S, B> Transform<S, ServiceRequest> for RateLimit<T, CB>
    where
        T: Store + 'static,
        CB: MessageBody + 'static,
        S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
        S::Future: 'static,
        B: 'static,
        <T as Store>::Key: 'static,
{
    type Response = ServiceResponse<EitherBody<B, EitherBody<BoxBody, CB>>>;
    type Error = S::Error;
    type Transform = RateLimitService<T, CB, S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RateLimitService {
            inner: self.inner.clone(),
            service: Rc::new(service),
        }))
    }
}

#[derive(Clone)]
pub struct RateLimitService<T, CB, S>
    where
        T: Store,
        CB: MessageBody,
{
    inner: Arc<RateLimitInner<T, CB>>,
    service: Rc<S>,
}

impl<T, CB, S, B> Service<ServiceRequest> for RateLimitService<T, CB, S>
    where
        T: Store + 'static,
        CB: MessageBody + 'static,
        S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
        S::Future: 'static,
        B: 'static,
        <T as Store>::Key: 'static,
{
    type Response = ServiceResponse<EitherBody<B, EitherBody<BoxBody, CB>>>;
    type Error = S::Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, svc: ServiceRequest) -> Self::Future {
        let service = self.service.clone();
        let inner = self.inner.clone();

        Box::pin(async move {
            let checked = RateLimitByPass::checked(svc.request());
            let do_rate_limit = !checked && if let Some(f) = &inner.controller.fn_do_rate_limit {
                f(svc.request())
            } else {
                // use default function
                default_do_rate_limit(svc.request())
            };

            if do_rate_limit {
                // get identifier of this request
                let identifier = inner.controller.fn_find_identifier.as_ref()
                    .map(|f| f(svc.request()));

                if let Some(identifier) = identifier { // continue only when identifier is found.
                    let req = svc.request();
                    match inner.store.incr(identifier).await {
                        Err(e) => {
                            // store error occur
                            return if let Some(f) = &inner.controller.fn_on_store_error {
                                let body = f(req, e);
                                Ok(ServiceResponse::new(
                                    req.clone(),
                                    body.map_into_right_body().map_into_right_body(),
                                ))
                            } else {
                                let body = default_on_store_error::<T>(req, e);
                                Ok(ServiceResponse::new(
                                    req.clone(),
                                    body.map_into_left_body().map_into_right_body(),
                                ))
                            }

                        },
                        Ok(value) => if value.count() > inner.max {
                            // rate limit error occur
                            let err = Error::RateLimited(value.expire_date());

                            return if let Some(f) = &inner.controller.fn_on_rate_limit_error {
                                let body = f(req, err);
                                Ok(ServiceResponse::new(
                                    req.clone(),
                                    body.map_into_right_body().map_into_right_body(),
                                ))
                            } else {
                                let body = default_on_rate_limit_error(req, err);
                                Ok(ServiceResponse::new(
                                    req.clone(),
                                    body.map_into_left_body().map_into_right_body(),
                                ))
                            }
                        },
                    }
                }
            }

            // rate-limit bypass
            // Add a marker to the request to ensure that no further checks are performed on it.
            RateLimitByPass::check(svc.request());

            // rate-limit bypass
            let res = service.call(svc).await?.map_into_left_body();
            Ok(res)
        })
    }
}

impl<T: Store, CB: MessageBody> RateLimit<T, CB> {
    /// create a new [RateLimit] middleware, with all custom functions.
    pub fn new(
        store: T,
        max: <<T as Store>::Value as Value>::Count,
        controller: Controller<T, CB>
    ) -> Self {
        Self {
            inner: Arc::new(RateLimitInner {
                store,
                max,
                controller,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use actix_web::{App, HttpRequest, HttpResponse, test, web};
    use actix_web::http::StatusCode;
    use chrono::{Utc};
    use tokio::time::Instant;
    use crate::controller::{default_find_identifier, DEFAULT_RATE_LIMITED_UNTIL_HEADER};
    use crate::store::MemStore;
    use super::*;

    async fn empty() -> HttpResponse {
        HttpResponse::new(StatusCode::NO_CONTENT)
    }

    #[tokio::test]
    async fn test_middleware() -> anyhow::Result<()> {
        let store = MemStore::new(1024, chrono::Duration::seconds(10));
        let controller = Controller::default();

        let app = test::init_service(
            App::new()
                .wrap(RateLimit::new(
                    store,
                    10,
                    controller,
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
            let ts = resp.headers().get(DEFAULT_RATE_LIMITED_UNTIL_HEADER).unwrap().to_str().unwrap_or_default();
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

    fn test_do_rate_limit_default_rate_limit_func(req: &HttpRequest) -> bool {
        req.path() != "/bypass"
    }

    #[tokio::test]
    async fn test_do_rate_limit() -> anyhow::Result<()> {
        let store = MemStore::new(1024, chrono::Duration::seconds(10));

        let controller = Controller::<_, BoxBody>::new()
            .with_do_rate_limit(test_do_rate_limit_default_rate_limit_func)
            .with_find_identifier(default_find_identifier)
            .on_rate_limit_error(default_on_rate_limit_error)
            .on_store_error(default_on_store_error::<MemStore>);

        let app = test::init_service(
            App::new()
                .wrap(RateLimit::new(
                    store,
                    10,
                    controller,
                ))
                .route("/", web::get().to(empty))
                .route("/bypass", web::get().to(empty))
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
            let ts = resp.headers().get(DEFAULT_RATE_LIMITED_UNTIL_HEADER).unwrap().to_str().unwrap_or_default();
            wait_until = ts.parse().unwrap();
        }

        // but /bypass will not be rate_limited
        for _ in 0..10 {
            let req = test::TestRequest::get().uri("/bypass").to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), StatusCode::NO_CONTENT);
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
