FROM yametech/rust:latest as cargo-build

WORKDIR /usr/src/harvest

ADD . .

# ADD config ${HOME}/.cargo/config

RUN cargo build --release && ls -alt /target/release/*

FROM alpine:latest

COPY --from=cargo-build target/release/harvest /usr/local/bin/harvest

ENTRYPOINT ["/usr/local/bin/harvest"]