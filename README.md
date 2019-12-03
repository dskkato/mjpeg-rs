# Live streaming server with Rust/actix-web

Live streaming server based on MJPEG over HTTP.

## Pre-requirement

A web-camera must be connected.

## Suppoted environment

Currentlry, Windows and macos are supported.

This constrain comes from Camera libraries that I can use.

## How to use

Since this module consists of a little image processing, release build is recomendded.

**Windows user**

There are no external dependencies, except for Web-camera.

```
cargo run --release
```

**Mac user**

You may need to install OpenCV 4.1, because of the `opencv` crate's dependencies.

```
brew install opencv
```

And run. Currently, switching camera resolution was failed and only (1280, 720) might work properly for built-in MBP Camera.

```
cargo run --release -- -w 1280 -h 720
```

Then, access to [http://127.0.0.1:8080](http://127.0.0.1:8080), or [http://127.0.0.1:8080/streaming](http://127.0.0.1:8080/streaming) from a web-browse (except IE).

