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
    docker compose down --remove-orphans

# Remove dev containers, volumes, networks, and all named Docker volumes
dev-clean-all: dev-clean
    #!/usr/bin/env nu
    let suffix = $env.USER
    let vols = [
        $"rus-cargo-target-($suffix)"
        $"rus-data-($suffix)"
    ]
    let existing = docker volume ls --quiet | lines
    for vol in $vols {
        if $vol in $existing {
            docker volume rm $vol
        }
    }

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

# Release
# Create a release: bump major (vx.0.0) or minor version (v0.x.0), commit, tag, and push
create-release bump:
    #!/usr/bin/env nu
    let bump = "{{ bump }}"
    let current = (open Cargo.toml | get package.version | split row "." | each { into int })
    let next = match $bump {
        "major" => [$"($current.0 + 1)" "0" "0"],
        "minor" => [$"($current.0)" $"($current.1 + 1)" "0"],
        _ => { print $"(ansi red)Usage: just create-release <major|minor>(ansi reset)"; exit 1 }
    }
    let bare = ($next | str join ".")
    let tag = $"v($bare)"
    open Cargo.toml | update package.version $bare | to toml | collect | save --force Cargo.toml
    git add Cargo.toml
    git commit --signoff --message $"Release ($tag)"
    git tag --annotate $tag --message $"Release ($tag)"
    git push --follow-tags
    print $"Released ($tag)"

# Test the release flow: create major release, cancel CI, delete tag, and revert commit (requires FORGEJO_TOKEN)
test-release:
    #!/usr/bin/env nu
    let token = ($env | get --ignore-errors FORGEJO_TOKEN | default "")
    if ($token | is-empty) { print $"(ansi red)FORGEJO_TOKEN env var required(ansi reset)"; exit 1 }
    let current = (open Cargo.toml | get package.version | split row "." | each { into int })
    let bare = $"($current.0 + 1).0.0"
    let tag = $"v($bare)"
    just create-release major
    print "Waiting for CI to pick up the tag..."
    sleep 5sec
    let headers = {Authorization: $"token ($token)"}
    let runs = (http get --headers $headers "https://dev.a8n.run/api/v1/repos/a8n-tools/rus/actions/runs")
    let matched = ($runs.workflow_runs | where prettyref == $tag)
    if ($matched | is-empty) {
        print $"(ansi yellow)No workflow run found for ($tag) — skipping cancel(ansi reset)"
    } else {
        let run_id = ($matched | first | get id)
        try {
            http post --headers $headers --content-type "application/json" $"https://dev.a8n.run/api/v1/repos/a8n-tools/rus/actions/runs/($run_id)/cancel" {}
            print $"Cancelled workflow run ($run_id)"
        } catch {
            print $"(ansi yellow)Could not cancel run ($run_id) — may have already completed(ansi reset)"
        }
    }
    ^git tag --delete $tag
    ^git push origin --delete $tag
    ^git revert --no-edit HEAD
    ^git push
    print $"Done — ($tag) cleaned up"
