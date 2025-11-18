# Chunk 40: Dockerfile Updates

## Context
Building on compose.yml. Update Dockerfile for health checks and optimization.

## Goal
Update Dockerfile with curl for health checks and optimize build.

## Prompt

```text
I have compose.yml with health check. Now update Dockerfile.

Replace Dockerfile:

```dockerfile
# Build stage
FROM rust:1.83 AS builder

WORKDIR /app

# Copy Cargo files first for dependency caching
COPY Cargo.toml Cargo.lock ./

# Create dummy main to cache dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy actual source
COPY src ./src
COPY static ./static

# Build real application (dependencies already cached)
RUN touch src/main.rs && cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 -s /bin/bash appuser

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/rus /app/rus

# Copy static files
COPY static ./static

# Create data directory with correct permissions
RUN mkdir -p /app/data && \
    chown -R appuser:appuser /app

# Switch to non-root user
USER appuser

# Expose port
EXPOSE 8080

# Environment defaults
ENV HOST=0.0.0.0
ENV PORT=8080
ENV RUST_LOG=info

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:${PORT}/health || exit 1

# Run the application
CMD ["/app/rus"]
```

Key improvements:

1. **Dependency caching**:
   - Copy Cargo.toml first
   - Build dummy project to cache deps
   - Real build is faster on code changes

2. **Security**:
   - Non-root user (appuser)
   - Minimal base image (debian:bookworm-slim)
   - Only necessary packages installed

3. **Health check**:
   - Built into image
   - Uses /health endpoint
   - Same settings as compose

4. **Optimization**:
   - --no-install-recommends reduces size
   - Clean up apt lists
   - Multi-stage keeps final image small

5. **Permissions**:
   - Data directory owned by appuser
   - Can write database file

The image should be:
- Secure (non-root, minimal packages)
- Fast to rebuild (dependency caching)
- Small (~100-150MB)
- Self-documenting (environment variables)
- Health-checkable (curl available)
```

## Expected Output
- Multi-stage build maintained
- Dependency caching added
- curl installed for health checks
- Non-root user created
- Proper file permissions
- Health check in Dockerfile
- Minimal image size
- Environment defaults set
