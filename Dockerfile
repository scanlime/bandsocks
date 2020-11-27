FROM rustlang/rust:nightly
WORKDIR /build
COPY . .
RUN cargo build --workspace --release 2>&1
RUN cargo test 2>&1
