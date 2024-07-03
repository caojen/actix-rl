//! In this example, we create a RateLimiter stored in static with max 3 requests in 10 secs.
//! We use [store::MemStore] as our storage.

use std::process::exit;
use actix_web::{App, HttpServer, web};
use actix_rl::controller;
use actix_rl::middleware::RateLimit;
use actix_rl::store::redis_store::RedisStore;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // actix-web simple http-server starts here...
    HttpServer::new(|| {
        // connect to redis here...
        let username = "";
        let password = "";
        let host = "127.0.0.1";
        let port = "6379";
        let client = redis::Client::open(format!("redis://{}:{}@{}:{}", username, password, host, port)).unwrap();

        // Let's create our middleware here
        let rate_limiter = {
            // timeout is 10 secs
            let store = RedisStore::from_client(client, "test-actix-web-rate-limit-2", chrono::Duration::seconds(10));

            // create a default controller which controls request handling.
            let controller = controller::Controller::default()
                .on_success(|_, _, value| {
                    println!("ok, request bypass: {:?}", value);
                });

            // finally, create a middleware and return
            RateLimit::new(
                store,
                3, // max request count is 3 (per 10 secs).
                controller,
            )
        };

        App::new()
            .wrap(rate_limiter)
            .service(web::resource("/hello").route(web::get().to(hello_world)))
            .service(web::resource("/exit").route(web::get().to(exit_program)))
    })
        .bind("0.0.0.0:8080")?
        .run()
        .await?;

    unreachable!()
}

async fn hello_world() -> &'static str {
    "Hello, World!"
}

async fn exit_program() -> &'static str {
    exit(0)
}
