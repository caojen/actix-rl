use crate::store::Store;

#[derive(Debug, Clone)]
pub struct RateLimit<T: Store> {
    pub store: T,
}

impl<T: Store> RateLimit<T> {
    pub fn new(store: T) -> Self {
        Self {
            store,
        }
    }
}
