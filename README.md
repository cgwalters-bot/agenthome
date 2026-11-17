# agenthome

> This text is LLM generated.

Agent configuration for agentic-job runs: per-agent settings and AGENTS.md

## Tooling

Configuration is edited directly in the checked-in files; no renderer or build
step is needed. Run `just check-config` (or `python3 tests/check_config.py`) with
Python 3.9 or later to lint the configuration and run its positive and negative
fixtures. This uses only the standard library, without Cargo, downloads, package
installation or network schema fetching.

This repository has no compiled application or Rust toolchain requirement.
Offline eval summarization is provided by `agent-board eval summarize MANIFEST RUNS`;
see [agent-board's eval documentation](https://github.com/cgwalters-forge/agent-board/blob/main/docs/eval.md).
No JavaScript is left, and `just check` fails if a `.js`, `.mjs` or `.cjs` file comes back.

## Navigation tools

Callers provision tools through their own setup steps or images and pass an
explicit `PATH` before launching agents. Nothing in this repository installs
tools on an agent's behalf. When ast-outline and ast-grep are absent, use `rg`
and scoped reads; do not retry or install missing tools.

The Check workflow demonstrates ordinary Linux x86_64 caller setup in a
job-local prefix, without privileges or credentials. It retains reviewed release
SHA-256 pins for ast-grep 0.50.0, uv 0.13.1 and cgwalters-forge-misctool 0.1.0,
rejects checksum mismatches before extraction or execution, and bounds downloads
and provisioning. ast-outline 1.9.0 uses `uv tool install --no-build`, the full
`constraints/ast-outline.txt` and `--exclude-newer 2026-10-21T00:00:00Z`.
No sdist build runs; uv ignores hashes in constraints, so wheels remain trusted
as PyPI served them before that cutoff. This is not a package sandbox.

`just check` runs `just check-config` and offline model discovery tests. CI separately provisions tools and
checks their exact versions and structural navigation. No generic installer or
manifest interface remains. Do not restore agentic-job's rejected `[setup.tools]`
mechanism. External caller package settings still need their agenthome-only Rust
requirements removed; this repository no longer builds or tests a Rust crate.

### Installer caller audit (2026-11-14)

Refreshed the [earlier inventory](https://github.com/cgwalters-forge/agenthome/issues/76#issuecomment-6779099295)
against agenthome `4e68032821d38fa0f49601fd4882c68f4afd3bbe` and current
default branches. The only active `check-install` caller was this repository's
Check workflow; it now owns setup directly. The Justfile wrappers, CLI variants,
installer-only tests and installation instructions were the other local
`install-tools` references; they are removed in this slice.

Tracker at `595cc08ed43c4c5245059ffe11265172b9a0d7f7` has only commented
`setup.tools` examples in `.github/agentic-job/hosted.toml:70–73` and
`acp-experiment.toml:71–74`, not active calls. Their agenthome revision
`9b43eb81e1266ec762a6c49a4e31fb6b4ff3e0e0` is a historical pin, not evidence
of a current deployment. Agentic-job at
`df6134e1009b8774c264de440028a4e324e43711` refuses `setup.tools`
(in its configuration parser); its
`docs/dispatch.md:376–393` discusses the former interface, not an active call.
Agent-board at `157fff2c117c6ea1ea59af2b8517963baa806d43` has no active
agenthome installer reference. Their workflows' Cargo invocations build their
own programs, not agenthome. Configuration/profile consumers do not require
the removed CLI. No active external dependency was found in those default
branches; other branches and deployed revisions remain unverified.

PR #87's draft and human-approval hold are unchanged. This slice neither runs
paid evals/hosted role probes nor changes canonical roles or model routing.

The root `AGENTS.md` guides work on this repository only. Its Code navigation
paragraph is copied into `opencode/AGENTS.md` (the file deployed to opencode's
config directory, and as `opencode/CLAUDE.md` to Claude's), and `just check`
keeps the two copies the same. agentic-job runs that turn on `agent-skills`
with this repository as their configuration repository receive `skills/`,
`skills/ast-explore` included.

The ast-explore skill is copied from `cgwalters-bot/homegit`,
`dotfiles/.agents/skills/ast-explore/SKILL.md`, at source commit
`c027d9f4c28c8e21df37f82e978b3fc55fe73cab`.

## opencode configuration

### Model discovery

`just discover-models` previews newer stable models in the families already
declared by the praxis prompt tiers; `just discover-models --write` writes the
bump, including role profile references and regression-test IDs. Preview is the
default so inspecting releases cannot accidentally change routing. It shows
affected IDs and old/new values; `--json` emits files, structured changes and
diagnostics for automated callers. Set
`BROKER_BASE_URL` (including `/v1`) and `BROKER_API_KEY` in the caller environment;
without either, discovery reads `provider.praxis.options` `baseURL` and `apiKey`
from the deployed `$XDG_CONFIG_HOME/opencode/opencode.json` (default
`~/.config/opencode/`), where an agentic-job opencode run has replaced the
placeholders with its proxy and run token. It fails naming what is missing.
Both values must be strings, with an HTTP(S) URL and no `.invalid` host;
unresolved opencode references such as `{env:...}` are not expanded by this
command: set both broker variables instead. A deployed plain-HTTP `baseURL` is
used only on a loopback address; for another plain-HTTP broker, which does not
protect the token in transit, set both broker variables explicitly.
Nothing schedules this by default. Discovery checks models.dev (opencode's
catalog), OpenAI's Markdown model overview and per-model pages, and Anthropic's
model overview. It never adds a tier, changes a fallback, or enables Terra.

Public releases are not evidence of broker availability: only IDs from the
broker's `/models`, or completed Responses probes with matching model IDs when
that route returns 404/405, are eligible. There are at most eight probes per
invocation, each requesting 16 output tokens at low effort; a broker may ignore
that token cap. Requests have a 60-second socket timeout and a 16 MiB response
bound. The Linux CLI also has a ten-minute elapsed-time deadline so a slow stream
cannot keep discovery running indefinitely. Authenticated redirects are refused
so a broker login redirect cannot leak the run token. Broker requests using the
deployed config connect directly rather than forwarding its token through an
ambient proxy. Public source fetches and explicitly configured broker requests
still honor proxy settings. Discovery is separate from the network-free `just check`.

`tools/model-sources.json` records provider prices and limits for each bump,
the URLs corroborating them, and configured caps. A bump adds the new definition
and keeps the replaced one, since callers outside this repository (tracker's
`pools.json`, actions' `deployment.json`, `AGENT_MODEL` variables) may still
name it; remove it by hand once they move. If a newer target is already defined
(for example after a tier rollback), discovery leaves that tier unchanged and
asks the operator to repoint it by hand, without overwriting its definition or
blocking other tiers. Tier models with a record are also
re-checked each run, and a changed published price or limit for the same ID
updates the record and the configured price (caps only ever shrink). Luna has
no record yet, so it is not re-checked. Existing local context/input
and output caps are retained, not automatically expanded. Cache creation keeps
the input rate because the broker meters it as plain input; published provider
cache-write rates are recorded separately. OpenAI numeric facts must agree with
models.dev before an update. A verified successor does not require the retired
model's page. An unverifiable same-ID check leaves that model unchanged and
reports a diagnostic without suppressing independent bumps. Claude families are supported
only if already declared; their numeric facts come from models.dev, with model
IDs checked against Anthropic's overview. No Claude pool is declared today.
Unparseable source bundles, missing credentials or ambiguous successor metadata
fail the job; they are not treated as a successful no-change check. Missing
evidence history starts empty, but malformed evidence remains an error.

#### Caller credential-fallback preview

The actual agentic-job fix caller for this pull request ran the following at
reviewed head `3ceeeebe840ab69e13bc0931bc23b7ecee16c26b`, not with fixtures.
The deployed config was
`/home/runner/work/_temp/agent-home/.config/opencode/opencode.json`, provider
`praxis`, with an HTTP proxy URL ending in `/v1` and a populated `apiKey`.
The proxy hostname and token are omitted. No endpoint was copied into code.

```text
$ env -u BROKER_BASE_URL -u BROKER_API_KEY timeout 90s just discover-models
python3 -B tools/discover_models.py
No model changes.
$ git status --short
(no output)
```

The preview exited 0; no `--write` was used and the checkout remained clean.

`just discover-models --fixtures tests/fixtures/model-discovery.json --write`
replays the first bump offline. The fixture contains selected verbatim source
excerpts and catalog fields recorded on 2026-11-17, plus sanitized successful
broker probe outcomes; it contains no endpoint, token or response IDs. Replaying
against the bumped tree is a no-op. The weekly workflow must be installed by the
controller from the accompanying PR description; it is not enabled here.

`opencode/` holds `opencode.json` (the one provider and its models) and
`opencode-runner.json` (the unattended-run profile merged over it) for
agentic-job runs: set the run's `agent-config-path` to `opencode`, and its
configuration repository and ref through the caller's own variables. The
provider address and key are placeholders a run replaces; `just check-config`
enforces the placeholders, since this repository is public.

`opencode-local.json` is an opt-in profile for interactive sessions on a
self-hosted OpenAI-compatible endpoint (llama.cpp router models), with a
trimmed `local` agent for small GPUs. Merge it over the deployed configuration
with `OPENCODE_CONFIG=~/.config/opencode/opencode-local.json opencode`
(tested with `opencode` 1.18.35, using its `provider`/`npm`/`options` format;
native V2 `providers` entries are ignored by that launcher). It
enables only its own provider and routes every agent, title and compaction
included, to it; `just check-config` refuses a profile that leaves any agent on
praxis. Its endpoint is a placeholder too, replaced in the operator's private
derived configuration at `provider.local.options.baseURL`. Before starting a
session, preview model resolution without inference or applying caller changes:
`OPENCODE_CONFIG=~/.config/opencode/opencode-local.json opencode --pure models local`.
The output must list the three `local/` model IDs, not just exit successfully:
1.18.35 also exits zero for `Provider not found`. This is a configuration preview,
not proof that the endpoint serves those models or supports tool calls.
Project configuration loads after it, so a repository's
own `opencode.json` can still re-enable praxis.

The runner follows the review procedure in the skill named by a task, such as
agentic-job's `implement` skill and its `diff-reviewer` subagent. The build
prompt in `opencode-runner.json` defers to that procedure and keeps its one read-only
`general` reviewer for tasks whose named skills don't define a review. That
prompt differs from homegit's copy; keep the change on a re-sync.

`opencode.json` and `opencode-runner.json` are copied from homegit `dotfiles/.config/opencode/` at source
commit `c027d9f4c28c8e21df37f82e978b3fc55fe73cab`, with the provider address
replaced by a placeholder.

`opencode/AGENTS.md` holds the standing rules; the procedures
are skills in `skills/`, loaded when a task needs them. Both are cut down
from agenthome#6 (`opencode/AGENTS.md`, which became the rules and the
`commit-messages`, `upstream-text` and `subagent-review` skills) and from
homegit's `dotfiles/.agents/skills/commit-review/SKILL.md` at the commit above.
They don't name an operator or bot login or a homegit tool. `just check`
enforces this. agenthome#8 is closed without deciding how a run learns the
operator and bot logins, so the skills that need the operator
(`commit-messages`, `upstream-text`, `commit-review`) cannot know it yet. There is no
issue for that follow-up yet.

The rules are written for opencode runs and Claude runs. opencode runs load
`AGENTS.md`; the agentic-job adapter for Claude loads only `CLAUDE.md`.
`opencode/CLAUDE.md` is a copy that `just check` keeps byte-identical, so edit
`opencode/AGENTS.md` and copy it over with
`cp opencode/AGENTS.md opencode/CLAUDE.md`.
Both need the run's `agent-config-path` set to `opencode`. A Claude
run gets the rules only once agentic-job dispatch forwards the configuration
repository for Claude, as it does for opencode and Vibe. Vibe runs do not get
the rules: they take only models from `config.toml`.

### Proposed prompt-tier source of truth

`prompt-tiers.json` is the proposed agenthome-owned source for per-pool
prompt tiers and role assignments. These are prompt capability/cost labels,
not tracker's pool failover tiers: resolution never switches pools. The
proposed names and mapping are not an operator-approved model table.
Only `praxis` is represented, using models already configured in
`opencode/opencode.json`; no other pool's support has been verified. `terra`
is explicitly unavailable (`model: null`) and falls back to `sol`. Each entry
declares its fallback, with `null` marking the end of the path. An unavailable
entry follows that edge; a declared model must exist in the tool catalogue.
Role assignments preserve the currently working model choices, including
`architect` using `sol`, rather than implicitly promoting it to `astra`.
Model identifiers belong in this data or tool configuration, not role prompts.

`just check-config` (`tests/check_config.py`) prevents a misspelled selection from choosing
an unconfigured model and a fallback cycle from hanging resolution. It
checks every edge, including those behind available models, and walks each
path at most once per tier. Unknown pools, tiers, undeclared models, cycles
and paths without terminal models fail. The role assignments must resolve to
the models `opencode/opencode.json` and `opencode/opencode-runner.json`
already configure, so the manifest describes the current configuration rather
than changing it. This is offline configuration validation, not broker
enforcement or a model-availability probe. Nothing reads the manifest at run
time; tiered hosted runs and an agentic-job#1367 consumer are not wired up.

### Shared role agents

The tiered chain has five roles: explore, plan, execute-lead, worker and
review, named `role-explore` and so on. Each is written twice, and the two
are edited together: as a Claude Code Markdown agent in
`roles/claude/agents/`, and as an opencode agent entry in
`roles/opencode/eval-tiered.json`, an opt-in profile for the `tiered` arm of
the evaluation below that makes `role-execute-lead` the default agent. Edit
these files directly. `just check-config` requires the same description and prompt
in both, each role's access level and prompt tier as `tests/check_config.py` pins
them, the opencode model that tier resolves to in `prompt-tiers.json`'s
`praxis` pool, no model or tier named in a prompt,
and the formatting the files have now (the profile as `JSON.stringify(value,
null, 2)`).

The standalone tests retain the invariant test names from the former Rust
configuration suites. Their mutation groups cover all 32 role/tier fixtures,
9 provider and 3 runner fixtures, and 25 standing-rule fixtures, in their original
order. Eval serialization tests now belong to agent-board; the eval digest format
is unchanged.

The prefix keeps the roles from replacing opencode's built-in `plan` and
`explore`, Claude Code's own subagents, or agentic-job's embedded
`diff-reviewer`; the lead and the reviewer follow any review procedure defined
by a skill named in the task. Explore, plan and review cannot edit; only the lead can
delegate, so a worker cannot start a chain of its own. The read-only roles
keep the shell for the navigation tools, as the runner's explore does, so
their prompts rather than a tool rule keep them read-only. Claude Code agents
take `model: inherit`: the manifest has no verified Claude pool, and Claude
Code subagents cannot start subagents, so the lead works there only as the
main agent. `opencode/opencode-runner.json` keeps its direct-worker policy.

The supported opencode source-subagent delivery is an inline
`opencode-runner.json` profile read from the configuration repository's selected
path. It does not require a root `agents/` installer. The
[pinned source audit](eval/report.md#supported-interfaces-inspected-read-only)
establishes that contract, not actual role/model execution in a deployed runner.
The closed installer issue is not installation proof. Opt-in complete bundles
for all three arms can be generated through the existing configuration resolver;
see [the protocol](eval/README.md#role-configuration-contract-and-authorized-execution).
No ordinary caller or runner setting changes, and measurement is still blocked
on capacity, enforcement and execution evidence.

## Offline role evaluation

`eval/README.md` defines the opt-in three-arm historical evaluation protocol and
the input format of `agent-board eval summarize MANIFEST RUNS`, documented in
[agent-board](https://github.com/cgwalters-forge/agent-board/blob/main/docs/eval.md).
The sample still needs
authorized sizing evidence and a runner that installs the role agents. No research runs or
workflow changes are enabled. `eval/summary.json` remains the recorded pending
output; synthetic eval tests run in agent-board, not `just check` here.

## Tracker supervisor profile

`skills/supervisor` holds the hosted supervisor's judgment brief.
`.github/agentic-job/profiles/supervisor.md` is the tracker profile template;
copy it to that same path in tracker, enable `agent-skills` with agenthome as
the configuration repository, and select it from the trusted supervisor caller.
It doesn't enable a workflow here. The skill's request examples come from
agent-board's `docs/supervisor-requests.md` at commit
9d36c74f5b8a71f94d9a95583f473591f37ec001; `tests/fixtures/agent-board-supervisor-actions.jsonl`
vendors that example block, and `just check` fails when the skill differs, so a
sync updates the commit, the fixture and the skill together. Before enabling
apply, prove a `preview` run lists the requests and doesn't apply any, including the
final state-body replacement; that proof belongs to tracker#687.
