# Role-prompt evaluation: blocked preflight for #52

> This text is LLM generated.

Part of https://github.com/cgwalters-forge/agenthome/issues/52.
As of 2026-11-16, this is **blocked**, not a measured comparison or
an adoption recommendation. No evaluation cells or paid probes were launched.
The first gate is unverified existing weekly capacity and enforceable evaluation
budgets. It is unknown whether new money would be needed; no new money was
requested or authorized. Normal runner defaults and routing are unchanged.

## Authorization and accessible evidence

The run task explicitly authorizes using the
[bounded decision](https://github.com/cgwalters-forge/agenthome/issues/52#issuecomment-6606944805):
the existing OpenAI pool within its existing weekly target, with no new spend.
The retrieved comment's author is `cgwalters-bot`, not cgwalters; authority here
comes from the run task naming it, not from treating bot comments as instructions.
Its last observed update is 2026-11-03T16:33:11Z. Research tasks are restricted to
`cgwalters-forge/agenthome`; configuration and reporting also stay here.

[preflight.json](preflight.json) retains bounded nonsecret observations and
accessible source links at the starting revision
`b04318245f2a4fb28df8856d7cb809cf990cfea5`. The authorization and all three pinned
source paths were retrieved successfully using `gh api`. The inspected protocol,
configuration and issue comments contain no remaining-capacity ledger or reserved
evaluation allocation. Model prices and the ability to run this session do not
prove spare weekly capacity. That historical preflight did not investigate other
repositories. The current read-only source audit below does; no jobs were dispatched.

To continue, supply accessible, dated evidence of the existing pool's accounting
window/reset and remaining usable allocation net of commitments, with an evaluation
reservation covering historical sizing, role/cache probes and all 54 cells including
priming. Also supply the aggregate and per-cell enforcement configuration and proof
that dollar, token, turn and elapsed-time limits include every stage/child. The
authorization alone is not that evidence. Lower sized limits may be used, but not
an assumed replenishment or automatic top-up.

## Remaining gates, not completed work

The historical preflight's local `agentic-job 0.6.9` version is not a full harness
revision pin or a role-execution test. The current source audit establishes the
inline source-profile delivery contract below, not deployment or execution proof.
After capacity and probe limits are established, pin the actual harness/opencode
revisions and demonstrate that they load and execute the intended role prompts
and model settings. Do not build a parallel installer or assume `acp-drive` can
spawn arbitrary model children.

Historical sizing/selection was not started after the first gate failed. No
requests, starting commits, independent check bundles or sample digests have been
frozen. Select three real small implements/fix rounds without arm outcomes, retain
their sizing evidence, and prove each independent check fails before and passes
after the historical fix. Keep checks and solutions outside agent context. Freeze
the same immutable requests, starts and check digests for all arms before comparison.

The former Rust renderer was removed; the existing Python configuration checker
renders opt-in bundles for skills-off callers through its shared tier resolver.
Single-strong uses resolved sol throughout, single-cheap resolved luna throughout,
and tiered adds the canonical inline role mapping. Bundles reject identical
resolved baselines and do not register agents installed by harness skills.
Configured IDs are not proof of provider model versions. Observed model/version
maps, common cost basis, and environment/deployed harness pins remain absent.
Archive those with digests before any cell runs.

Cache namespace isolation, identical bounded priming and aggregate enforcement
remain unverified. Once all gates pass, predeclare a balanced rotated order for
three tasks × three arms × three repetitions × two cache strata = **54 cells**.
Include priming within limits, retain every failed/exhausted cell without retries,
and independently check and archive every outcome. Nothing in this report relaxes
the [protocol](README.md) or authorizes further spend.

## Offline result and limitations

[manifest.json](manifest.json) deliberately remains pending with no tasks or
configuration. [runs.json](runs.json) is the empty record array, not synthetic
measurement data. [summary.json](summary.json) is the archived pending summary.
To regenerate it with the current summarizer:

```text
agent-board eval summarize eval/manifest.json eval/runs.json
```

The output has status `pending`, no groups and no decisions. There are **0 of 54**
planned cells, not 54 failed or successful cells. Input/output tokens, cache
read/write tokens, common-basis dollar cost, wall time, stage timings and quality
comparisons are all **unavailable for every arm and stratum** because no experiment
was launched; none are reported as zero usage or zero cost. Preflight session usage
is not an arm measurement and is not included in the empty records.

**quality-cost-time-v1: no measured verdict; blocked before comparison.** Neither
a win nor a no-win nor a measured inconclusive result can be derived from pending
inputs. The summarizer correctly emits no decisions rather than manufacturing one.
Even a future complete 54-cell comparison is a tiny historical sample, not evidence
of general performance or permission to adopt defaults. Synthetic fixture tests
validate the offline machinery only; they are not evaluation evidence.

## Supported interfaces inspected read-only

On 2026-11-16, GitHub source/API inspection pinned agentic-job to
`670e3c3e7def4c0fd456397b0f2034e764757184` and agent-board to
`20d1a3c7d39a390872d8034e98c46718e1eed760`. These are source observations, not
claims about the deployed runner. No inference addresses or credentials were
collected; any later execution must use addresses from its environment.

**Delivery:** agentic-job's
[source reader](https://github.com/cgwalters-forge/agentic-job/blob/670e3c3e7def4c0fd456397b0f2034e764757184/crates/agentic-job/src/run/agent.rs#L1100-L1157)
reads `opencode.json`, `AGENTS.md`, and `opencode-runner.json` from the selected
configuration-repository path. Its
[profile validation and write](https://github.com/cgwalters-forge/agentic-job/blob/670e3c3e7def4c0fd456397b0f2034e764757184/crates/agentic-job/src/run/agent.rs#L859-L957)
reject provider settings in the profile and copy it to the opencode configuration
directory. Inline `agent` entries are the supported source-subagent route for
this opencode-only comparison. Root source `agents/*.md` is not installed; the
[embedded reviewer](https://github.com/cgwalters-forge/agentic-job/blob/670e3c3e7def4c0fd456397b0f2034e764757184/skills/agents/diff-reviewer.md)
has no model in frontmatter. The
[closed delivery issue](https://github.com/cgwalters-forge/agentic-job/issues/1367)
is not proof of installation and is not the dependency to reopen. Verify the
actual caller's config repository/ref/path, archived installed profile digest,
opencode merge precedence (including ACP/main-model overrides), and observed
role prompts/model versions. Rendering alone proves none of those runtime facts.

**Local, no-inference config preview (2026-11-22):** OpenCode 1.18.35 loaded
all three skills-off bundles generated from base `37bdd4bec333861aaf831e7abf2325b1e4b0cb9d`
with generator blob `8a89367eb53e4e6ad33297b3932e261ee43348bb`. For each `ARM`, with a
fresh directory `D`, the launcher supplied only `PATH` and the environment below:

```sh
python3 -B tests/check_config.py --eval-arm "$ARM" --output "$D/opencode"
export HOME="$D" XDG_CONFIG_HOME="$D" OPENCODE_CONFIG="$D/opencode/opencode-runner.json"
export OPENCODE_DISABLE_MODELS_FETCH=true OPENCODE_DISABLE_PROJECT_CONFIG=true
opencode agent list --pure
opencode debug config --pure
```

Both inspections exited 0 for each arm. In both baselines the registry was
`build`, `compaction`, `explore`, `general`, `plan`, `summary`, `title`,
defaulting to `build`. All agent and top-level model settings were
`praxis/gpt-6.2-sol` for strong and `praxis/gpt-6-luna` for cheap. Tiered
defaulted to `role-execute-lead`; its merged configuration showed:

| Configured model | Registered agents |
| --- | --- |
| `praxis/gpt-6.2-sol` | `build`, `compaction`, `general`, `plan`, `role-execute-lead`, `role-plan`, `role-review`, `summary` |
| `praxis/gpt-6-luna` | `explore`, `role-explore`, `role-worker`, `title` |

Tiered's top-level `model` and `small_model` were sol and luna, respectively.
`architect` and `fast` remained disabled in all arms, and `diff-reviewer` was
absent. The merged model maps matched `resolved-models.json`; the five merged
role prompts matched the tiered profile. This is a local caller load check, not
evidence of hosted agentic-job installation, provider invocation, or enforcement.

**Existing limits:** inspect the pinned
[budget implementation](https://github.com/cgwalters-forge/agentic-job/blob/670e3c3e7def4c0fd456397b0f2034e764757184/crates/agentic-job/src/session/budget.rs)
and [harness limits](https://github.com/cgwalters-forge/agentic-job/blob/670e3c3e7def4c0fd456397b0f2034e764757184/docs/harness.md#limits).
`[limits]` provides `budget` (hundredths of a dollar), `max-requests`,
`timeout-minutes`, `max-tasks`, `max-worker-tasks`. Budget uses agent-reported cost;
the documented second-agent path accounts only the main agent's cost. Proxy
request counting and timeouts do not establish dollar/token/turn enforcement
across children or aggregate evaluation limits. Do not equate requests to turns.
Retrieve the trusted deployed limits, `harness.json`, `summary.json` and proxy
usage records; reconcile child requests and costs exactly once. Missing cache
write or stage-time counters stay unavailable. The
[summary implementation](https://github.com/cgwalters-forge/agentic-job/blob/670e3c3e7def4c0fd456397b0f2034e764757184/crates/agentic-job/src/run/summary.rs)
is accounting, not a reservation interface. The supported `acp-drive` child is
not an arbitrary OpenAI role launcher.

**Existing pool accounting:** agent-board's
[pace reader](https://github.com/cgwalters-forge/agent-board/blob/20d1a3c7d39a390872d8034e98c46718e1eed760/crates/board-forge/src/poll/pace.rs)
uses a snapshot with `schema`, `updated_at`, `pools` and per-pool `used_percent`,
`window_minutes`, `resets_at` (plus holds/dispatch timing). Follow the trusted
caller's `policy.pace` source to retrieve that snapshot, validate freshness,
window/reset and hold state. Dispatch slots/leases and pace eligibility are not
reserved dollars. The read-only
[budget CLI](https://github.com/cgwalters-forge/agent-board/blob/20d1a3c7d39a390872d8034e98c46718e1eed760/crates/agent-board/src/budget.rs)
supports `agent-board budget --config FILE --four-weeks --until RFC3339` and
`--split --since RFC3339 --until RFC3339`; these aggregate reported costs against
configured `budget.weeklyUsd` and `budget.harnessRepositories`, not live allocation.
The [pool resolver](https://github.com/cgwalters-forge/agent-board/blob/20d1a3c7d39a390872d8034e98c46718e1eed760/crates/agent-board/src/pools.rs)
selects configured models/budgets; it is not a capacity API. None establishes an
existing-pool allocation net of commitments or an evaluation reservation.

There is no readiness checker in agenthome. The
[evidence contract](README.md#opt-in-offline-readiness-evidence) retains the
seven-day window and capacity, reservation and plan bindings.
A reviewer must retrieve and hash the originals, confirm allocation authority
distinct from prices/targets, verify a real reservation against other commitments,
and verify actual enforcement before filling in the evidence.
The evidence document cannot replace the missing service or authorize a run.
No broker/token scope, spending authority, top-up, or default routing is added.

## External prerequisite and concrete follow-up

The exact first remaining gate is **no verified existing-allocation reservation
and child-inclusive aggregate/per-cell enforcement**. Role/model execution and
cache proof follow only within such bounds. Searches of open/closed issues found
no direct implementation issue for this combined contract.
[Cost reporting](https://github.com/cgwalters-forge/agent-board/issues/281),
[banked usage](https://github.com/cgwalters-forge/agent-board/issues/940), and
[dispatch leases](https://github.com/cgwalters-forge/agent-board/issues/382)
do not provide it. No issue was filed and no issue number is claimed.

Concrete unfiled follow-up repository: `cgwalters-forge/agentic-job`.
Title: **Expose and enforce existing-allocation bounds across evaluation children**.
Body:

> The offline evaluation in agenthome needs a supported evidence/enforcement
> contract before it can launch sizing, probes, priming or 54 comparison cells.
> Reuse existing pool accounting and run-token enforcement, without new money,
> top-ups or wider broker/token scope. Expose a read-only dated accounting
> window/reset, existing allocation net of spending and other commitments, and
> reservation identity. Bind aggregate and per-cell dollar/token/turn/time ceilings
> to that reservation and cover every stage/child, failing closed on unavailable
> accounting. Current agent-reported budget, request caps, timeout and board pace
> reports do not prove this coverage. Add tests demonstrating child-inclusive
> exhaustion and reservation consistency, and document the supported keys and
> artifacts a caller can validate. Coordinate the allocation evidence with
> agent-board rather than treating its configured weekly target as capacity.

This is infrastructure work, not a request for the operator to implement it
manually. Until it exists and is verified, retain the useful offline tooling,
pending manifest, empty runs and blocked verdict.
