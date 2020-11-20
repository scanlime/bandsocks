FROM rustlang/rust:nightly
WORKDIR /build
COPY . .
RUN cargo test 2>&1
RUN cargo build --release 2>&1

