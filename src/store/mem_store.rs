use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use chrono::{DateTime, Utc};
use tokio::sync::Mutex;
use crate::store::{Store, Value};

pub const DEFAULT_STORE_CAPACITY: usize = 4096;

/// [DateCount] stores the creation time and the current count.
#[derive(Debug, Clone, Copy)]
pub struct DateCount {
    pub create_date: DateTime<Utc>,
    pub count: u32,
}

impl Default for DateCount {
    fn default() -> Self {
        Self {
            create_date: Utc::now(),
            count: 0u32,
        }
    }
}

impl DateCount {
    /// Check if [DateCount] has expired.
    pub fn expired(&self, ttl: chrono::Duration) -> bool {
        self.expired_at(ttl, Utc::now())
    }

    /// Check if [DateCount] has expired at instant.
    pub fn expired_at(&self, ttl: chrono::Duration, instant: DateTime<Utc>) -> bool {
        self.create_date + ttl < instant
    }
}

#[derive(Debug, Clone)]
pub struct DateCountUntil {
    pub date_count: DateCount,
    pub until: DateTime<Utc>,
}

impl Value for DateCountUntil {
    type Count = u32;

    fn count(&self) -> Self::Count {
        self.date_count.count
    }

    fn create_date(&self) -> Option<DateTime<Utc>> {
        Some(self.date_count.create_date)
    }

    fn expire_date(&self) -> Option<DateTime<Utc>> {
        Some(self.until)
    }
}

/// [MemStore] stores data in memory.
#[derive(Debug, Clone)]
pub struct MemStore {
    pub(crate) inner: Arc<Mutex<MemStoreInner>>,
}

impl MemStore {
    pub fn new(capacity: usize, ttl: chrono::Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(MemStoreInner::new(capacity, ttl))),
        }
    }
}

impl Default for MemStore {
    fn default() -> Self {
        Self::new(DEFAULT_STORE_CAPACITY, chrono::Duration::seconds(60))
    }
}

#[async_trait::async_trait]
impl Store for MemStore {
    type Error = ();
    type Key = String;
    type Value = DateCountUntil;
    type Count = u32;

    async fn incr_by(&self, key: Self::Key, val: u32) -> Result<Self::Value, Self::Error> {
        Ok(self.inner.lock().await.incr_by(key, val))
    }

    async fn incr(&self, key: Self::Key) -> Result<Self::Value, Self::Error> {
        self.incr_by(key, 1).await
    }

    async fn del(&self, key: Self::Key) -> Result<Option<Self::Value>, Self::Error> {
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
    pub(crate) ttl: chrono::Duration,
}

impl MemStoreInner {
    pub fn new(capacity: usize, ttl: chrono::Duration) -> Self {
        Self {
            data: HashMap::with_capacity(capacity),
            ttl,
        }
    }

    pub fn incr_by(&mut self, key: String, val: u32) -> DateCountUntil {
        let entry = self.data.entry(key).or_default();

        if entry.expired(self.ttl) {
            *entry = DateCount::default()
        }

        entry.count += val;

        DateCountUntil {
            date_count: *entry,
            until: entry.create_date + self.ttl,
        }
    }

    pub fn del(&mut self, key: String) -> Option<DateCountUntil> {
        self.data.remove(&key)
            .map(|entry| DateCountUntil {
                date_count: entry,
                until: entry.create_date + self.ttl,
            })
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
        let store = MemStore::new(8, chrono::Duration::seconds(100000));

        assert_eq!(store.incr("John".to_string()).await?.date_count.count, 1);
        assert_eq!(store.incr("John".to_string()).await?.date_count.count, 2);
        assert_eq!(store.incr("John".to_string()).await?.date_count.count, 3);
        assert_eq!(store.incr_by("John".to_string(), 2).await?.date_count.count, 5);

        assert_eq!(store.incr_by("Meg".to_string(), 3).await?.date_count.count, 3);
        assert_eq!(store.incr_by("Meg".to_string(), 3).await?.date_count.count, 6);
        assert_eq!(store.incr_by("Meg".to_string(), 3).await?.date_count.count, 9);
        assert_eq!(store.incr_by("Meg".to_string(), 4).await?.date_count.count, 13);
        assert_eq!(store.incr_by("Meg".to_string(), 4).await?.date_count.count, 17);
        assert_eq!(store.incr("Meg".to_string()).await?.date_count.count, 18);

        let cloned = store.clone();
        assert_eq!(cloned.incr("John".to_string()).await?.date_count.count, 6);
        assert_eq!(store.incr("John".to_string()).await?.date_count.count, 7);

        store.del("Meg".to_string()).await?;
        assert_eq!(store.incr_by("Meg".to_string(), 3).await?.date_count.count, 3);
        assert_eq!(cloned.incr_by("Meg".to_string(), 3).await?.date_count.count, 6);
        assert_eq!(store.incr_by("Meg".to_string(), 3).await?.date_count.count, 9);
        assert_eq!(store.incr("John".to_string()).await?.date_count.count, 8);

        Ok(())
    }

    #[tokio::test]
    async fn clear() -> Result<(), ()> {
        let store = MemStore::new(8, chrono::Duration::seconds(100000));

        assert_eq!(store.incr("John".to_string()).await?.date_count.count, 1);
        assert_eq!(store.incr_by("Meg".to_string(), 3).await?.date_count.count, 3);

        store.clear().await?;
        assert_eq!(store.incr("John".to_string()).await?.date_count.count, 1);
        assert_eq!(store.incr_by("Meg".to_string(), 3).await?.date_count.count, 3);

        Ok(())
    }

    #[tokio::test]
    async fn ttl() -> Result<(), ()> {
        let store = MemStore::new(8, chrono::Duration::seconds(5));
        assert_eq!(store.incr("John".to_string()).await?.date_count.count, 1);

        // wait 2 seconds to add a new one...
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        assert_eq!(store.incr_by("Meg".to_string(), 3).await?.date_count.count, 3);

        // wait 4 seconds, "John" should be expired, while "Meg" should still exist.
        tokio::time::sleep(tokio::time::Duration::from_secs(4)).await;
        assert_eq!(store.incr("John".to_string()).await?.date_count.count, 1);
        assert_eq!(store.incr_by("Meg".to_string(), 3).await?.date_count.count, 6);

        Ok(())
    }
}
