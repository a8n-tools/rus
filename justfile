# Rust URL Shortener (RUS) - Task Runner

# List available recipes
default:
    @just --list

# Copy the appropriate .env file for the given mode
[private]
ensure-env mode="standalone":
    @cp .env.{{ mode }} .env

# Start dev server with Traefik routing (mode: standalone or saas)
dev mode="standalone": (ensure-env mode)
    BUILD_MODE={{ mode }} docker compose -f compose.dev.yml up --build app

# Start dev server with Traefik routing, detached (mode: standalone or saas)
dev-detach mode="standalone": (ensure-env mode)
    BUILD_MODE={{ mode }} docker compose -f compose.dev.yml up --build --detach app

# Stop Traefik-routed dev containers
dev-stop:
    docker compose -f compose.dev.yml down

# Remove Traefik-routed dev containers and volumes
dev-clean:
    docker compose -f compose.dev.yml down --remove-orphans

# Start local dev server in Docker (cargo-watch, localhost:4001)
dev-local: (ensure-env "standalone")
    docker compose up --build app

# Start local dev server in Docker, detached
dev-local-detach: (ensure-env "standalone")
    docker compose up --build --detach app

# Stop local dev containers
dev-local-stop:
    docker compose down

# Remove local dev containers and volumes
dev-local-clean:
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

# Run tests (standalone) inside Docker
test:
    docker compose run --rm --no-deps -e JWT_SECRET=test-secret-at-least-32-chars-ok! app sh -c "cargo test --features standalone 2>&1"

# Run tests (saas) inside Docker
test-saas:
    docker compose run --rm --no-deps -e SAAS_JWT_SECRET=test-saas-secret-32-chars-padded! app sh -c "cargo test --no-default-features --features saas 2>&1"

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

# ── Release ──────────────────────────────────────────────────────────────────

# Create a release: bump major (vx.0.0), minor (v0.x.0), or hotfix (v0.0.x) version, commit, tag, and push
# After the PR is merged, the create-release workflow creates the tag and release automatically
create-release bump:
    #!/usr/bin/env nu
    let bump = "{{ bump }}"

    # Abort if there are uncommitted changes
    let status = git status --porcelain | str trim
    if ($status | is-not-empty) {
        print $"(ansi red)Working tree is dirty. Please stash or commit your changes first.(ansi reset)"
        exit 1
    }

	# Switch to main if not already there
	let branch = git branch --show-current | str trim
	if $branch != "main" {
		print $"Switching from ($branch) to main..."
        git checkout main
    }

    # Pull latest changes
    git pull --rebase origin main

	# Calculate next version
	let current = (open Cargo.toml | get package.version | split row "." | each { into int })
    let next = match $bump {
        "major" => [$"($current.0 + 1)" "0" "0"],
        "minor" => [$"($current.0)" $"($current.1 + 1)" "0"],
        "hotfix" => [$"($current.0)" $"($current.1)" $"($current.2 + 1)"],
		_ => { print $"(ansi red)Usage: just create-release <major|minor|hotfix>(ansi reset)"; exit 1 }
    }
    let bare = ($next | str join ".")
    let tag = $"v($bare)"
    let release_branch = $"release/($tag)"

    # Create release branch, bump version, and commit
    git checkout -b $release_branch
    open Cargo.toml | update package.version $bare | to toml | collect | save --force Cargo.toml
    git add Cargo.toml
    git commit --signoff --message $"Release ($tag)"

    # Push release branch
    git push --set-upstream origin $release_branch

    # Print PR and release links
    let remote = git remote get-url origin
    let base_url = if ($remote | str starts-with "ssh://") {
        $remote | str replace "ssh://git@" "https://" | str replace "git.a8n.run" "dev.a8n.run" | str replace ".git" ""
    } else {
        $remote | str replace --regex "git@([^:]+):" "https://$1/" | str replace "git.a8n.run" "dev.a8n.run" | str replace ".git" ""
    }
    print $"(ansi green)Pushed ($release_branch)(ansi reset)"
    print $"Create PR: ($base_url)/compare/main...($release_branch)"
    print $"After merging, the create-release workflow will tag and release ($tag) automatically."


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
