FROM rust:1.93-alpine

RUN apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static
RUN cargo install cargo-watch

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build && rm -rf src

COPY static ./static

EXPOSE 8080

CMD ["sh", "-c", "find src -name '*.rs' -exec touch {} + && cargo watch -x run"]
