FROM ghcr.io/niceguyit/rust-builder-musl:v1.0.0-rust1.94-alpine

RUN apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static git

WORKDIR /app

COPY Cargo.toml Cargo.lock build.rs ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && touch src/lib.rs && cargo build && rm -rf src

COPY static ./static

EXPOSE 4001

CMD ["sh", "-c", "find src -name '*.rs' -exec touch {} + && cargo watch -x run"]
