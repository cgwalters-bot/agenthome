# Role-prompt evaluation: blocked preflight for #52

> This text is LLM generated.

Part of #52. As of 2026-11-13, this is **blocked**, not a measured comparison or
an adoption recommendation. No evaluation cells or paid probes were launched.
The first gate is unverified existing weekly capacity and enforceable evaluation
budgets. It is unknown whether new money would be needed; no new money was
requested or authorized. Normal runner defaults and routing are unchanged.

## Authorization and initial evidence (2026-11-03)

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
prove spare weekly capacity. No other repository was investigated and no jobs
were dispatched to bypass the gate.

To continue, supply accessible, dated evidence of the existing pool's accounting
window/reset and remaining usable allocation net of commitments, with an evaluation
reservation covering historical sizing, role/cache probes and all 54 cells including
priming. Also supply the aggregate and per-cell enforcement configuration and proof
that dollar, token, turn and elapsed-time limits include every stage/child. The
authorization alone is not that evidence. Lower sized limits may be used, but not
an assumed replenishment or automatic top-up.

## Remaining gates, not completed work

The local executable reports `agentic-job 0.6.9`; this is not a full harness revision
pin or a role-execution test. The
[starting installation documentation](https://github.com/cgwalters-forge/agenthome/blob/b04318245f2a4fb28df8856d7cb809cf990cfea5/README.md#shared-role-templates)
records the source-subagent delivery dependency on **agentic-job#1367** and warns
that rendered files do not demonstrate installation. Current delivery remains
unverified, rather than proved absent by that older documentation. After capacity
is established, pin the actual harness revision and demonstrate that its supported
opencode runner loads and executes the intended role prompts and model settings
through that delivery contract. If absent, record the concrete dependency; do not
build a parallel installer or assume `acp-drive` can spawn arbitrary model children.
The current inspection below supersedes the historical #1367 reference.

Historical sizing/selection was not started after the first gate failed. No
requests, starting commits, independent check bundles or sample digests have been
frozen. Select three real small implements/fix rounds without arm outcomes, retain
their sizing evidence, and prove each independent check fails before and passes
after the historical fix. Keep checks and solutions outside agent context. Freeze
the same immutable requests, starts and check digests for all arms before comparison.

The offline `eval-bundle` command now supplies complete single-model and tiered
configuration pairs through a shared resolver, with coverage for all configured
and internal models. It does not install them or change ordinary runner defaults.
Configured IDs are not proof of provider model versions. No exact
model/version maps, complete three-arm bundles, common cost basis, environment pin
or harness pin are claimed here. Archive those with digests before any cell runs.

Cache namespace isolation, identical bounded priming and aggregate enforcement
remain unverified. Once all gates pass, predeclare a balanced rotated order for
three tasks × three arms × three repetitions × two cache strata = **54 cells**.
Include priming within limits, retain every failed/exhausted cell without retries,
and independently check and archive every outcome. Nothing in this report relaxes
the [protocol](README.md) or authorizes further spend.

## Current runner contract and external prerequisites

The 2026-11-13 offline slice adds `check-eval-readiness`: it rejects stale or
inconsistent allocation/reservation declarations, insufficient aggregate budgets,
post-reply or parent-only enforcement, missing phases and top-ups. Its documented
schema and tests are in [the protocol](README.md#offline-allocation-evidence-check).
It authenticates no evidence and grants no spending authority. No real capacity
snapshot, reservation or passing readiness claim was created.

Read-only inspection pinned agentic-job main to
[`3d0c9f79ef74791bef608718b2cc2dbe7101cc46`](https://github.com/cgwalters-forge/agentic-job/commit/3d0c9f79ef74791bef608718b2cc2dbe7101cc46).
The following source/documents were retrieved through `gh api`; no live role
probe, model call or job dispatch was made:

- [Limits](https://github.com/cgwalters-forge/agentic-job/blob/3d0c9f79ef74791bef608718b2cc2dbe7101cc46/docs/harness.md#limits)
  expose per-run timeout, request/task caps and agent-reported cost. Dollars are
  checked after replies and can overshoot. Task observations are not admission
  gates. These do not establish hard token/turn/dollar bounds or an aggregate
  reservation across 54 cells, nor remaining weekly capacity net of commitments.
- [Worker subagents](https://github.com/cgwalters-forge/agentic-job/blob/3d0c9f79ef74791bef608718b2cc2dbe7101cc46/docs/skills.md#worker-subagents)
  share the run token and proxy request accounting. They require positive
  `max-worker-tasks`, `max-requests` and exact advertised models on a broker route
  with per-model accounting. Partial/unpriced usage is not a hard dollar bound.
- [Source installation](https://github.com/cgwalters-forge/agentic-job/blob/3d0c9f79ef74791bef608718b2cc2dbe7101cc46/crates/agentic-job/src/run/agent.rs#L1110-L1189)
  reads committed `skills/` blobs, not configuration-root `agents/*.md`.
  [Generated workers](https://github.com/cgwalters-forge/agentic-job/blob/3d0c9f79ef74791bef608718b2cc2dbe7101cc46/crates/agentic-job/src/run/agent.rs#L1360-L1468)
  have a fixed generic body, not the shared role prompts. **However, inline
  delivery is supported**: [opencode setup](https://github.com/cgwalters-forge/agentic-job/blob/3d0c9f79ef74791bef608718b2cc2dbe7101cc46/crates/agentic-job/src/run/agent.rs#L909-L957)
  preserves a source `opencode-runner.json` profile and rewrites the base config's
  provider address/key and sharing setting. An authorized caller can stage the
  emitted pair through that existing contract; no parallel installer is needed.
  Prove actual final profile selection and every role/model execution, including
  precedence over embedded/generated agent files, before measuring.
- [The opt-in skills test](https://github.com/cgwalters-forge/agentic-job/blob/3d0c9f79ef74791bef608718b2cc2dbe7101cc46/docs/skills.md#where-each-agent-reads-them)
  checks agent discovery without model requests, not hosted role execution.
  [ACP children](https://github.com/cgwalters-forge/agentic-job/blob/3d0c9f79ef74791bef608718b2cc2dbe7101cc46/docs/harness.md#a-second-agent)
  remain Vibe-only; they are not an arbitrary opencode role installer.

[#1367](https://github.com/cgwalters-forge/agentic-job/issues/1367) closed as
NOT_PLANNED/superseded by tracker#782, not as delivery proof. No open PR addresses
#52 at this inspection. No existing dedicated source-prompt follow-up was found;
inline profile delivery means a new source-file installer is not a prerequisite
for this opencode-only slice.

The first external gate remains allocation/reservation and enforceable limits,
not a request for the operator to implement plumbing. Concrete follow-up for the
controller (this run cannot file an issue in another repository):

**Repository:** `cgwalters-forge/agentic-job`

**Title:** Expose existing allocation evidence and child-inclusive eval reservations

**Body:** Part of agenthome#52 / tracker#782. At pinned revision
`3d0c9f79ef74791bef608718b2cc2dbe7101cc46`, documented request caps and after-reply
agent-reported dollar budgets cannot demonstrate the evaluation protocol's
no-new-spend gate. Identify and reuse the current allocation authority's dated
accounting-window/reset, spending, other commitments and exclusive reservation
receipts; expose nonsecret pinned evidence without creating a second spending
authority. Demonstrate aggregate and per-cell dollar/token/turn/time admission
and external deadlines across sizing, probes, priming and every child, including
independent exhaustion tests and no top-ups. If an existing interface cannot
provide those bounds, report that limitation before any inference; do not expand
broker credentials or token scope. Integrate with agenthome's offline evidence
schema and retain blocked status until supported by authenticated evidence.

The remaining frozen-task, cache, execution and measurement gates above are
unchanged. This slice launches zero of the 54 cells and makes no adoption change.

## Offline result and limitations

[manifest.json](manifest.json) deliberately remains pending with no tasks or
configuration. [runs.json](runs.json) is the empty record array, not synthetic
measurement data. [summary.json](summary.json) is exactly the output of:

```text
cargo run --locked -- summarize-eval eval/manifest.json eval/runs.json
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
