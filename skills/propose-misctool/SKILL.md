---
name: propose-misctool
description: Propose reusable custom code to cgwalters-forge/misctool as a secondary pull request. Load when a task produces a helper useful beyond that task.
---

# Propose a misctool helper

Keep the normal task and its primary pull request first. Consider a helper only
when the custom code has a concrete use beyond this task; do not manufacture
one just to make a second PR.

To avoid publishing outside the run's authorized outputs, secondary proposals
are off by default. Proceed only when the run explicitly supports a secondary
pull request to `cgwalters-forge/misctool`. Otherwise mention the candidate in
the normal report and stop; do not push, open a PR directly, or squeeze a
second request into a primary-only output contract. Secondary output support
is tracked in `cgwalters-forge/tracker#677`; this skill does not enable it.

Read misctool's current repository instructions and a neighboring subcommand
before editing. Check for an existing equivalent. Use Rust only: add one
subcommand module following the existing layout, with only the CLI registration
needed to expose it. Do not introduce a separate script or framework.

Give the subcommand useful `--help` explaining its purpose, inputs and output,
and add at least one test asserting its behavior. Include no credentials in
code, examples, fixtures or output; do not copy task-specific secrets or private
data into the helper.

Run the focused test, exercise the subcommand's `--help`, and run misctool's
required checks. Review the diff, then propose one secondary pull request
alongside the normal one using the run's supported handback mechanism. Keep
its description brief: why the helper is reusable and how it was verified.
If support or checks are unavailable, report that instead of claiming a PR
was submitted or checks passed.
