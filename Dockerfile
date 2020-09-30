FROM rust:latest
WORKDIR /build
COPY . .
RUN cargo build --release 2>&1
RUN cargo run --release -- --help
RUN cargo run --release -- busybox
RUN cargo run --release -- alpine
RUN cargo run --release -- ubuntu
RUN cargo run --release -- nginx
RUN cargo run --release -- postgres
RUN cargo run --release -- rust
RUN cargo run --release -- golang
RUN cargo run --release -- docker.io/jrottenberg/ffmpeg:4.3.1-scratch38 -- --help
