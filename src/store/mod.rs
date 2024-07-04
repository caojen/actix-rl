#![allow(unused_imports)]

pub mod mem_store;
#[cfg(feature = "redis-store")]
pub mod redis_store;

use std::fmt::Debug;
use std::ops::Deref;
use std::sync::Arc;
use chrono::{DateTime, Utc};

/// [Store] indicates the location and method of caching,
/// such as storing in memory ([MemStore]) in the form of a HashMap
/// or in Redis in the form of key-value pairs.
///
/// All methods are implemented in an async manner.
///
/// [Store] can be [Clone], use [Arc] to store it.
#[async_trait::async_trait]
pub trait Store: Clone + Send + Sync {
    /// [Error] represents possible errors that may
    /// occur during the execution of a function.
    type Error;

    /// [Key] is used to denote the index type
    /// in key-value pairs.
    type Key: Send + Clone;

    /// [Value] is a structure used to represent values,
    /// such as the current count and the initial time.
    type Value: Value;

    /// Alias of [Value::Count].
    type Count: Send + Clone;

    /// The [incr_by] function takes a [Key] and
    /// an unsigned integer, indicating the amount
    /// by which the index should be incremented.
    ///
    /// The function returns the result of
    /// the incremented count.
    async fn incr_by(&self, key: Self::Key, val: Self::Count) -> Result<Self::Value, Self::Error>;

    /// The [incr] function is a wrapper for the [incr_by] function,
    /// with val = 1.
    async fn incr(&self, key: Self::Key) -> Result<Self::Value, Self::Error>;

    /// The [del] function deletes the storage of
    /// the index [Key] and returns the count result
    /// before deletion.
    async fn del(&self, key: Self::Key) -> Result<Option<Self::Value>, Self::Error>;

    /// The [clear] function clears all cached data.
    ///
    /// This function is not mandatory;
    /// if [Store] does not need to clear cached data
    /// (for example, when stored in Redis or a
    /// relational database, bulk clearing of data
    /// is slow and unnecessary), the function can do nothing.
    async fn clear(&self) -> Result<(), Self::Error>;
}

pub trait Value: Send + Clone + Debug {
    /// [Count] is the type of the counter, such as [u32].
    type Count: Send + PartialOrd + Clone;

    /// Return the count value from the counter.
    fn count(&self) -> Self::Count;

    /// Return the time of first creation.
    fn create_date(&self) -> Option<DateTime<Utc>>;

    /// Return the expiration time.
    fn expire_date(&self) -> Option<DateTime<Utc>>;
}

#[async_trait::async_trait]
impl<T: Store> Store for Arc<T> {
    type Error = <T as Store>::Error;
    type Key = <T as Store>::Key;
    type Value = <T as Store>::Value;
    type Count = <T as Store>::Count;

    async fn incr_by(&self, key: Self::Key, val: Self::Count) -> Result<Self::Value, Self::Error> {
        self.deref().incr_by(key, val).await
    }

    async fn incr(&self, key: Self::Key) -> Result<Self::Value, Self::Error> {
        self.deref().incr(key).await
    }

    async fn del(&self, key: Self::Key) -> Result<Option<Self::Value>, Self::Error> {
        self.deref().del(key).await
    }

    async fn clear(&self) -> Result<(), Self::Error> {
        self.deref().clear().await
    }
}

#[async_trait::async_trait]
impl<'a, T: Store> Store for &'a T {
    type Error = T::Error;
    type Key = T::Key;
    type Value = T::Value;
    type Count = T::Count;

    async fn incr_by(&self, key: Self::Key, val: Self::Count) -> Result<Self::Value, Self::Error> {
        (*self).incr_by(key, val).await
    }

    async fn incr(&self, key: Self::Key) -> Result<Self::Value, Self::Error> {
        (*self).incr(key).await
    }

    async fn del(&self, key: Self::Key) -> Result<Option<Self::Value>, Self::Error> {
        (*self).del(key).await
    }

    async fn clear(&self) -> Result<(), Self::Error> {
        (*self).clear().await
    }
}
