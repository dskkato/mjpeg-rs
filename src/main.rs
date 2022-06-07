use structopt::StructOpt;

use actix_web::web::Data;
use actix_web::{web, App, HttpResponse, HttpServer, Responder};

#[macro_use]
extern crate log;

use std::sync::Mutex;

mod broadcaster;
use broadcaster::Broadcaster;

#[derive(Debug, StructOpt)]
#[structopt(name = "mjpeg-rs")]
struct Opt {
    #[structopt(short, long, default_value = "320")]
    width: u32,

    #[structopt(short, long, default_value = "180")]
    height: u32,

    #[structopt(short, long, default_value = "30")]
    fps: u64,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let opt = Opt::from_args();

    info!("{:?}", opt);

    let data = Broadcaster::create(opt.width, opt.height, opt.fps);

    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .route("/", web::get().to(index))
            .route("/streaming", web::get().to(new_client))
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}

async fn index() -> impl Responder {
    let content = include_str!("index.html");

    HttpResponse::Ok()
        .append_header(("Content-Type", "text/html"))
        .body(content)
}

/// Register a new client and return a response
async fn new_client(broadcaster: Data<Mutex<Broadcaster>>) -> impl Responder {
    info!("new_client...");
    let rx = broadcaster.lock().unwrap().new_client();

    HttpResponse::Ok()
        .append_header(("Cache-Control", "no-store, must-revalidate"))
        .append_header(("Pragma", "no-cache"))
        .append_header(("Expires", "0"))
        .append_header(("Connection", "close"))
        .append_header((
            "Content-Type",
            "multipart/x-mixed-replace;boundary=boundarydonotcross",
        ))
        .streaming(rx) // now starts streaming
}
