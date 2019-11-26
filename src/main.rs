use actix_rt::Arbiter;
use actix_web::error::ErrorInternalServerError;
use actix_web::web::{Bytes, Data, Path};
use actix_web::http::header;
use actix_web::{web, App, Error, HttpResponse, HttpServer, Responder};

use env_logger;
use tokio::prelude::*;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::timer::Interval;

use std::sync::Mutex;
use std::time::{Duration, Instant};

use image;
use rand;
use rand::RngCore;

fn main() {
    env_logger::init();
    let data = Broadcaster::create();

    HttpServer::new(move || {
        App::new()
            .register_data(data.clone())
            .route("/", web::get().to(index))
            .route("/events", web::get().to(new_client))
            .route("/broadcast/{msg}", web::get().to(broadcast))
            .route("/image", web::get().to(send_image))
    })
    .bind("127.0.0.1:8080")
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

fn new_client(broadcaster: Data<Mutex<Broadcaster>>) -> impl Responder {
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
        .streaming(rx)
}

fn broadcast(msg: Path<String>, broadcaster: Data<Mutex<Broadcaster>>) -> impl Responder {
    broadcaster.lock().unwrap().send(&msg.into_inner());

    HttpResponse::Ok().body("msg sent")
}

fn send_image(_: Data<Mutex<Broadcaster>>) -> impl Responder {
    let mut buf = [0u8; 640 * 480];
    rand::thread_rng().fill_bytes(&mut buf);

    let mut out = vec![];
    let mut encoder = image::jpeg::JPEGEncoder::new(&mut out);
    encoder
        .encode(&buf, 640, 480, image::ColorType::Gray(8))
        .unwrap();

    HttpResponse::Ok()
        .header("content-type", "image/jpeg")
        .body(out)
}

struct Broadcaster {
    clients: Vec<Sender<Bytes>>,
}

impl Broadcaster {
    fn create() -> Data<Mutex<Self>> {
        // Data ≃ Arc
        let me = Data::new(Mutex::new(Broadcaster::new()));

        // ping clients every 10 seconds to see if they are alive
        Broadcaster::spawn_ping(me.clone());

        me
    }

    fn new() -> Self {
        Broadcaster {
            clients: Vec::new(),
        }
    }

    fn spawn_ping(me: Data<Mutex<Self>>) {
        let task = Interval::new(Instant::now(), Duration::from_secs(1))
            .for_each(move |_| {
                me.lock().unwrap().remove_stale_clients();
                Ok(())
            })
            .map_err(|e| panic!("interval errored; err={:?}", e));

        Arbiter::spawn(task);
    }

    fn remove_stale_clients(&mut self) {
        let mut ok_clients = Vec::new();

        let mut buf = [0u8; 640 * 480];
        rand::thread_rng().fill_bytes(&mut buf);

        let mut out = vec![];
        let mut encoder = image::jpeg::JPEGEncoder::new(&mut out);
        encoder
            .encode(&buf, 640, 480, image::ColorType::Gray(8))
            .unwrap();

        let mut msg: Vec<u8> = Vec::from(
            format!(
                "--boundarydonotcross\r\ncontent-length:{}\r\ncontent-type:image/jpeg\r\n\r\n",
                out.len()
            )
            .as_bytes(),
        );
        msg.extend(out.iter().clone());

        for client in self.clients.iter() {
            let result = client.clone().try_send(Bytes::from(&msg[..]));

            if let Ok(()) = result {
                ok_clients.push(client.clone());
            }
        }
        self.clients = ok_clients;
    }

    fn new_client(&mut self) -> Client {
        let (tx, rx) = channel(100);

        // tx.clone()
        //     .try_send(Bytes::from("data: connected\n\n"))
        //     .unwrap();

        self.clients.push(tx);
        Client(rx)
    }

    fn send(&self, msg: &str) {
        // let msg = Bytes::from(["data: ", msg, "\n\n"].concat());
        let mut buf = [0u8; 640 * 480];
        rand::thread_rng().fill_bytes(&mut buf);

        let mut out = vec![];
        let mut encoder = image::jpeg::JPEGEncoder::new(&mut out);
        encoder
            .encode(&buf, 640, 480, image::ColorType::Gray(8))
            .unwrap();

        let msg = format!(
            "--boundarydonotcross\r\ncontent-length:{}\r\ncontent-type:image/jpeg\r\n\r\n",
            out.len()
        );
        let mut buf2: Vec<u8> = Vec::from(msg.as_bytes());
        buf2.extend(out.iter().clone());
        buf2.extend("\r\n".as_bytes());

        for client in self.clients.iter() {
            client
                .clone()
                .try_send(Bytes::from(&buf2[..]))
                .unwrap_or(());
        }
    }
}

// wrap Receiver in own type, with correct error type
struct Client(Receiver<Bytes>);

impl Stream for Client {
    type Item = Bytes;
    type Error = Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.0.poll().map_err(ErrorInternalServerError)
    }
}
