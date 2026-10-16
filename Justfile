default:
    @just --list

install-tools:
    node scripts/install-tools.mjs

check:
    node --test tests/install-tools.test.mjs tests/opencode-config.test.mjs

check-install:
    node scripts/check-install.mjs
