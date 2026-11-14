See `README.md` for purpose, usage and caller-owned tool provisioning.

# Code navigation

When `ast-outline` and `ast-grep` are on PATH (provisioned by the caller),
use the `ast-explore` skill: `ast-outline` for a file's skeleton
and `ast-outline show` for one symbol before reading it, and both before text
grep for definitions or callers, with `rg` only where they cannot parse the code.
Otherwise use `rg` and scoped reads; do not retry or install the missing tools.

In this repository the skill is `skills/ast-explore/SKILL.md`.

Run `just check` after changes. It does not provision tools or use the network
except for Cargo's initial locked dependency fetch.
