FROM rust:latest
WORKDIR /build
COPY . .
RUN cargo build --release 2>&1
RUN cargo run --release -- --help
RUN cargo run --release -- docker.io/jrottenberg/ffmpeg:4.3.1-scratch38 -- --help
