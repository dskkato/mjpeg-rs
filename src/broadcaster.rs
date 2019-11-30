use actix_web::error::ErrorInternalServerError;
use actix_web::web::{Bytes, Data};
use actix_web::Error;

use tokio::prelude::*;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use std::sync::Mutex;

#[cfg(target_os = "windows")]
use escapi;

use image;

#[cfg(target_os = "macos")]
use opencv;
#[cfg(target_os = "macos")]
use opencv::videoio;

const FRAME_RATE: u64 = 30;

/// Hold clients channels
pub struct Broadcaster {
    clients: Vec<Sender<Bytes>>,
}

impl Broadcaster {
    fn new() -> Self {
        Broadcaster {
            clients: Vec::new(),
        }
    }

    pub fn create() -> Data<Mutex<Self>> {
        // Data ≃ Arc
        let me = Data::new(Mutex::new(Broadcaster::new()));

        Broadcaster::spawn_capture(me.clone());

        me
    }

    pub fn new_client(&mut self) -> Client {
        let (tx, rx) = channel(100);

        self.clients.push(tx);
        Client(rx)
    }

    fn send_image(&mut self, msg: &[u8]) {
        let mut ok_clients = Vec::new();
        for client in self.clients.iter() {
            let result = client.clone().try_send(Bytes::from(&msg[..]));

            if let Ok(()) = result {
                ok_clients.push(client.clone());
            }
        }
        self.clients = ok_clients;
    }

    #[cfg(target_os = "windows")]
    fn spawn_capture(me: Data<Mutex<Self>>) {
        #[cfg(target_os = "windows")]
        const WIDTH: u32 = 320;
        #[cfg(target_os = "windows")]
        const HEIGHT: u32 = 180;
        std::thread::spawn(move || {
            let camera = escapi::init(0, WIDTH, HEIGHT, FRAME_RATE)
                .expect("Could not initialize the camera");
            let (width, height) = (camera.capture_width(), camera.capture_height());

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
                        warn!("failed to capture");
                        vec![0; width as usize * height as usize * 3]
                    }
                };

                let mut temp = Vec::new();
                let mut encoder = image::jpeg::JPEGEncoder::new(&mut temp);
                encoder
                    .encode(&buffer, WIDTH, HEIGHT, image::ColorType::RGB(8))
                    .unwrap();

                let mut msg = format!(
                    "--boundarydonotcross\r\nContent-Length:{}\r\nContent-Type:image/jpeg\r\n\r\n",
                    temp.len()
                )
                .into_bytes();
                msg.extend(&temp);
                me.lock().unwrap().send_image(&msg);
            }
        });
    }

    #[cfg(target_os = "macos")]
    fn spawn_capture(me: Data<Mutex<Self>>) {
        #[cfg(target_os = "macos")]
        const WIDTH: u32 = 1280;
        #[cfg(target_os = "macos")]
        const HEIGHT: u32 = 720;
        std::thread::spawn(move || {
            let mut cam = videoio::VideoCapture::new_with_backend(0, videoio::CAP_ANY).unwrap(); // 0 is the default camera
            let opened = videoio::VideoCapture::is_opened(&cam).unwrap();
            cam.set(videoio::CAP_PROP_FRAME_WIDTH, WIDTH as f64)
                .unwrap();
            cam.set(videoio::CAP_PROP_FRAME_HEIGHT, HEIGHT as f64)
                .unwrap();
            cam.set(videoio::CAP_PROP_FPS, FRAME_RATE as f64).unwrap();
            cam.set(videoio::CAP_PROP_CONVERT_RGB, 1 as f64).unwrap();

            info!(
                "{}, {}, {}",
                cam.get(videoio::CAP_PROP_FRAME_WIDTH).unwrap(),
                cam.get(videoio::CAP_PROP_FRAME_HEIGHT).unwrap(),
                cam.get(videoio::CAP_PROP_FPS).unwrap()
            );

            if !opened {
                panic!("Unable to open default camera!");
            }
            loop {
                let mut frame = opencv::core::Mat::default().unwrap();
                cam.read(&mut frame).unwrap();

                let mut temp = Vec::new();
                let mut encoder = image::jpeg::JPEGEncoder::new(&mut temp);
                unsafe {
                    let mut samples = Vec::from(std::slice::from_raw_parts(
                        frame.data().unwrap() as *const u8,
                        (WIDTH * HEIGHT * 3) as usize,
                    ));
                    for i in 0..(WIDTH * HEIGHT) {
                        samples.swap((i * 3) as usize, (i * 3 + 2) as usize);
                    }
                    encoder
                        .encode(&samples, WIDTH, HEIGHT, image::ColorType::RGB(8))
                        .unwrap();
                }

                let mut msg = format!(
                    "--boundarydonotcross\r\nContent-Length:{}\r\nContent-Type:image/jpeg\r\n\r\n",
                    temp.len()
                )
                .into_bytes();
                msg.extend(&temp);
                me.lock().unwrap().send_image(&msg);
            }
        });
    }
}

// wrap Receiver in own type, with correct error type
pub struct Client(Receiver<Bytes>);

impl Stream for Client {
    type Item = Bytes;
    type Error = Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.0.poll().map_err(ErrorInternalServerError)
    }
}
