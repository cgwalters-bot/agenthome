# Three-arm historical evaluation

> This text is LLM generated.

This is an offline protocol, not a launcher. Nothing runs by default and no
workflow, provider routing, broker or installed agent changes. The concrete
threat to validity is comparing different tasks, prompts, prices or environments
and crediting the arm for the difference. One mechanism addresses it: freeze a
manifest and reject records that do not match it. Validation is bounded to 54
records, two or three tasks, and 4 MiB per CLI input. These are validity checks,
not a sandbox for executing untrusted historical code.

## Research sizing comes first

`manifest.json` is deliberately **pending**, not an executable sample. The
[preflight report](report.md) records the bounded no-new-spend authorization and
the remaining capacity gate. Historical tasks, commits, sizing, resolved models
and measured results remain pending; the test fixtures are synthetic and are not
research evidence. `runs.json` is empty, and `summary.json` is its offline summary,
not a measured comparison.

Before freezing, obtain separate operator authorization identifying the research
budget, permitted repositories, harness, models and artifact destination. Inspect
two or three real historical fix-round or small-implement tasks and their
transcripts in that authorized preflight. Record links, observed size/runtimes
and why each task fits the limits; do not select by a favorable arm outcome.
Verify the starting commit is before the fix, reproduces the problem, and can run
independent checks. Freeze the sample before any comparison: retain the original
request text, immutable full starting commit, historical issue/PR links, a pinned
independent check bundle and sizing evidence per task. Validate that checks fail
on the start and pass on the historical fix; keep expected solutions/checks
outside agent context. All arms receive identical task text, tools, environment,
permissions and check-independent context. Do not cherry-pick the historical fix.

`preflight` holds `authorization` and `evidence` links. Each `tasks` entry has
`id`, `repository`, `history`, `startCommit` (40 hex), `taskText` (literal common
text), `checks: {artifact, sha256}` (64 hex), and `sizingEvidence`. Change status
to `frozen` only with evidence and the configuration below. Manifest, tasks,
checks and configuration must be immutable artifacts for the comparison; record
their content digests in the research report. Every run carries `manifestSha256`,
computed by `agent-board eval summarize` as documented in
[agent-board's eval documentation](https://github.com/cgwalters-forge/agent-board/blob/main/docs/eval.md)
(SHA-256 of the `JSON.stringify`
spelling of the manifest with recursively sorted object keys, preserving array
order, so digests from the earlier JavaScript summarizer stay valid).
The summary includes this digest. It binds the entire sample, limits and policies,
not just the current run's task. A changed task/check/config requires
a new full comparison, never pooling the two. No budget is authorized by this file.

## Role configuration contract and authorized execution

The `tiered` arm's role configuration is edited directly ("Shared role agents"
in the top-level README): `roles/opencode/eval-tiered.json` for opencode and
`roles/claude/agents/` for Claude Code; `just check` lints them. This protocol
references those artifacts rather than copying role prompts, concrete model IDs
or tier mappings. Generate complete opt-in bundles with the existing configuration
checker/resolver (the former Rust role renderer was removed):

```text
python3 tests/check_config.py --eval-arm single-strong --output /tmp/strong
python3 tests/check_config.py --eval-arm single-cheap --output /tmp/cheap
python3 tests/check_config.py --eval-arm tiered --output /tmp/tiered
```

Each output directory must not already exist. These commands only write local
artifacts: `opencode.json`, `opencode-runner.json`, `AGENTS.md`, and
`resolved-models.json`. They never install or dispatch anything. These bundles
are for skills-off callers only. Single-model arms retain the normal direct-worker
prompt and permissions; every configured agent and built-in helper is explicitly
assigned resolved sol or luna, respectively. Agents installed by harness skills
are not added as inline entries. Tiered adds the canonical five inline roles to
the same base configuration. If sol and luna resolve to the same model, the
generator refuses all arms rather than staging a same-model baseline comparison.
The provider catalogue and credential/address placeholders are unchanged.
`resolved-models.json` records configuration resolution, **not** observed provider
versions or proof of execution. Archive those separately.
Use a fresh runner session when testing bundles; configuration is loaded at startup.

