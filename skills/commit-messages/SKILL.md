---
name: commit-messages
description: How to write commit messages and fold fixes or review feedback into existing commits. Load before writing a commit message, amending, or rebasing to address review.
---

# commit-messages

## Commit messages

Unless a project's contributing docs (look for CONTRIBUTING.md or an equivalent) say otherwise, use Linux kernel style: a subject such as `kernel: Add find API w/correct hyphen-dash equality, add docs`, in the imperative mood ("Add integration with...", not "Adds integration with...").

The body focuses on **why**, not what. The reader can see what from the diff; the message explains the motivation, the reasoning, or the problem being solved. For a "prep" commit a single line such as "Prep for handling X later." is fine.

Keep it natural and concise. A few sentences of prose on the design intent or the high-level data flow is often enough. Call out a non-obvious consequence briefly (e.g. "Note the manifest becomes part of the GC root") rather than explaining the whole mechanism.

Avoid:
- restating what the diff already shows ("Changed function X to call Y instead of Z");
- `Changes:` sections with bulleted implementation details;
- "Files changed" sections;
- an overly formal or robotic tone; write like a human talking to another developer.

Never add a `Signed-off-by` trailer, and never remove one a commit already has.

## Fixes and review feedback

Pushed history never contains `fixup!` or `squash!` commits, nor a standalone "Address review feedback" commit. Who wrote the commit being fixed decides how:

- **Your own commits:** squash the fix into the relevant commit (amend it, or `git commit --fixup=<sha>` then `git rebase --autosquash` locally before pushing). Reword the message if the change alters what it says.
- **The operator's commits:** squash into them the same way, keeping their author and existing trailers. Say in the PR reply or body which of their commits changed, so they can re-review the diff. If the run's environment doesn't name the operator's login, no commit counts as theirs.
- **Anyone else's commits:** never rewrite them (a conflict-only rebase is fine). Add a separate, normal commit with a real subject and the AI trailer.
