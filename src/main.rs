use actix_web::error::ErrorInternalServerError;
use actix_web::web::{Bytes, Data};
use actix_web::{web, App, Error, HttpResponse, HttpServer, Responder};

use env_logger;
use tokio::prelude::*;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use std::sync::Mutex;

use escapi;
use image;

const FRAME_RATE: u64 = 30;
const WIDTH: u32 = 320;
const HEIGHT: u32 = 240;

fn main() {
    env_logger::init();
    let data = Broadcaster::create();

    HttpServer::new(move || {
        App::new()
            .register_data(data.clone())
            .route("/", web::get().to(index))
            .route("/events", web::get().to(new_client))
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

struct Broadcaster {
    clients: Vec<Sender<Bytes>>,
}

impl Broadcaster {
    fn create() -> Data<Mutex<Self>> {
        // Data ≃ Arc
        let me = Data::new(Mutex::new(Broadcaster::new()));

        Broadcaster::spawn_capture(me.clone());

        me
    }

    fn new() -> Self {
        Broadcaster {
            clients: Vec::new(),
        }
    }

    fn remove_stale_clients(&mut self, msg: &[u8]) {
        let mut ok_clients = Vec::new();
        for client in self.clients.iter() {
            let result = client.clone().try_send(Bytes::from(&msg[..]));

            if let Ok(()) = result {
                ok_clients.push(client.clone());
            }
        }
        self.clients = ok_clients;
    }

    fn spawn_capture(me: Data<Mutex<Self>>) {
        let camera =
            escapi::init(0, WIDTH, HEIGHT, FRAME_RATE).expect("Could not initialize the camera");
        let (width, height) = (camera.capture_width(), camera.capture_height());

        std::thread::spawn(move || {
            loop {
                let pixels = camera.capture();

                let buffer = match pixels {
                    Ok(pixels) => {
                        // Lets' convert it to RGB.
                        let mut buffer = vec![0; width as usize * height as usize * 3];
                        for i in 0..pixels.len() / 4 {
                            buffer[i * 3] = pixels[i * 4 + 2];
                            buffer[i * 3 + 1] = pixels[i * 4 + 1];
                            buffer[i * 3 + 2] = pixels[i * 4];
                        }

                        buffer
                    }
                    _ => {
                        println!("failed to capture");
                        vec![0; width as usize * height as usize * 3]
                    }
                };

                let mut temp = Vec::new();
                let mut encoder = image::jpeg::JPEGEncoder::new(&mut temp);
                encoder
                    .encode(&buffer, 320, 240, image::ColorType::RGB(8))
                    .unwrap();

                let mut msg = format!(
                    "--boundarydonotcross\r\nContent-Length:{}\r\nContent-Type:image/jpeg\r\n\r\n",
                    temp.len()
                )
                .into_bytes();
                msg.extend(&temp);
                me.lock().unwrap().remove_stale_clients(&msg);
            }
        });
    }

    fn new_client(&mut self) -> Client {
        let (tx, rx) = channel(100);

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