A frozen `configuration` contains `configRevision`, `harnessRevision` (full
commits), `roleInterface` (pinned link to the role configuration), `environment`
(immutable image/toolchain identity), and `arms`. Each of `single-strong`,
`single-cheap`, `tiered` has `{artifact, sha256, resolvedModels}`.
`artifact` links to the complete configuration bundle; `resolvedModels`
is a canonical serialized role-to-resolved-model/version map, not aliases;
`configuration.costBasis` is one common `{artifact, sha256}` pin covering the
price table/currency/date and token/cache charging semantics for all arms, or
exactly `{unavailable: "reason"}`. Do not mix billed baseline costs with estimated
candidate costs; normalize all arms to the common basis or declare costs
unavailable. Each run repeats that same cost-basis object. Do not copy mappings into this
protocol. Keep the base config revision common; the per-arm bundle digest captures
the intentional arm difference. Strong and cheap each use one resolved model
throughout; tiered uses the shared explore-plan-execute role configuration.

For an authorized research run:

1. Pin the configuration revision and harness, assemble the three bundles from
   the checked-in files at a revision where `just check` passes, and archive them and
   resolved model maps. Keep ordinary hosted direct-worker settings unchanged;
   tiered is an opt-in research configuration only.
2. Verify the actual runner's supported source-profile contract (pinned source
   evidence is in [the report](report.md#supported-interfaces-inspected-read-only)).
   Inline roles arrive through `opencode-runner.json`, not a parallel installer
   or root `agents/` delivery. The closed installer issue is not installation
   proof. Prove the selected runner loads each role prompt and model setting.
   If unavailable, stop and record the dependency as pending. Do not assume
   acp-drive can spawn arbitrary tool/model children. It may drive supported
   sessions only; use a separately authorized harness with verified role support
   for tiered execution. A manual staged run must be labeled as such and not
   pooled with automatic sessions. Archive exact inter-stage context handoffs.
3. Start a clean worktree at the frozen commit for each cell. An external harness
   enforces the manifest's limits across all stages/children: at most 1800 seconds,
   $5, 100 turns and 200000 tokens per run (lower limits allowed after sizing).
   If a budget cannot be metered/enforced, do not launch. No retries or automatic
   top-ups. A timeout, exhausted limit or harness failure occupies its cell.
4. Run independent checks outside agent control; archive commands, exit codes,
   logs, patch and transcripts. Normalize exposed telemetry to the records below.
   Missing provider fields remain unavailable, never inferred from context size.
5. Summarize offline with `agent-board eval summarize MANIFEST.json RUNS.json`.
   Both inputs are JSON documents; RUNS is an array, not a partial JSONL stream.
   Malformed/truncated JSON is rejected, not silently repaired. No commands from
   either document are executed. Synthetic eval fixtures are tested in agent-board;
   `just check` here validates configuration only. Keep `summary.json` as the
   recorded pending output.

## Opt-in offline readiness evidence

The concrete threat is launching sizing, probes or comparison cells against
stale capacity, double-committed allocation or parent-only limits. The checked-in
`readiness.json` is deliberately blocked and retained as evidence data, not an
executable check. Agenthome no longer provides a readiness checker; that code
belongs in agent-board if wanted and has not been ported here. The schema below
records the proposed evidence contract, not implemented validation in this repo.
Assertions in JSON cannot prove that a trusted enforcement service exists, and
are neither spending authority nor a replacement for that service.
The [report](report.md#supported-interfaces-inspected-read-only) explains how to
obtain and independently validate supported evidence and the absent prerequisites.

Version 1 uses unsigned integer Unix seconds and integer micro-USD (no floating
point price arithmetic). The envelope has `pool: "current-openai"`,
`noNewSpend: true`, `window: {start, reset}` (current, at most seven days), and
`finishBy` before the reset. Each of `capacity`, `reservation`, `enforcement`,
`delivery`, `cache` needs `source` (accessible nonsecret HTTPS artifact), `sha256`
(64 hex of the archived evidence), `observedAt`, `validUntil`, `pool`,
`windowStart`, `windowReset`. The trusted reviewer must retrieve,
hash and validate the original evidence. Do not convert an unknown into `true`.

* `capacity`: `basis: "existing-allocation"`, `allocationMicroUsd`,
  `spentMicroUsd`, `otherCommitmentsMicroUsd`, `remainingMicroUsd`. Remaining must
  equal allocation minus spent minus other commitments; the evaluation reservation
  is not also included in other commitments. Prices/weekly targets are not evidence.
* `reservation`: `id`, `capacitySha256`, `microUsd` no greater than net remaining.
* `enforcement`: `interface` naming the supported service/configuration,
  `reservationId`, `scope: "all-stages-and-children"`, `perCell` and `aggregate`.
  Each limit object requires positive `microUsd`, `tokens`, `turns`, `seconds`.
  This schema describes required evidence, **not new broker or configuration keys**.
* `delivery`: `contract: "source-opencode-runner-profile"`, full 40-hex
  `harnessRevision`, `rolesAndModelsVerified: true` with actual execution evidence.
* `cache`: `isolationVerified: true`, `boundedPrimingVerified: true` with proof.
* `plan`: `sizing`, `probes`, `priming`, `cells`, each `{count, limits}`. Counts
  are 1–54; cells must be 54, priming 27 (one procedure for each warm cell).
  Each activity is bounded by $5, 200000 tokens, 100 turns, 1800 seconds.
  Priming plus its cell must fit the same per-cell enforced ceiling. All activities
  sum into aggregate limits; the aggregate dollar ceiling must fit the reservation.
  The conservative serial sum of elapsed ceilings must fit before `finishBy`.

Reservation, enforcement, delivery and cache evidence must also carry `planSha256`
equal to `manifest_digest(plan)` (the canonical digest described above), preventing
reuse for different phase counts/limits. All evidence must remain valid through
`finishBy`; refresh and revalidate before using it. Archive the immutable plan,
configuration/environment pins and exact observed model maps with the evidence.
No real allocation, reservation or enforcement artifact is supplied in this repo;
synthetic tests are not proof. Do not run probes to fill delivery/cache fields
until their own capacity and enforced limits have independently been established.

## Fair comparison and predeclared rule

Run three repetitions of every task/arm/cache cell: 36 runs for two tasks or
54 for three. Predeclare and archive a balanced, rotated arm order per task and
repetition before starting; schedule strata close together and record provider
version changes. No best-of selection, silent retries or excluding failures.
Keep temperature, seeds when supported, context policy and permissions fixed.

Cold means no intentional priming and a fresh session/cache namespace, with
provider cache isolation verified. Warm means one identical, authorized priming
procedure per arm/cell using only the common initial context, not solutions or
earlier transcripts. Archive the procedure and include priming cost and time in
that cell's metrics and limits. If isolation/priming cannot be established, stop
that stratum rather than labeling an unknown cache state cold/warm. Three
repetitions are independent sessions, not conversation continuations.

`quality-cost-time-v1` is frozen before comparison: each candidate is eligible
only if **every** candidate and strong-baseline run passes the independent checks,
its total USD is at most 80% of baseline and its total wall time is at most 110%
of baseline, separately in both cold and warm strata. Inclusive comparisons
tolerate only binary roundoff (32 machine epsilons relative to the compared totals).
All runs count, including
failed runs' spending and elapsed time. An incomplete matrix or unavailable
cost/time yields inconclusive; a quality failure rejects. Zero baseline cost/time
cannot provide a ratio. Eligibility is permission to propose a subsequent
authorized adoption experiment, not evidence of statistical generality or an
automatic default switch. This tiny historical sample cannot establish that.

## Run records and unavailable telemetry

Each version-1 run records `id`, `runUrl`, `artifactUrl`, `manifestSha256`, `taskId`, `arm`,
`cachePolicy`, `repetition` (1–3), `startCommit`, literal `taskText`,
`checksSha256`, `configRevision`, `harnessRevision`, `environment`, `renderSha256`,
`resolvedModels` and `costBasis`. They must match the frozen manifest. Links must
remain accessible to the authorized reviewer; never publish credentials or raw
secrets in fixtures or logs. Different revisions, duplicate IDs/cells and runs
outside the matrix are errors. Per-run links retain provenance when aggregating.

`metrics` requires `inputTokens`, `outputTokens`, `cacheReadTokens`,
`cacheWriteTokens`, `costUsd`, `wallSeconds`. Each is `{value: number}` or
`{value: null, unavailable: "reason"}`. Numbers are finite and nonnegative;
unsupported is not zero. State in the pinned cost basis whether input includes
cache tokens and whether cost is billed or estimated; never sum incompatible
bases. Include all child/stage usage once, not both parent totals and children.
If complete totals cannot be recovered from truncated telemetry, set affected
metrics unavailable even if a partial count exists. Unaffected externally timed
wall time or billing totals may still be known. `telemetry` has `status`
(`complete`, `missing`, `truncated`) and a `reason` for incomplete capture.

`stageSeconds` maps exposed stage names to the same metric objects or is exactly
`{unavailable: "reason"}`; no fabricated zero-duration stages. `outcome` is
`passed`, `failed`, `timeout` or `harness-error`; `failures` is an array of reasons
(required for non-passes), and `testEvidence` links to independent check results
or the log explaining why checks could not run. A process exit alone is not
correctness. The summary reports failures and availability per metric; a single
missing value makes that aggregate unavailable, not a partial total. Keep raw run
records alongside summary output for individual links, timing and cost audits.
