# Live streaming server with Rust/actix-web

This repository demonstrates how to use actix-web's streaming for live streaming, so called Motion JPEG.

## Pre-requirement

A web-camera must be connected.

## Suppoted environment

Currentlry, Windows and macos is supported.

This constrain comes from Camera driver, so one can use this repository by using appropriate camera driver, like L4V2 in Linux envirnment.

## How to use

**Windows user**

```
cargo run --release
```

**Mac user**

You need to install OpenCV 4.1, because of the `opencv` crate's dependencies.

```
brew install opencv
```

Then, run.

```
cargo run --release
```

Then, access to [http://127.0.0.1:8080](http://127.0.0.1:8080), or [http://127.0.0.1:8080/streaming](http://127.0.0.1:8080/streaming) from a web-browse (except IE).

