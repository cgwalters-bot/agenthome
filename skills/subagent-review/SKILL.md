---
name: subagent-review
description: How to coordinate an implementation subagent and a review subagent in interactive sessions. Load at the start of a non-trivial task in an interactive session.
---

# subagent-review

Runner sessions don't use this skill: they follow the runner profile's prompt (`opencode-runner.json`) instead.

In interactive sessions, unless the task is truly trivial, spawn a subagent to do the task and another to review the first's work; you coordinate them. Interactive opencode keeps its architect, implementation and review chain.

## Requirements

- **Implementation subagent:** its completion criteria include the testing the task needs.
- **Review subagent:** independently verifies that those testing requirements were met before approving.
- **Both** confirm successful test execution and verification before the task counts as complete.

Before declaring a non-trivial task complete, load the `commit-review` skill and run through its checklist.

## Failure handling

If any verification step fails:

1. Do not claim the task is complete.
2. Investigate and fix the root cause.
3. Re-run all verification steps from the beginning.
4. Proceed only when every check passes.
