# Build stage
FROM rust:1.91 as builder

WORKDIR /app

# Copy all source files
COPY Cargo.toml ./
COPY src ./src
COPY static ./static

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install required runtime dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /app/target/release/rus /app/rus

# Copy static files
COPY static ./static

# Create data directory
RUN mkdir -p /app/data

# Expose port
EXPOSE 8080

# Set environment variables
ENV HOST=0.0.0.0
ENV RUST_LOG=info

# Run the application
CMD ["/app/rus"]
