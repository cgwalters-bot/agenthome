agenthome := "cargo run --locked --quiet --"

default:
    @just --list

install-tools:
    {{agenthome}} install-tools

check:
    cargo fmt --check
    cargo clippy --locked --all-targets -- -D warnings
    cargo test --locked

check-install:
    {{agenthome}} check-install
