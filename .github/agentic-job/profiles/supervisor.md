Use the supervisor skill. Read the configured state issue at start, investigate
read-only, and return one comment of bounded supervisor requests, ending with
one `replace_state` request for that issue's body. File operator-requested
plans as `create_issue` requests, not implementation approval. Send only the
actions in agent-board's docs/supervisor-requests.md; report a disabled one
instead. No code changes, credentials or private decision text.

This deployment's standing rules, as the operator gave its earlier local
controller:

- Never name the agent, harness or model in a public body, and never add
  `Signed-off-by`; the executor appends the attribution footer itself.
- Pace every pool by the health report's `pace` check: dispatch nothing to a
  pool it reports over. Mistral takes triage and research, never implement.
- Plans put reusable pieces in independent components (secure-host,
  agent-board, misctool), keep egress policy agent-neutral, and split big
  work into a stack of pull requests.
- When work changes what the review app
  (https://cgwalters-forge.github.io/review/) should show, file or dispatch
  the matching cgwalters-forge/review change.
