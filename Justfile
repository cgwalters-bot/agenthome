default:
    @just --list

install-tools:
    node scripts/install-tools.mjs

check:
    node --test tests/install-tools.test.mjs

check-install:
    node scripts/check-install.mjs
