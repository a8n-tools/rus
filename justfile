# Rust URL Shortener (RUS) - Task Runner

# SPEC-3.2, SPEC-3.5: required because the default recipe below redefines one common.just also defines.
set allow-duplicate-recipes := true

# SPEC-3.1: set before the import line below, per SPEC-1.5.
app := "rus"

# RUS-47: route the fmt-only pre-commit/pre-push leg through a `docker run`
# of the builder image rather than `docker compose run`, because the
# compose `app` service builds the production runtime image (alpine +
# server ENTRYPOINT, no cargo) and cannot run `cargo fmt`.
pre_commit_mode := "docker"

# Image used by the fmt leg (common.just's `pre-commit-docker`) and inherited
# by this repo's own `check-ci` for every feature leg. Matches the `builder`
# stage in oci-build/Dockerfile so local checks use the same toolchain rus
# is built against.
# Set before the import line (SPEC-3.1, SPEC-3.4) since common.just also defines `dev_image`.
dev_image := "ghcr.io/niceguyit/rust-builder-musl:v1.1.0-rust1.98.1-alpine"

# RUS-47: common's `pre-push` only runs fmt + em-dash + dprint. The heavy
# per-leg matrix (source-tree guard, clippy and build for both feature
# legs, guarded cargo tests, static page tests) lives in `check-ci`
# below and runs on every push through this hook.
pre_push_checks := "check-ci"

# RUS-47: single-crate layout for common's `create-release` dispatcher.
# `release_crate` defaults to `app` ("rus"), so it is not set.
release_layout := "generic"

import 'common/common.just'

# List available recipes. Keep FIRST: just picks the default recipe by source order (SPEC-1.6, SPEC-3.2).
default:
    @just --list

# Use the per-developer Traefik-routed dev compose file
compose := "docker compose -f compose.dev.yml "

# Image for the static/ page tests. The rust-builder image ships no Node, so the
# JS step gets its own pinned image; docker is already required by pre-commit, so
# a developer with no Node on the host still runs the same check.
node_image := "node:24-alpine"

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

# Run one feature leg's tests, all targets, through the empty-run guard (RUS-26).
# Same builder image and caches as `just pre-commit`, so the target volume stays
# warm and no .env, database or Node runtime is needed. NOT the compose `app`
# service: its image ships a dummy-crate target tree whose artifacts look newer
# than the mounted sources, so cargo skips rebuilding the library.
[private]
test-leg leg:
    #!/usr/bin/env nu
    let img = "{{ dev_image }}"
    let user_name = (^whoami | str trim)
    let target_vol = $"dev-rus-target-($user_name)"
    let reg_vol = "dev-rus-cargo-registry"
    let vols = ["--volume" $"($env.PWD):/build" "--workdir" "/build" "--volume" $"($target_vol):/build/target" "--volume" $"($reg_vol):/usr/local/cargo/registry"]
    let secrets = ["--env" "JWT_SECRET=test-secret-at-least-32-chars-ok!"]
    let runner = (["docker" "run" "--rm"] | append $vols | append $secrets | append $img | str join " ")
    ^nu scripts/check-cargo-tests-ran.nu --self-test
    ^nu scripts/check-cargo-tests-ran.nu --leg "{{ leg }}" --runner $runner

# Run tests (standalone) inside Docker
test: (test-leg "standalone")

# Run tests (saas) inside Docker
test-saas: (test-leg "saas")

# Run the static/ page tests (static/tests) in a pinned Node container
test-js:
    #!/usr/bin/env nu
    ^docker run --rm --volume $"($env.PWD):/build:ro" --workdir /build "{{ node_image }}" node static/tests/run.mjs

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

# ── Cleanup ──────────────────────────────────────────────────────────────────

# Tear down this repo's dev footprint: stop both dev stacks (the Traefik-routed compose.dev.yml stack and the localhost compose.yml stack, dropping their default networks), remove this repo's named volumes (rus-cargo-target-$USER, rus-data-$USER), and delete local build/runtime artifacts (target/, data/). Scoped to this repo; safe on a shared host.
[group: 'cleanup']
dev-clean:
    #!/usr/bin/env nu
    {{ compose }}down --remove-orphans
    docker compose down --remove-orphans
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
    let paths = [target data]
    for p in $paths {
        if ($p | path exists) {
            rm --recursive $p
            print $"removed ($p)"
        }
    }
    print "dev-clean: done"

