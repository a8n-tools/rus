# syntax=docker/dockerfile:1

# Unified Dockerfile for RUS (Rust URL Shortener)
# Supports both standalone and saas modes via BUILD_MODE arg.
#
# Usage:
#   docker build --build-arg BUILD_MODE=standalone -t rus:standalone .
#   docker build --build-arg BUILD_MODE=saas -t rus:saas .

ARG RUST_VERSION=1.91
ARG ALPINE_VERSION=3.21

# === Builder stage ===
FROM rust:${RUST_VERSION}-alpine AS builder

ARG NU_VERSION=0.110.0
ARG BUILD_MODE=standalone

WORKDIR /build

# Install nushell binary
RUN wget -qO- "https://github.com/nushell/nushell/releases/download/${NU_VERSION}/nu-${NU_VERSION}-x86_64-unknown-linux-musl.tar.gz" \
    | tar xz -C /usr/local/bin --strip-components=1 "nu-${NU_VERSION}-x86_64-unknown-linux-musl/nu"

# Copy source files
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY static ./static
COPY oci-build/setup.nu ./

# Build using setup.nu with BuildKit cache mounts
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/build/target \
    nu setup.nu ${BUILD_MODE}

# === Runtime stage ===
FROM alpine:${ALPINE_VERSION}

RUN apk add --no-cache ca-certificates tzdata \
    && adduser -D -u 1001 appuser

WORKDIR /app

COPY --from=builder /build/output/app /app/app
COPY static ./static

RUN mkdir -p /app/data && chown -R appuser:appuser /app

USER appuser

LABEL org.opencontainers.image.source=https://dev.a8n.run/a8n-tools/rus

ENV HOST=0.0.0.0
ENV RUST_LOG=info

EXPOSE 8080

CMD ["/app/app"]
