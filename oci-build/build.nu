#!/usr/bin/env nu

# Build script for Rust applications using buildah and Alpine

export-env {
    # Set the log level and file.
    $env.NU_LOG_LEVEL = "DEBUG"
}

# Load the configuration
def load-config []: [nothing -> any, string -> any] {
    try {
        mut config = ($in | default "config.yml" | open)
        $config.builder.image_url = $"($config.builder.url):($config.builder.version)"
        $config.runtime.image_url = $"($config.runtime.url):($config.runtime.version)"
        $config
    } catch {|err|
        use std log
        log error $"[load-config] Failed to load config: ($err.msg)"
        exit 1
    }
}

# Build stage - compile the Rust application
def build-stage []: any -> any {
    use std log
    mut config = $in
    let build_dir = $config.builder.dir

    log info "========================================\n"
    log info $"[build-stage] Starting build stage using '($config.builder.image_url)'"

    # Create builder container from rust alpine image
    let builder = (^buildah from $config.builder.image_url)
    $config.builder.id = $builder
    log info $"[build-stage] Created builder container: ($builder)"

    # Set working directory
    ^buildah config --workingdir $build_dir $builder

    # Install build dependencies
    log info "[build-stage] Installing build dependencies..."
    ^buildah run $builder -- apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static

    # Copy source files into builder
    let project_root = ($env.FILE_PWD | path dirname)
    log info $"[build-stage] Project root: ($project_root)"

    # Copy Cargo files first for better layer caching
    ^buildah copy $builder ($project_root | path join "Cargo.toml") ($build_dir | path join "Cargo.toml")

    let cargo_lock_path = ($project_root | path join "Cargo.lock")
    if ($cargo_lock_path | path exists) {
        log info "[build-stage] Found Cargo.lock"
        ^buildah copy $builder $cargo_lock_path ($build_dir | path join "Cargo.lock")
    }

    # Create dummy source to cache dependencies
    log info "[build-stage] Caching dependencies..."
    ^buildah run $builder -- sh -c $"mkdir -p ($build_dir)/src && echo 'fn main\(\) {}' > ($build_dir)/src/main.rs"
    ^buildah run $builder -- cargo build --release
    ^buildah run $builder -- rm -rf $"($build_dir)/src"

    # Copy the actual source code
    ^buildah copy $builder ($project_root | path join "src") ($build_dir | path join "src")

    # Copy additional files if they exist
    ["build.rs", ".cargo", "benches", "examples", "tests", "templates", "migrations"]
    | each {|it|
        let item_path = ($project_root | path join $it)
        if ($item_path | path exists) {
            log info $"[build-stage] Copying: ($it)"
            ^buildah copy $builder $item_path ($build_dir | path join $it)
        }
    }

    # Build the application
    log info "[build-stage] Building Rust application..."
    ^buildah run $builder -- cargo build --release

    # Return config
    $config
}

