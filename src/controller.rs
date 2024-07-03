use actix_web::{HttpRequest, HttpResponse, HttpResponseBuilder};
use actix_web::body::{BoxBody, MessageBody};
use actix_web::http::StatusCode;
use crate::error::Error;
use crate::store::Store;

pub(crate) type FromRequestFunc<I> = Box<dyn Fn(&HttpRequest) -> I + 'static>;

pub(crate) type FromRequestOnError<E, R> = Box<dyn Fn(&HttpRequest, E) -> R + 'static>;

pub struct Controller<T: Store, B: MessageBody = BoxBody> {
    pub(crate) fn_do_rate_limit: Option<FromRequestFunc<bool>>,
    pub(crate) fn_find_identifier: Option<FromRequestFunc<T::Key>>,
    pub(crate) fn_on_rate_limit_error: Option<FromRequestOnError<Error, HttpResponse<B>>>,
    pub(crate) fn_on_store_error: Option<FromRequestOnError<<T as Store>::Error, HttpResponse<B>>>,
}

impl<T: Store, B: MessageBody> Controller<T, B> {
    /// Create a default Controller, with all functions as [None]
    pub fn new() -> Self {
        Self {
            fn_do_rate_limit: None,
            fn_find_identifier: None,
            fn_on_rate_limit_error: None,
            fn_on_store_error: None,
        }
    }

    /// Determine if a request needs to be checked for rate limiting.
    /// If not set, all requests will be checked.
    pub fn with_do_rate_limit(mut self, f: impl Fn(&HttpRequest) -> bool + 'static) -> Self {
        self.fn_do_rate_limit = Some(Box::new(f));
        self
    }

    /// Extract the identifier from the request, such as the IP address or other information.
    pub fn with_find_identifier(mut self, f: impl Fn(&HttpRequest) -> T::Key + 'static) -> Self {
        self.fn_find_identifier = Some(Box::new(f));
        self
    }

    /// Set the [HttpResponse<B>] to be returned when a rate-limit error occurs.
    pub fn on_rate_limit_error(mut self, f: impl Fn(&HttpRequest, Error) -> HttpResponse<B> + 'static) -> Self {
        self.fn_on_rate_limit_error = Some(Box::new(f));
        self
    }

    /// Set the [HttpResponse] to be returned when an error occurs in the [Store]
    /// (such as Redis or other storage structures).
    pub fn on_store_error(mut self, f: impl Fn(&HttpRequest, <T as Store>::Error) -> HttpResponse<B> + 'static) -> Self {
        self.fn_on_store_error = Some(Box::new(f));
        self
    }
}

impl<T> Default for Controller<T, BoxBody>
    where T: Store<Key = String> + 'static,
{
    /// alias of [Self::new], but use default functions.
    fn default() -> Self {
        Self::new()
            .with_do_rate_limit(default_do_rate_limit)
            .with_find_identifier(default_find_identifier)
            .on_rate_limit_error(default_on_rate_limit_error)
            .on_store_error(default_on_store_error::<T>)
    }
}

pub(crate) fn default_do_rate_limit(_: &HttpRequest) -> bool {
    true
}

pub(crate) fn default_find_identifier(req: &HttpRequest) -> String {
    req.peer_addr()
        .map(|addr| addr.ip().to_string())
        .unwrap_or("<Unknown Source IP>".to_string())
}

pub const DEFAULT_RATE_LIMITED_UNTIL_HEADER: &str = "X-Rate-Limited-Until";

pub(crate) fn default_on_rate_limit_error(_: &HttpRequest, error: Error) -> HttpResponse {
    match error {
        Error::RateLimited(until) => {
            let mut builder = HttpResponseBuilder::new(StatusCode::TOO_MANY_REQUESTS);

            if let Some(until) = until {
                builder.insert_header((DEFAULT_RATE_LIMITED_UNTIL_HEADER, until.timestamp().to_string()));
            }

            builder.finish()
        }
    }
}

pub(crate) fn default_on_store_error<T: Store>(_: &HttpRequest, _: T::Error) -> HttpResponse {
    HttpResponse::new(StatusCode::INTERNAL_SERVER_ERROR)
}
