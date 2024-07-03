use actix_web::{HttpMessage, HttpRequest};

#[derive(Debug, Clone, Default)]
pub struct RateLimitByPass;

impl RateLimitByPass {
    pub fn checked(req: &HttpRequest) -> bool {
        req.extensions().get::<Self>().is_some()
    }

    pub fn check(req: &HttpRequest) {
        req.extensions_mut().insert(Self::default());
    }
}
