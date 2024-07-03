use actix_web::{HttpMessage, HttpRequest};
use crate::store::Store;

#[derive(Clone, Default)]
pub struct RateLimitByPass<T: Store + 'static> {
    pub(crate) value: Option<<T as Store>::Value>,
}

impl<T: Store + 'static> RateLimitByPass<T> {
    pub(crate) fn checked(req: &HttpRequest) -> bool {
        Self::from_request(req).is_some()
    }

    pub(crate) fn check(req: &HttpRequest, value: Option<<T as Store>::Value>) {
        let rl = RateLimitByPass::<T> { value };
        req.extensions_mut().insert(rl);
    }

    pub fn get_value(&self) -> Option<&<T as Store>::Value> {
        self.value.as_ref()
    }

    pub fn from_request(req: &HttpRequest) -> Option<RateLimitByPass<T>> {
        req.extensions().get::<RateLimitByPass<T>>().cloned()
    }
}
