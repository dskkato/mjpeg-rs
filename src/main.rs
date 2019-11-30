use actix_web::web::Data;
use actix_web::{web, App, HttpResponse, HttpServer, Responder};

#[macro_use]
extern crate log;
use env_logger;

use std::sync::Mutex;

mod broadcaster;
use broadcaster::Broadcaster;

fn main() {
    env_logger::init();
    let data = Broadcaster::create();

    HttpServer::new(move || {
        App::new()
            .register_data(data.clone())
            .route("/", web::get().to(index))
            .route("/streaming", web::get().to(new_client))
    })
    .bind("0.0.0.0:8080")
    .expect("Unable to bind port")
    .run()
    .unwrap();
}

fn index() -> impl Responder {
    let content = include_str!("index.html");

    HttpResponse::Ok()
        .header("Content-Type", "text/html")
        .body(content)
}

/// Register a new client and return a response
fn new_client(broadcaster: Data<Mutex<Broadcaster>>) -> impl Responder {
    info!("new_client...");
    let rx = broadcaster.lock().unwrap().new_client();

    HttpResponse::Ok()
        .header("Cache-Control", "no-store, must-revalidate")
        .header("Pragma", "no-cache")
        .header("Expires", "0")
        .header("Connection", "close")
        .header(
            "Content-Type",
            "multipart/x-mixed-replace;boundary=boundarydonotcross",
        )
        .no_chunking()
        .streaming(rx) // now starts streaming
}
