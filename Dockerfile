FROM rustlang/rust:nightly
WORKDIR /build
COPY . .
RUN cargo build --release 2>&1
RUN cargo run --release -- --help 2>&1
RUN cargo run --release -- busybox 2>&1
RUN cargo run --release -- alpine 2>&1
RUN cargo run --release -- ubuntu 2>&1
RUN cargo run --release -- nginx 2>&1
RUN cargo run --release -- postgres 2>&1
RUN cargo run --release -- rust 2>&1
RUN cargo run --release -- golang 2>&1
RUN cargo run --release -- jrottenberg/ffmpeg:4.3.1-scratch38 -- --help 2>&1

