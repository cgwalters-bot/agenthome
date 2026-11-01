default:
    @just --list

install-tools:
    node scripts/install-tools.mjs

check: check-roles
    node --test tests/install-tools.test.mjs tests/opencode-config.test.mjs tests/agent-rules.test.mjs tests/eval.test.mjs tests/roles.test.mjs

render-roles:
    node scripts/render-roles.mjs

check-roles:
    node scripts/render-roles.mjs --check

check-install:
    node scripts/check-install.mjs
