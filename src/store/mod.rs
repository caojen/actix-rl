mod mem_store;
#[allow(unused_imports)]
pub use mem_store::*;

/// [Store] indicates the location and method of caching,
/// such as storing in memory ([MemStore]) in the form of a HashMap
/// or in Redis in the form of key-value pairs.
///
/// All methods are implemented in an async manner.
#[async_trait::async_trait]
pub trait Store {
    /// [Error] represents possible errors that may
    /// occur during the execution of a function.
    type Error;

    /// [Key] is used to denote the index type
    /// in key-value pairs.
    type Key: Send;

    /// The [incr_by] function takes a [Key] and
    /// an unsigned integer, indicating the amount
    /// by which the index should be incremented.
    ///
    /// The function returns the result of
    /// the incremented count.
    async fn incr_by(&self, key: Self::Key, val: u32) -> Result<u32, Self::Error>;

    /// The [incr] function is a wrapper for the [incr_by] function,
    /// with val = 1.
    async fn incr(&self, key: Self::Key) -> Result<u32, Self::Error> {
        self.incr_by(key, 1).await
    }

    /// The [del] function deletes the storage of
    /// the index [Key] and returns the count result
    /// before deletion.
    async fn del(&self, key: Self::Key) -> Result<u32, Self::Error>;

    /// The [clear] function clears all cached data.
    ///
    /// This function is not mandatory;
    /// if [Store] does not need to clear cached data
    /// (for example, when stored in Redis or a
    /// relational database, bulk clearing of data
    /// is slow and unnecessary), the function can do nothing.
    async fn clear(&self) -> Result<(), Self::Error>;
}