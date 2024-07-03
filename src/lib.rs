//! # `actix-rl`: RateLimiter for `actix-web`

//! ## Description
//! `actix-rl` is a RateLimit middleware for the `actix-web` library.
//! It supports asynchronous processing and currently provides two storage options:
//! in-memory storage (`MemStore`) and Redis storage (`RedisStore`).

//! If you have other storage options, feel free to submit a Pull Request.
//! PR is welcome.

//! ## Features
//! |    Feature    |  Component   |                                    Description                                    |
//! |:-------------:|:------------:|:---------------------------------------------------------------------------------:|
//! |   `default`   |  `MemStore`  |                               Store data in memory                                |
//! | `redis-store` | `RedisStore` | Store data using an async connection from [redis](https://crates.io/crates/redis) |

//! ## Usage
//! 1. Define a `Store` where the program stores information and sets timeouts.
//! 2. Define a `Controller`. The `Controller` is used to define middleware
//!    response behaviors, such as how to return HTTP information during exceptions.
//!    You can also use the library's provided default functions,
//!    but in most cases, you will need to customize the response to return necessary limit information to the client.
//! 3. Finally, add a `Middleware` to your HTTP Server using the `wrap` function (from `actix-web`).

//! ### Examples
//! You can find examples in `examples` folder.

//! ### Store
//! The `Store` is used to store caching information.

//! Let's take `MemCache` as an example:

//! ```rust
//! // data timeout for each key is 10 seconds, with 1024 init capacity.
//! let store = actix_rl::store::mem_store::MemStore::new(1024, chrono::Duration::seconds(10));
//! ```

//! ### Controller
//! `Controller` is a set of functions. To create a default one:
//! ```rust
//! let controller = actix_rl::controller::Controller::new();
//! ```

//! You can determine which requests should be checked, by modifying `Controller`:
//! ```rust
//! let controller = actix_rl::controller::Controller::new();
//! let controller = controller.with_do_rate_limit(|req| !req.path().start_with("/healthz"));
//! ```

//! In this case, only those requests without prefix `/healthz` will be checked by RateLimiter.

//! For more functions, please check the doc of `Controller`.

//! ### RateLimiter
//! Define a `RateLimiter` and `wrap` to HTTP server:

//! ```rust
//! let rate_limiter = actix_rl::middleware::RateLimitMiddleware::new(
//!     store,
//!     10, // max count is 10, which means max 10 hits per 10 seconds.
//!     controller,
//! );
//! ```

//! Then, add it to `actix-web` HTTP server wrap:
//! ```rust
//! App::new()
//!    .wrap(rate_limiter)
//!     // ...
//! ```

pub mod store;
pub mod middleware;
pub mod error;
pub mod controller;
pub mod utils;
