FROM rust:latest as cargo-build

RUN apt-get update && apt-get install musl-tools pkg-config openssl libssl-dev -y

RUN rustup target add x86_64-unknown-linux-musl

WORKDIR /usr/src/harvest

ADD . .

COPY config ${HOME}/.cargo/config

RUN rm -f target/release/harvest*

RUN rustup default nightly

RUN CARGO_HTTP_MULTIPLEXING=false cargo build --release

FROM alpine:latest

COPY --from=cargo-build /usr/src/harvest/target/release/harvest /usr/local/bin/harvest

ENTRYPOINT ["/usr/local/bin/harvest"]