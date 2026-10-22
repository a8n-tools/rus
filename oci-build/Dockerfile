# Dockerfile for RUS (Rust URL Shortener)
#
# Usage:
#   docker build -t rus .                                        # standalone (default)
#   docker build --build-arg BUILD_MODE=saas -t rus-saas .       # saas

ARG BUILD_MODE=standalone

# Build stage
FROM ghcr.io/niceguyit/rust-builder-musl:v1.1.0-rust1.98.1-alpine AS builder

ARG BUILD_MODE
# CI passes the runner's value (DEV-769); `default` lets cargo pick for a local build.
ARG CARGO_BUILD_JOBS=default
# Version metadata baked into the binary (surfaced via /api/version, /health,
# and the startup banner). Pass these from CI; fall back to "unknown" so local
# `docker build` still works.
ARG GIT_TAG=unknown
ARG GIT_HASH=unknown
ARG BUILD_DATE=unknown
ENV GIT_TAG=${GIT_TAG} \
    GIT_HASH=${GIT_HASH} \
    BUILD_DATE=${BUILD_DATE}

WORKDIR /build

# Install build dependencies
RUN apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static

# Copy cargo files for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY build.rs ./

# Create dummy src for dependency compilation
RUN mkdir src && echo "fn main() {}" > src/main.rs && touch src/lib.rs

# Build dependencies only (default features covers standalone deps)
RUN cargo build --release && rm -rf src target/release/deps/rus*

# Copy actual source code
COPY src ./src
COPY static ./static

# Build the binary for the selected mode and copy to a stable path
RUN set -e; \
    if [ "$BUILD_MODE" = "standalone" ]; then \
      cargo build --release --features standalone; \
      cp target/release/rus /build/app; \
    else \
      cargo build --release --no-default-features --features saas; \
      cp target/release/rus-saas /build/app; \
    fi

# Runtime stage
FROM alpine:3.23

WORKDIR /app

RUN apk add --no-cache ca-certificates tzdata gosu shadow \
    && addgroup -g 1001 appuser \
    && adduser -D -H -u 1001 -G appuser appuser

COPY --from=builder /build/app /app/app
COPY static ./static
COPY oci-build/gosu-entrypoint.sh /usr/local/bin/gosu-entrypoint.sh

RUN chmod 0755 /usr/local/bin/gosu-entrypoint.sh \
    && mkdir -p /data /config \
    && chown -R appuser:appuser /app /data /config

LABEL org.opencontainers.image.source=https://dev.a8n.run/a8n-tools/rus

ENV HOST=0.0.0.0
ENV RUST_LOG=info
ENV DB_PATH=/data/rus.db
ENV APP_USER=appuser DATA_DIRS="/data /config"

EXPOSE 4001

# The container starts as root; gosu-entrypoint.sh remaps appuser to PUID/PGID (or the
# first data dir's owner) and drops to it with gosu. A compose `user:` pin disables the
# remap and chown and instead requires every DATA_DIRS entry already be writable.
ENTRYPOINT ["/usr/local/bin/gosu-entrypoint.sh", "/app/app"]