# Everything dev-clean does, plus remove the Docker images this repo builds and prune its buildx cache. Run for a from-scratch rebuild.
[group: 'cleanup']
dev-clean-all: dev-clean
    #!/usr/bin/env nu
    let images = [
        "rus:standalone-check"
        "rus:saas-check"
        "rus:local"
    ]
    for img in $images {
        let present = (do { ^docker image inspect $img } | complete).exit_code == 0
        if $present {
            docker image rm $img
        }
    }
    docker buildx prune --force
    print "dev-clean-all: done"

# ── Release ──────────────────────────────────────────────────────────────────

# RUS-47: `create-release` now comes from common.just via `release_layout
# := "generic"` (set above). Common bumps `[package] version` in Cargo.toml,
# opens the release PR via `fj --host dev.a8n.run pr create`, and runs the
# same preflight checks the old local recipe did. The automatic tag + image
# publish still runs through .forgejo/workflows/create-release.yml on merge.

# ── Hooks ──────────────────────────────────────────────────────────────────

# RUS-47: `install-hooks`, `pre-commit` and the fmt-only leg of `pre-push` now
# come from common.just. The heavy per-leg check matrix (source-tree guard,
# clippy and build per feature leg, guarded cargo tests, static page tests)
# lives in `check-ci` below, which `pre_push_checks := "check-ci"` chains
# into common's `pre-push` after fmt / em-dash / dprint.

# Run every CI check: source-tree guard, clippy and build for standalone and
# saas, the guarded library tests (lib leg only), and the static/ page tests.
# `pre_push_checks := "check-ci"` (above) wires this into common's `pre-push`
# so it fires on every push. Also runnable on its own locally.
[group: 'check']
check-ci:
    #!/usr/bin/env nu
    let img = "{{ dev_image }}"
    let user_name = (^whoami | str trim)
    let target_vol = $"dev-rus-target-($user_name)"
    let reg_vol = "dev-rus-cargo-registry"
    let vols = ["--volume" $"($env.PWD):/build" "--workdir" "/build" "--volume" $"($target_vol):/build/target" "--volume" $"($reg_vol):/usr/local/cargo/registry"]
    # Static and instant, so a source tree that drifted from src/ fails before
    # anything is compiled. --self-test first, same reason as the cargo guard.
    print "\n[check-ci] source tree vs src/"
    ^nu scripts/check-source-tree.nu --self-test
    ^nu scripts/check-source-tree.nu
    print "\n[check-ci] cargo clippy --all-targets --features standalone -- -D warnings"
    ^docker run --rm ...$vols $img cargo clippy --all-targets --features standalone -- -D warnings
    print "\n[check-ci] cargo clippy --all-targets --no-default-features --features saas -- -D warnings"
    ^docker run --rm ...$vols $img cargo clippy --all-targets --no-default-features --features saas -- -D warnings
    print "\n[check-ci] cargo build --all-targets --features standalone"
    ^docker run --rm ...$vols $img cargo build --all-targets --features standalone
    print "\n[check-ci] cargo build --all-targets --no-default-features --features saas"
    ^docker run --rm ...$vols $img cargo build --all-targets --no-default-features --features saas
    print "\n[check-ci] guarded library tests (every feature leg)"
    # --self-test first: a guard that stopped detecting a vacuous run, or a
    # `test = false` target carrying test code, must fail here rather than wave
    # every later leg through (RUS-23, RUS-27).
    ^nu scripts/check-cargo-tests-ran.nu --self-test
    let secrets = ["--env" "JWT_SECRET=test-secret-at-least-32-chars-ok!"]
    let runner = (["docker" "run" "--rm"] | append $vols | append $secrets | append $img | str join " ")
    ^nu scripts/check-cargo-tests-ran.nu --lib --runner $runner
    print "\n[check-ci] node static/tests/run.mjs"
    ^docker run --rm --volume $"($env.PWD):/build:ro" --workdir /build "{{ node_image }}" node static/tests/run.mjs
    print "\n[check-ci] all checks passed"

