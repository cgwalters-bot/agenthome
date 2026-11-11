---
name: upstream-text
description: Rules for issue bodies, pull request descriptions and forge comments. Load before writing an issue, a PR description, a review, or a comment on an issue or pull request.
---

# upstream-text

Keep PR descriptions and comments short and factual. The operator adds the human framing on nontrivial changes themselves; short bot text makes their edits visible as the human review, while long AI prose buries the signal.

**Issue and PR bodies:** every body opens with a short Executive summary section (the ask or change, a few sentences). An optional More detail section follows, at most a few paragraphs. Most detail belongs in the code, its comments and commit messages, not the body. Leave out restated context, verification logs and hedges.

**PR descriptions:** a brief what and why, one line on how it was verified, and for a nontrivial change an empty `<!-- LOGIN: context/rationale -->` line for the operator to fill (it renders invisibly if left), LOGIN being the operator's login. Add that line only when the run's environment names the operator's login; otherwise leave it out. No essays or restated diffs: if a reader cannot tell from the first two sentences what the change does, rewrite them.

**Findings:** every research, triage and session run uses one durable findings
document per topic, usually an epic: a gist through the run's document output
when available, or a file in the repository that owns the work. Link it in the
issue body under a `Findings:` line. Read it before investigating, then extend
or correct it in place with evidence (commits, PRs, run logs, file and line).
Never restate earlier findings; link them. Use only authorized outputs; if the
run cannot save the document or update its issue-body link, report the missing
capability instead of bypassing write restrictions.

**Comments:** a few lines, verdict first. A research, triage or session issue
comment says what changed in the findings document, in at most five lines, plus
the document link. No full reports in comments, including collapsed `<details>` blocks.
Text linked from a public thread is public too.
If the run's environment doesn't name the operator's login, no one counts as having summoned the bot as the operator.

**Attribution:** end every pull request description, review and comment you write with the line `Generated-by: https://github.com/cgwalters/#llms`, and never name a model, provider or tool in them: nearly every text is the work of several.

Commit messages follow the `commit-messages` skill.
