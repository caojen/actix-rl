use actix_web::{HttpMessage, HttpRequest};
use crate::store::Store;

#[derive(Clone, Default)]
pub struct RateLimitByPass<T: Store + 'static> {
    pub(crate) value: Option<<T as Store>::Value>,
}

impl<T: Store + 'static> RateLimitByPass<T> {
    pub(crate) fn checked(req: &HttpRequest) -> bool {
        req.extensions().get::<RateLimitByPass<T>>().is_some()
    }

    pub(crate) fn check(req: &HttpRequest, value: Option<<T as Store>::Value>) {
        let rl = RateLimitByPass::<T> { value };
        req.extensions_mut().insert(rl);
    }

    pub fn get_value(&self) -> Option<&<T as Store>::Value> {
        self.value.as_ref()
    }
}
