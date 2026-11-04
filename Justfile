agenthome := "cargo run --locked --quiet --"

default:
    @just --list

install-tools:
    {{agenthome}} install-tools

check: check-roles
    cargo fmt --check
    cargo clippy --locked --all-targets -- -D warnings
    cargo test --locked

render-roles:
    {{agenthome}} render-roles

check-roles:
    {{agenthome}} render-roles --check

check-install:
    {{agenthome}} check-install
