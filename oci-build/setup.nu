#!/usr/bin/env nu

# Build script for RUS (Rust URL Shortener)
# Runs inside the Docker builder container during `docker build`.

def main [build_mode: string] {
    # Validate build mode
    if $build_mode not-in ["standalone" "saas"] {
        print $"Error: Invalid build mode '($build_mode)'. Must be 'standalone' or 'saas'."
        exit 1
    }

    print $"Building in ($build_mode) mode..."

    # Install Alpine build dependencies
    print "Installing build dependencies..."
    ^apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static

    # Build with correct feature flags
    if $build_mode == "standalone" {
        print "Running: cargo build --release --features standalone"
        ^cargo build --release --features standalone
    } else {
        print "Running: cargo build --release --no-default-features --features saas"
        ^cargo build --release --no-default-features --features saas
    }

    # Determine binary name based on Cargo.toml [[bin]] entries
    let binary_name = if $build_mode == "standalone" { "rus" } else { "rus-saas" }
    let binary_src = $"/build/target/release/($binary_name)"

    # Copy to output location (cache mounts don't persist in layers)
    print $"Copying ($binary_src) to /build/output/app"
    mkdir /build/output
    cp $binary_src /build/output/app

    print "Build complete!"
}
