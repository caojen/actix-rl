use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::time;
use tokio::sync::Mutex;
use crate::store::Store;

pub const DEFAULT_STORE_CAPACITY: usize = 4096;

/// [DateCount] stores the creation time and the current count.
#[derive(Debug, Clone)]
pub(crate) struct DateCount {
    pub create_date: time::Instant,
    pub count: u32,
}

impl Default for DateCount {
    fn default() -> Self {
        Self {
            create_date: time::Instant::now(),
            count: 0u32,
        }
    }
}

impl DateCount {
    /// Check if [DateCount] has expired.
    pub fn expired(&self, ttl: time::Duration) -> bool {
        self.expired_at(ttl, time::Instant::now())
    }

    /// Check if [DateCount] has expired at instant.
    pub fn expired_at(&self, ttl: time::Duration, instant: time::Instant) -> bool {
        self.create_date + ttl < instant
    }
}

/// [MemStore] stores data in memory.
#[derive(Debug, Clone)]
pub struct MemStore {
    pub(crate) inner: Arc<Mutex<MemStoreInner>>,
}

impl MemStore {
    pub fn new(capacity: usize, ttl: time::Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(MemStoreInner::new(capacity, ttl))),
        }
    }
}

impl Default for MemStore {
    fn default() -> Self {
        Self::new(DEFAULT_STORE_CAPACITY, time::Duration::from_secs(60))
    }
}

#[async_trait::async_trait]
impl Store for MemStore {
    /// We don't have any error here.
    type Error = ();
    type Key = String;

    async fn incr_by(&self, key: Self::Key, val: u32) -> Result<u32, Self::Error> {
        Ok(self.inner.lock().await.incr_by(key, val))
    }

    async fn del(&self, key: Self::Key) -> Result<u32, Self::Error> {
        Ok(self.inner.lock().await.del(key))
    }

    async fn clear(&self) -> Result<(), Self::Error> {
        self.inner.lock().await.clear();
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MemStoreInner {
    pub(crate) data: HashMap<String, DateCount>,
    /// The [ttl] field indicates the time-to-live (TTL)
    /// of data from its creation. Once this TTL expires,
    /// the data in the cache is considered empty or expired.
    pub(crate) ttl: time::Duration,
}

impl MemStoreInner {
    pub fn new(capacity: usize, ttl: time::Duration) -> Self {
        Self {
            data: HashMap::with_capacity(capacity),
            ttl,
        }
    }

    pub fn incr_by(&mut self, key: String, val: u32) -> u32 {
        let entry = self.data.entry(key).or_default();

        if entry.expired(self.ttl) {
            *entry = DateCount::default()
        }

        entry.count += val;
        entry.count
    }

    pub fn del(&mut self, key: String) -> u32 {
        self.data.remove(&key)
            .map(|date_count| date_count.count)
            .unwrap_or_default()
    }

    pub fn clear(&mut self) {
        self.data.clear()
    }
}

impl Deref for MemStoreInner {
    type Target = HashMap<String, DateCount>;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl DerefMut for MemStoreInner {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn incr_del() -> Result<(), ()> {
        let store = MemStore::new(8, time::Duration::from_secs(100000));

        assert_eq!(store.incr("John".to_string()).await?, 1);
        assert_eq!(store.incr("John".to_string()).await?, 2);
        assert_eq!(store.incr("John".to_string()).await?, 3);
        assert_eq!(store.incr_by("John".to_string(), 2).await?, 5);

        assert_eq!(store.incr_by("Meg".to_string(), 3).await?, 3);
        assert_eq!(store.incr_by("Meg".to_string(), 3).await?, 6);
        assert_eq!(store.incr_by("Meg".to_string(), 3).await?, 9);
        assert_eq!(store.incr_by("Meg".to_string(), 4).await?, 13);
        assert_eq!(store.incr_by("Meg".to_string(), 4).await?, 17);
        assert_eq!(store.incr("Meg".to_string()).await?, 18);

        let cloned = store.clone();
        assert_eq!(cloned.incr("John".to_string()).await?, 6);
        assert_eq!(store.incr("John".to_string()).await?, 7);

        store.del("Meg".to_string()).await?;
        assert_eq!(store.incr_by("Meg".to_string(), 3).await?, 3);
        assert_eq!(cloned.incr_by("Meg".to_string(), 3).await?, 6);
        assert_eq!(store.incr_by("Meg".to_string(), 3).await?, 9);
        assert_eq!(store.incr("John".to_string()).await?, 8);

        Ok(())
    }

    #[tokio::test]
    async fn clear() -> Result<(), ()> {
        let store = MemStore::new(8, time::Duration::from_secs(100000));

        assert_eq!(store.incr("John".to_string()).await?, 1);
        assert_eq!(store.incr_by("Meg".to_string(), 3).await?, 3);

        store.clear().await?;
        assert_eq!(store.incr("John".to_string()).await?, 1);
        assert_eq!(store.incr_by("Meg".to_string(), 3).await?, 3);

        Ok(())
    }

    #[tokio::test]
    async fn ttl() -> Result<(), ()> {
        let store = MemStore::new(8, time::Duration::from_secs(5));
        assert_eq!(store.incr("John".to_string()).await?, 1);

        // wait 2 seconds to add a new one...
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        assert_eq!(store.incr_by("Meg".to_string(), 3).await?, 3);

        // wait 4 seconds, "John" should be expired, while "Meg" should still exist.
        tokio::time::sleep(tokio::time::Duration::from_secs(4)).await;
        assert_eq!(store.incr("John".to_string()).await?, 1);
        assert_eq!(store.incr_by("Meg".to_string(), 3).await?, 6);

        Ok(())
    }
}
