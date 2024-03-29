FROM rust:latest

WORKDIR /usr/src/app
COPY . .
RUN cargo build --release

EXPOSE 8080
CMD ./target/release/rust-libp2p-chat --port 8080 --seed master-0 --silent