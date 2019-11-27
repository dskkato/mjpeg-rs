use actix_rt::Arbiter;
use actix_web::error::ErrorInternalServerError;
use actix_web::web::{Bytes, Data};
use actix_web::{web, App, Error, HttpResponse, HttpServer, Responder};

use env_logger;
use tokio::prelude::*;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::timer::Interval;

use std::sync::Mutex;
use std::time::{Duration, Instant};

use escapi;
use image;

fn main() {
    env_logger::init();
    let data = Broadcaster::create();

    HttpServer::new(move || {
        App::new()
            .register_data(data.clone())
            .route("/", web::get().to(index))
            .route("/events", web::get().to(new_client))
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

fn send_image(_: Data<Mutex<Broadcaster>>) -> impl Responder {
    const W: u32 = 320;
    const H: u32 = 240;

    let camera = escapi::init(0, W, H, 1).expect("Could not initialize the camera");
    println!("capture initialized, device name: {}", camera.name());

    let (width, height) = (camera.capture_width(), camera.capture_height());
    let pixels = camera.capture().expect("Could not capture an image");

    // Lets' convert it to RGB.
    let mut buffer = vec![0; width as usize * height as usize * 3];
    for i in 0..pixels.len() / 4 {
        buffer[i * 3] = pixels[i * 4 + 2];
        buffer[i * 3 + 1] = pixels[i * 4 + 1];
        buffer[i * 3 + 2] = pixels[i * 4];
    }

    let mut out = vec![];
    let mut encoder = image::jpeg::JPEGEncoder::new(&mut out);
    encoder
        .encode(&buffer, 320, 240, image::ColorType::RGB(8))
        .unwrap();

    HttpResponse::Ok()
        .header("content-type", "image/jpeg")
        .body(out)
}

struct Broadcaster {
    clients: Vec<Sender<Bytes>>,
    camera: escapi::Device,
    out: Vec<u8>,
}

impl Broadcaster {
    fn create() -> Data<Mutex<Self>> {
        // Data ≃ Arc
        let me = Data::new(Mutex::new(Broadcaster::new()));

        // ping clients every 10 seconds to see if they are alive
        Broadcaster::spawn_ping(me.clone());
        Broadcaster::spawn_capture(me.clone());

        me
    }

    fn new() -> Self {
        const W: u32 = 320;
        const H: u32 = 240;

        let camera = escapi::init(0, W, H, 1).expect("Could not initialize the camera");

        Broadcaster {
            clients: Vec::new(),
            camera,
            out: Vec::new(),
        }
    }

    fn spawn_ping(me: Data<Mutex<Self>>) {
        let task = Interval::new(Instant::now(), Duration::from_millis(1000))
            .for_each(move |_| {
                me.lock().unwrap().remove_stale_clients();
                Ok(())
            })
            .map_err(|e| panic!("interval errored; err={:?}", e));

        Arbiter::spawn(task);
    }

    fn spawn_capture(me: Data<Mutex<Self>>) {
        let task = Interval::new(Instant::now(), Duration::from_millis(1000))
            .for_each(move |_| {
                me.lock().unwrap().capture();
                Ok(())
            })
            .map_err(|e| panic!("interval errored; err={:?}", e));

        Arbiter::spawn(task);
    }

    fn capture(&mut self) {
        let camera = &self.camera;
        let (width, height) = (camera.capture_width(), camera.capture_height());
        let pixels = camera.capture().expect("Could not capture an image");
        println!("Captured");

        // Lets' convert it to RGB.
        let mut buffer = vec![0; width as usize * height as usize * 3];
        for i in 0..pixels.len() / 4 {
            buffer[i * 3] = pixels[i * 4 + 2];
            buffer[i * 3 + 1] = pixels[i * 4 + 1];
            buffer[i * 3 + 2] = pixels[i * 4];
        }

        let mut temp = Vec::new();
        let mut encoder = image::jpeg::JPEGEncoder::new(&mut temp);

        encoder
            .encode(&buffer, 320, 240, image::ColorType::RGB(8))
            .unwrap();
        self.out = temp;
        println!("finish");
    }

    fn remove_stale_clients(&mut self) {
        let mut ok_clients = Vec::new();

        let mut msg = Vec::from(
            format!(
                "--boundarydonotcross\r\ncontent-length:{}\r\ncontent-type:image/jpeg\r\n\r\n",
                self.out.len()
            )
            .into_bytes(),
        );
        msg.extend(&self.out);
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

        let mut msg = Vec::from(
            format!(
                "--boundarydonotcross\r\ncontent-length:{}\r\ncontent-type:image/jpeg\r\n\r\n",
                self.out.len()
            )
            .into_bytes(),
        );
        msg.extend(&self.out);
        tx.clone().try_send(Bytes::from(msg)).unwrap();

        self.clients.push(tx);
        Client(rx)
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
