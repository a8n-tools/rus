# Unified Dockerfile for RUS (Rust URL Shortener)
# Supports both standalone and saas modes via BUILD_MODE arg.
#
# Usage:
#   docker build --build-arg BUILD_MODE=standalone -t rus:standalone .
#   docker build --build-arg BUILD_MODE=saas -t rus:saas .

ARG BUILD_MODE=standalone

# Build stage
FROM rust:1.93-alpine AS builder

ARG BUILD_MODE

WORKDIR /build

# Install build dependencies
RUN apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static

# Copy cargo files for dependency caching
COPY Cargo.toml Cargo.lock ./

# Create dummy src for dependency compilation
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Build dependencies only
RUN cargo build --release && rm -rf src target/release/deps/rus*

# Copy actual source code
COPY src ./src
COPY static ./static

# Build the correct binary based on build mode and copy to a stable path
RUN set -e; \
    if [ "$BUILD_MODE" = "standalone" ]; then \
      cargo build --release --features standalone; \
      cp target/release/rus /build/rus; \
    else \
      cargo build --release --no-default-features --features saas; \
      cp target/release/rus-saas /build/rus; \
    fi

# Runtime stage
FROM alpine:3.21

WORKDIR /app

RUN apk add --no-cache ca-certificates tzdata \
    && adduser -D -u 1001 appuser

COPY --from=builder /build/rus /app/rus
COPY static ./static

RUN mkdir -p /app/data && chown -R appuser:appuser /app

USER appuser

LABEL org.opencontainers.image.source=https://dev.a8n.run/a8n-tools/rus

ENV HOST=0.0.0.0
ENV RUST_LOG=info

EXPOSE 8080

ENTRYPOINT ["/app/rus"]
