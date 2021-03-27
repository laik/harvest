FROM rust:latest as cargo-build

RUN apt-get update && apt-get install musl-tools pkg-config openssl libssl-dev -y

RUN rustup target add x86_64-unknown-linux-musl

RUN rustup default nightly

WORKDIR /usr/src/harvest

COPY . .

COPY config ${HOME}/.cargo/config

RUN cargo vendor

RUN rm -f target/x86_64-unknown-linux-musl/release/deps/harvest*

ENV CARGO_HTTP_MULTIPLEXING=false 

RUN RUSTFLAGS=-Clinker=musl-gcc cargo build --release --target=x86_64-unknown-linux-musl

FROM alpine:latest

COPY --from=cargo-build target/x86_64-unknown-linux-musl/release/deps/harvest /usr/local/bin/harvest

ENTRYPOINT ["/usr/local/bin/harvest"]