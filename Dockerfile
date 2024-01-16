FROM rust:latest

WORKDIR /usr/src/app
COPY . .
RUN cargo build --release

EXPOSE 8080
CMD PORT=8080 ./target/release/rust-libp2p-chat --seed master-0 --silent