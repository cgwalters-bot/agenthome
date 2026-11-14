agenthome := "cargo run --locked --quiet --"

default:
    @just --list

install-tools:
    {{agenthome}} install-tools

# Offline configuration lint and positive/negative fixtures; Python stdlib only.
check-config:
    python3 tests/check_config.py

check: check-config
    cargo fmt --check
    cargo clippy --locked --all-targets -- -D warnings
    cargo test --locked

check-install:
    {{agenthome}} check-install
