# Rust URL Shortener (RUS) - Task Runner

# List available recipes
default:
    @just --list

# Compile-check standalone mode via Docker
check-standalone:
    docker buildx build --build-arg BUILD_MODE=standalone --target builder -t rus:standalone-check .

# Compile-check saas mode via Docker
check-saas:
    docker buildx build --build-arg BUILD_MODE=saas --target builder -t rus:saas-check .

# Compile-check both modes
check-all: check-standalone check-saas
