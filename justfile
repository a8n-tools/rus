# Rust URL Shortener (RUS) - Task Runner

# List available recipes
default:
    @just --list

# Create .env from .env.standalone if it doesn't exist
[private]
ensure-env:
    @test -f .env || cp .env.standalone .env

# Start dev server in Docker
dev: ensure-env
    docker compose up --build app

# Start dev server in Docker (detached)
dev-detach: ensure-env
    docker compose up --build --detach app

# Stop dev containers
dev-stop:
    docker compose down

# Remove dev containers, volumes, and networks
dev-clean:
    docker compose down --volumes --remove-orphans

# Run in standalone mode (debug)
run: ensure-env
    cargo run

# Run in saas mode (debug)
run-saas:
    cargo run --no-default-features --features saas

# Production build (standalone)
build:
    cargo build --release

# Production build (saas)
build-saas:
    cargo build --release --no-default-features --features saas

# Run tests (standalone)
test:
    cargo test

# Run tests (saas)
test-saas:
    cargo test --no-default-features --features saas

# Lint with Clippy (standalone)
lint:
    cargo clippy

# Lint with Clippy (saas)
lint-saas:
    cargo clippy --no-default-features --features saas

# Format code
fmt:
    cargo fmt

# Type-check without building (standalone)
typecheck:
    cargo check

# Type-check without building (saas)
typecheck-saas:
    cargo check --no-default-features --features saas

# Run all checks (Docker builds)
check: check-docker

# Build Docker image for validation (both modes)
check-docker: check-docker-standalone check-docker-saas

# Build Docker image for validation (standalone)
check-docker-standalone:
    docker buildx build --build-arg BUILD_MODE=standalone --target builder -t rus:standalone-check -f oci-build/Dockerfile .

# Build Docker image for validation (saas)
check-docker-saas:
    docker buildx build --build-arg BUILD_MODE=saas --target builder -t rus:saas-check -f oci-build/Dockerfile .

# Build Docker image (mode: standalone or saas)
build-docker mode="standalone":
    docker buildx build --build-arg BUILD_MODE={{ mode }} -t rus:local -f oci-build/Dockerfile .
