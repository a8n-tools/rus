# Rust URL Shortener (RUS) - Task Runner

# List available recipes
default:
    @just --list

# Use the per-developer Traefik-routed dev compose file
compose := "docker compose -f compose.dev.yml "

# Install the git pre-commit hook (run once per fresh clone). Writes a stub at .git/hooks/pre-commit that execs `just pre-commit`. Bypass with `git commit --no-verify`.
install-hooks:
    #!/usr/bin/env nu
    let hook = ".git/hooks/pre-commit"
    # Remove first so a leftover symlink from an older install does not get
    # written through to its target file. `try` swallows the not-found case.
    try { rm $hook }
    "#!/usr/bin/env sh\nexec just pre-commit\n" | save $hook
    ^chmod +x $hook
    print $"Wrote ($hook) -> just pre-commit"

# Run the same checks as .forgejo/workflows/check.yml inside the dev compose `app` container, exercising both `standalone` (default) and `saas` feature modes.
pre-commit: ensure-env
    #!/usr/bin/env nu
    print "\n[pre-commit] cargo fmt --check"
    ^docker compose -f compose.dev.yml run --rm --no-deps app cargo fmt --check
    print "\n[pre-commit] cargo clippy --all-targets -- -D warnings (standalone)"
    ^docker compose -f compose.dev.yml run --rm --no-deps app cargo clippy --all-targets -- -D warnings
    print "\n[pre-commit] cargo clippy --all-targets --no-default-features --features saas -- -D warnings"
    ^docker compose -f compose.dev.yml run --rm --no-deps app cargo clippy --all-targets --no-default-features --features saas -- -D warnings
    print "\n[pre-commit] cargo build --all-targets (standalone)"
    ^docker compose -f compose.dev.yml run --rm --no-deps app cargo build --all-targets
    print "\n[pre-commit] cargo build --all-targets --no-default-features --features saas"
    ^docker compose -f compose.dev.yml run --rm --no-deps app cargo build --all-targets --no-default-features --features saas
    print "\n[pre-commit] cargo test --lib (standalone)"
    ^docker compose -f compose.dev.yml run --rm --no-deps -e JWT_SECRET=test-secret-at-least-32-chars-ok! app cargo test --lib
    print "\n[pre-commit] all checks passed"

# Copy the appropriate .env file for the given mode
[private]
ensure-env mode="standalone":
    @cp .env.{{ mode }} .env

# Build and start dev server with Traefik routing on a8n.run (mode: standalone or saas)
dev mode="standalone": (ensure-env mode)
    #!/usr/bin/env nu
    let git_tag = (^git describe --tags --always --dirty | str trim)
    let git_hash = (^git rev-parse --short=12 HEAD | str trim)
    let build_date = (date now | format date "%Y-%m-%dT%H:%M:%SZ")
    BUILD_MODE={{ mode }} GIT_TAG=$git_tag GIT_HASH=$git_hash BUILD_DATE=$build_date {{ compose }}up --build --detach app
    print ""
    print "Service started!"
    print $"  App: https://($env.USER)-rus.a8n.run"

# Build and start local dev server in Docker (cargo-watch, localhost:4001)
dev-local: (ensure-env "standalone")
    docker compose up --build --detach app
    @echo ""
    @echo "Service started!"
    @echo "  App: http://localhost:4001"

# Stop every dev stack started by `just dev` / `just dev-local` (Traefik + localhost)
down:
    {{ compose }}down --remove-orphans
    docker compose down --remove-orphans

# Tail logs for the Traefik-routed dev container
logs:
    {{ compose }}logs --follow app

# Tail logs for the localhost dev container
logs-local:
    docker compose logs --follow app

# Remove Traefik-routed dev containers and volumes
dev-clean:
    {{ compose }}down --remove-orphans

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