# Runtime stage - create the final slim Alpine image
def runtime-stage []: any -> any {
    use std log
    mut config = $in
    let builder = $config.builder.id
    let project_root = ($env.FILE_PWD | path dirname)
    let app_dir = $config.runtime.dir

    log info "========================================\n"
    log info $"[runtime-stage] Starting runtime stage using '($config.runtime.image_url)'"

    # Create runtime container
    let runtime = (^buildah from $config.runtime.image_url)
    $config.runtime.id = $runtime
    log info $"[runtime-stage] Created runtime container: ($runtime)"

    # Install runtime dependencies
    log info "[runtime-stage] Installing runtime dependencies..."
    ^buildah run $runtime -- apk add --no-cache ca-certificates tzdata

    # Create non-root user
    log info "[runtime-stage] Creating appuser..."
    ^buildah run $runtime -- adduser -D -u 1001 appuser

    # Mount builder to copy build artifacts
    log info "[runtime-stage] Copying build artifacts from builder..."
    let builder_mount = (^buildah mount $builder)
    let runtime_mount = (^buildah mount $runtime)

    # Common directories
    # Note: 'path join' does not see the mount point as a real directory and strips it. Use string interpolation.
    let builder_dir = $"($builder_mount)($config.builder.dir)"
    let runtime_dir = $"($runtime_mount)($config.runtime.dir)"

    # Create application directory in runtime
    log debug $"[runtime-stage] Creating runtime directories: ($runtime_dir)"
    mkdir $runtime_dir

    # Get the binary name from Cargo.toml
    let cargo_toml = (open ($project_root | path join "Cargo.toml"))
    let binary_name = $cargo_toml.package.name
    log info $"[runtime-stage] Binary name: ($binary_name)"

    # Copy the compiled binary
    let binary_src = ($builder_dir | path join $"target/release/($binary_name)")
    let binary_dest = ($runtime_dir | path join "app")

    if ($binary_src | path exists) {
        log info $"[runtime-stage] Copying binary from ($binary_src) to ($binary_dest)"
        cp $binary_src $binary_dest
    } else {
        log error $"[runtime-stage] Binary not found at ($binary_src)"
        ^buildah umount $builder
        ^buildah umount $runtime
        ^buildah rm $builder
        ^buildah rm $runtime
        exit 1
    }

    # Copy any additional runtime assets if they exist
    ["assets", "config", "templates", "static"]
    | each {|dir|
        let src = ($project_root | path join $dir)
        if ($src | path exists) {
            log info $"[runtime-stage] Copying ($dir) directory..."
            cp -r $src ($runtime_dir | path join $dir)
        }
    }

    # Set ownership
    log info "[runtime-stage] Setting ownership to appuser..."
    ^buildah run $runtime -- chown -R appuser:appuser $app_dir

    # Unmount containers
    ^buildah umount $builder
    ^buildah umount $runtime

    # Build config arguments
    let args = [
        --cmd $"/($app_dir)/app"
        --port $config.runtime.cfg.port
        --workingdir $config.runtime.dir
        --user "appuser"
        ...($config.runtime.cfg.env | each {|it| ["--env" $it]} | flatten)
        ...($config.runtime.cfg.labels | each {|it| ["--label" $it]} | flatten)
    ]

    # Configure the runtime container
    log info $"[runtime-stage] Configuring the container"
    log debug $"[runtime-stage] ^buildah config ($args) runtime"
    ^buildah config ...$args $runtime

    # Cleanup builder container
    log info "[runtime-stage] Cleaning up builder container..."
    ^buildah rm $builder

    $config
}

# Publish the image
def publish-image []: any -> any {
    use std log
    let config = $in
    let runtime = $config.runtime.id

    log info "========================================\n"
    log info "[publish-image] Committing and publishing image"

    let image_name = $"($config.published.name):($config.published.version)"
    let docker_image_name = $"docker-daemon:($image_name)"

    # Commit the container as an image
    let image = (^buildah commit --format docker $runtime $image_name)
    log info $"[publish-image] Committed image: ($image_name)"

    # Push to Docker daemon
    ^buildah push $image $docker_image_name
    log info $"[publish-image] Pushed image to Docker: ($docker_image_name)"

    # Cleanup runtime container
    ^buildah rm $runtime

    # Output for CI/CD
    mut output = "output.log"
    if ("GITHUB_OUTPUT" in $env) {
        $output = $env.GITHUB_OUTPUT
    }
    $"image=($config.published.name)\n" | save --append $output
    $"tags=($config.published.version)\n" | save --append $output

    log info $"[publish-image] Build complete: ($image_name)"
    $config
}

# Main entry point
def main [] {
    use std log
    log info "Starting Rust container build..."

    # Check environment for buildah
    use buildah-wrapper.nu *
    $env.BUILD_ARGS = ""
    check-environment

    # Run the build pipeline
    load-config
    | build-stage
    | runtime-stage
    | publish-image

    log info "Build complete!"
}