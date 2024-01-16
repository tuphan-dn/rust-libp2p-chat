FROM rust:latest

ARG PORT=8080

WORKDIR /usr/src/app
COPY . .
RUN cargo build --release

EXPOSE $PORT
CMD PORT=$PORT ./target/release/rust-libp2p-chat --seed master-0