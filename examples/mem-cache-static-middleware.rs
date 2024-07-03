//! In this example, we create a RateLimiter stored in static with max 100 requests in 1 minute.
//! We use [store::MemStore] as our storage.

use std::process::exit;
use actix_web::{App, HttpServer, web};
use lazy_static::lazy_static;
use actix_rl::{controller, store};
use actix_rl::middleware::RateLimit;

lazy_static! {
    static ref STORE: store::MemStore = store::MemStore::new(1024, chrono::Duration::minutes(1));
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // actix-web simple http-server starts here...
    HttpServer::new(|| {

        // Let's create our middleware here
        let rate_limiter = {

            // create a default controller which controls request handling.
            let controller = controller::Controller::default();

            // finally, create a middleware and return
            let middleware = RateLimit::new(
                STORE.clone(),
                100, // max request count is 100 (per minute).
                controller,
            );

            middleware
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
