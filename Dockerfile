FROM rust:latest as cargo-build

RUN apt-get update && apt-get install musl-tools pkg-config openssl libssl-dev gcc -y

RUN rustup target add x86_64-unknown-linux-musl

RUN rustup default nightly

WORKDIR /usr/src/harvest

COPY . .

COPY config ${HOME}/.cargo/config.toml

RUN CARGO_HTTP_MULTIPLEXING=false cargo vendor

# RUN cargo vendor

COPY config.toml ${HOME}/.cargo/config.toml

RUN rm -f target/x86_64-unknown-linux-musl/release/deps/harvest*

RUN RUSTFLAGS=-Clinker=musl-gcc cargo build --release --target=x86_64-unknown-linux-musl

FROM alpine:latest

COPY --from=cargo-build target/x86_64-unknown-linux-musl/release/deps/harvest /usr/local/bin/harvest

ENTRYPOINT ["/usr/local/bin/harvest"]