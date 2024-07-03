use std::ops::Add;
use std::sync::Arc;
use chrono::{DateTime, Utc};
use redis::{AsyncCommands, Commands, RedisResult};
use redis::aio::MultiplexedConnection;
use crate::store::{Store, Value};

#[derive(Debug, Clone, Copy)]
pub struct RateLimitResult {
    pub count: i32,
    pub expire_date: DateTime<Utc>,
}

impl Value for RateLimitResult {
    type Count = i32;

    fn count(&self) -> Self::Count {
        self.count
    }

    fn create_date(&self) -> Option<DateTime<Utc>> {
        None
    }

    fn expire_date(&self) -> Option<DateTime<Utc>> {
        Some(self.expire_date)
    }
}

/// [RedisStore] stores data in redis.
#[derive(Clone)]
pub struct RedisStore {
    pub(crate) inner: Arc<RedisStoreInner>,
}

impl RedisStore {
    /// create from a [redis::Client]
    pub fn from_client<T: ToString>(client: redis::Client, prefix: T, ttl: chrono::Duration) -> Self {
        Self {
            inner: Arc::new(RedisStoreInner {
                client,
                prefix: prefix.to_string(),
                ttl,
            }),
        }
    }
}

#[async_trait::async_trait]
impl Store for RedisStore {
    type Error = redis::RedisError;
    type Key = String;
    type Value = RateLimitResult;
    type Count = i32;

    async fn incr_by(&self, key: Self::Key, val: Self::Count) -> Result<Self::Value, Self::Error> {
        let redis_key = self.inner.get_key(&key);
        let mut conn = self.inner.conn().await?;

        // SET {key} 0 NX PX {ttl in millisecons}
        // incrby {key} {val}
        // get {key} ===> as the result
        // get {ttl} ===> as the result

        let result: (i32, i64) = redis::pipe()
            .cmd("SET").arg(&redis_key).arg(0).arg("NX").arg("PX").arg(self.inner.ttl.num_milliseconds()).ignore()
            .cmd("INCRBY").arg(&redis_key).arg(val).ignore()
            .cmd("GET").arg(&redis_key)
            .cmd("TTL").arg(&redis_key)
            .query_async(&mut conn)
            .await?;

        Ok(RateLimitResult {
            count: result.0,
            expire_date: Utc::now() + chrono::Duration::seconds(result.1),
        })
    }

    async fn incr(&self, key: Self::Key) -> Result<Self::Value, Self::Error> {
        self.incr_by(key, 1).await
    }

    async fn del(&self, key: Self::Key) -> Result<Option<Self::Value>, Self::Error> {
        let redis_key = self.inner.get_key(key);
        let mut conn = self.inner.conn().await?;
        conn.del(redis_key).await?;

        Ok(None)
    }

    /// Since we cannot clear all data in redis, here we do nothing.
    async fn clear(&self) -> Result<(), Self::Error> {
        Ok(())
    }
}

pub(crate) struct RedisStoreInner {
    /// the redis client
    pub client: redis::Client,
    /// the prefix which would prepend to redis-key
    pub prefix: String,
    /// timeout duration
    pub ttl: chrono::Duration,
}

impl RedisStoreInner {
    pub fn get_key<T: AsRef<str>>(&self, key: T) -> String {
        format!("{}-{}", &self.prefix, key.as_ref())
    }

    pub async fn conn(&self) -> RedisResult<MultiplexedConnection> {
        self.client.get_multiplexed_async_connection().await
    }
}
