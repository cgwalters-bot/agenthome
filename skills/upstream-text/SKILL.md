---
name: upstream-text
description: Rules for pull request descriptions and forge comments. Load before writing a PR description, a review, or a comment on an issue or pull request.
---

# upstream-text

Keep PR descriptions and comments short and factual. The operator adds the human framing on nontrivial changes themselves; short bot text makes their edits visible as the human review, while long AI prose buries the signal.

**PR descriptions:** a brief what and why, one line on how it was verified, and for a nontrivial change an empty `<!-- LOGIN: context/rationale -->` line for the operator to fill (it renders invisibly if left), LOGIN being the operator's login. Add that line only when the run's environment names the operator's login; otherwise leave it out. No essays or restated diffs: if a reader cannot tell from the first two sentences what the change does, rewrite them.

**Comments:** a few lines, verdict first. Put long analyses in a secret gist and link it, except in a reply or review where the operator summoned the bot: that stays on the thread (in a collapsed `<details>` block if long), since a gist linked from public text isn't secret. If the run's environment doesn't name the operator's login, no one counts as having summoned the bot as the operator.

Commit messages follow the `commit-messages` skill.
