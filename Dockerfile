FROM yametech/rust:latest as cargo-build

WORKDIR /usr/src/harvest

ADD . .

# ADD config ${HOME}/.cargo/config

RUN rm -f target/x86_64-unknown-linux-musl/release/deps/harvest*

RUN RUSTFLAGS=-Clinker=musl-gcc cargo build --release --target=x86_64-unknown-linux-musl

FROM alpine:latest

COPY --from=cargo-build /usr/src/harvest/target/x86_64-unknown-linux-musl/release/harvest /usr/local/bin/harvest

ENTRYPOINT ["/usr/local/bin/harvest"]