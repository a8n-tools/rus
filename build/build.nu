#!/usr/bin/env nu

# Build script for Rust URL Shortener (RUS) using buildah

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

	# Rust image
    log info "========================================\n"
    log info $"[build-stage] Starting build stage using '($config.builder.image_url)'"

    # Create builder container from rust image
    let builder = (^buildah from $config.builder.image_url)
	$config.builder.id = $builder
    log info $"[build-stage] Created builder container: ($builder)"

    # Set working directory
    ^buildah config --workingdir $build_dir $builder

    # Copy source files into builder
    let project_root = ($env.FILE_PWD | path dirname)
    log info $"[build-stage] Project root: ($project_root)"

    ^buildah copy $builder ($project_root | path join "Cargo.toml") ($build_dir | path join "Cargo.toml")
    ^buildah copy $builder ($project_root | path join "Cargo.lock") ($build_dir | path join "Cargo.lock")
    ^buildah copy $builder ($project_root | path join "src") ($build_dir | path join "src")
    ^buildah copy $builder ($project_root | path join "static") ($build_dir | path join "static")

    # Build the application
    log info "[build-stage] Building Rust application..."
    ^buildah run $builder -- cargo build --release

    # Return config
    $config
}

# Runtime stage - create the final slim image
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
    # log info "[runtime-stage] Installing runtime dependencies..."
    # ^buildah run $runtime -- sh -c "apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*"

    # Mount builder to copy binary
    log info "[runtime-stage] Copying binary from builder..."
    let builder_mount = (^buildah mount $builder)
    let runtime_mount = (^buildah mount $runtime)

	# Common directories
	# Note: 'path join' does not see the mount point as a real directory and strips it. Use string interpolation.
	let builder_dir = $"($builder_mount)($config.builder.dir)"
	let runtime_dir = $"($runtime_mount)($config.runtime.dir)"

    # Create application and data directories in runtime
    log debug $"[runtime-stage] Creating runtime directories: ($runtime_dir)"
	mkdir $runtime_dir
    mkdir ($runtime_dir | path join "data")

    # Copy the compiled binary
	let src = ($builder_dir | path join "target/release/rus")
	let dest = ($runtime_dir | path join "rus")
    log debug $"[runtime-stage] Copying rus binary: ($src)"
	cp $src $dest

    # Copy static files
	let src_dir = ($project_root | path join "static")
	let dest_dir = ($runtime_dir | path join "static")
    log debug $"[runtime-stage] Copying static files: ($src_dir)"
    cp -r $src_dir $dest_dir

    # Unmount containers
    ^buildah umount $builder
    ^buildah umount $runtime

    # Set working directory
    # ^buildah config --workingdir $app_dir $runtime

	let args = [
		--cmd ($app_dir | path join "rus")
		--port $config.runtime.cfg.port
		--workingdir $config.runtime.dir
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
    log info "Starting RUS container build..."

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
