use actix_web::{HttpRequest, HttpResponse, HttpResponseBuilder};
use actix_web::http::StatusCode;
use crate::error::Error;
use crate::store::Store;

type FromRequestFunc<T> = Box<dyn Fn(&HttpRequest) -> T>;
type FromRequestOnError<T> = Box<dyn Fn(&HttpRequest, Error) -> T>;

pub struct RateLimit<T: Store> {
    pub(crate) store: T,
    pub(crate) fn_find_identifier: FromRequestFunc<T::Key>,
    pub(crate) fn_on_error: FromRequestOnError<HttpResponse>,
}

impl<T: Store> RateLimit<T>{
    /// create a new [RateLimit] middleware, with all custom functions.
    pub fn new(
        store: T,
        find_identifier: FromRequestFunc<T::Key>,
        on_error: FromRequestOnError<HttpResponse>,
    ) -> Self {
        Self {
            store,
            fn_find_identifier: find_identifier,
            fn_on_error: on_error,
        }
    }
}

impl<T: Store<Key = String>> RateLimit<T> {
    /// create a new [RateLimit] middleware with default functions.
    pub fn new_default(store: T) -> Self {
        Self::new(store, Box::new(default_find_identifier), Box::new(default_on_error))
    }
}

fn default_find_identifier(req: &HttpRequest) -> String {
    req.peer_addr()
        .map(|addr| addr.ip().to_string())
        .unwrap_or("<Unknown Source IP>".to_string())
}

fn default_on_error(_: &HttpRequest, error: Error) -> HttpResponse {
    match error {
        Error::RateLimited(until) => HttpResponseBuilder::new(StatusCode::TOO_MANY_REQUESTS)
            .insert_header(("X-Rate-Limit-Until", until.timestamp().to_string()))
            .finish()
    }
}
