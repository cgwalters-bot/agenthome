# agenthome
Agent configuration for agentic-job runs: per-agent settings and AGENTS.md

## Navigation tools

Run `just install-tools` explicitly to install the tools in `tools.toml`.
Nothing installs by default. Requires Node 22+, npm, curl and tar on Linux
x86_64. Add `$HOME/.local/bin` to the caller's `PATH` before launching agents;
the installer uses that path internally but cannot alter its parent's environment.
No workflow inputs select tool sources or versions: the checked-in manifest owns
them. Existing commands at or above the minimum version are left alone;
older commands in the user-local prefix may be replaced.

The manifest uses a deliberately restricted TOML subset: `[[tools]]` tables,
JSON-compatible strings, booleans and string arrays, and whole-line comments.
uv's release archive is pinned by SHA-256. ast-grep is installed as
`@ast-grep/cli@0.50.0` through npm into `$HOME/.local`; ast-outline is installed
with `uv tool install --force ast-outline==1.9.0`. Only the top-level packages
are pinned; transitive dependencies are accepted as resolved. This opt-in
installation trusts the package registries and package install/build hooks,
and is not a sandbox for untrusted packages.
uv is bootstrapped only when ast-outline needs installing and uv is missing or
too old. Commands and downloads have time/output limits, with no retry loop.
Failures produce a nonzero exit and one status line per attempted tool.

`just check` runs offline tests (archive fixtures also require zip).
`just check-install` runs the installer twice against the network and asserts
that the second run reports only present tools. The separate agentic-job
integration must invoke installation outside its sandbox and pass the PATH in.

The root `AGENTS.md` guides work on this repository only. When integrating
with the opencode configuration introduced by #1, copy its Code navigation
paragraph into `opencode/AGENTS.md` (the file deployed to opencode's config
directory), and deploy `skills/ast-explore` with the other agent skills.
That file is not yet present on this branch; the root guidance alone does not
reach agentic-job runs.

The ast-explore skill is copied from `cgwalters-bot/homegit`,
`dotfiles/.agents/skills/ast-explore/SKILL.md`, at source commit
`c027d9f4c28c8e21df37f82e978b3fc55fe73cab`.
