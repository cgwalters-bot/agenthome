---
name: commit-messages
description: How to write short commit messages and pull request descriptions (why, and only the essential what; implementation explanations go in docs and module docs), and how to fold fixes or review feedback into existing commits. Load before writing a commit message or pull request description, amending, or rebasing to address review.
---

# commit-messages

## Commit messages

Unless a project's contributing docs (look for CONTRIBUTING.md or an equivalent) say otherwise, use Linux kernel style: a subject such as `install: Explicitly label .ostree.cfs`, in the imperative mood ("Add", not "Adds"), under about 70 characters.

The body says **why**, plus only the essentials of what: the motivation, the problem, the one non-obvious consequence a reviewer needs ("Note the manifest becomes part of the GC root"). Most bodies are one to three short paragraphs; many are a single sentence. A "prep" commit can be just "Prep for handling X later." A trivial change can have no body at all.

How the code works does not go in the commit message. The message is read once, at review; the explanation is needed every time someone reads the code. Put the design, the data flow and the invariants in the project's docs or in module and item docs (`//!` and `///` in Rust), next to the code, and change those with the code. If the body starts walking through the implementation, move that text into docs and leave the message the reason.

Write like a developer talking to another, plainly, in the first person where it fits. These, from the operator's own history, are the register to aim for:

```
cli: Check for container before root

This has `bootc upgrade` inside a container image give a more useful
error message.
```

```
lib: Re-export containers_image_proxy in full

We currently expose a type from that, so we might as well
just re-export the whole thing.
```

```
prepareroot: Also accept composefs enabled=signed

This was an oversight, unfortunately having two different
programming languages here led to less code sharing.
```

Avoid:
- restating the diff ("Changed function X to call Y instead of Z");
- `Changes:` or "What changed" sections, file-by-file lists, and bullet lists of implementation details; a short "While we're here:" list of unrelated small fixes is fine;
- test transcripts and command lists; one line on how it was verified, if anything;
- review bookkeeping ("Review: two passes, no blockers") in the commit; that belongs in the pull request;
- an overly formal or robotic tone.

Never add a `Signed-off-by` trailer, and never remove one a commit already has.

## Pull request descriptions

The same rules, shorter still. A single-commit PR's description can repeat the commit body or say less. A series gets a few sentences on the goal and how the commits get there, not a summary of each. Then, each on one line where the task asks for it: how it was verified, what review ran and what it left open, and what is left undone. If a reader cannot tell what the change does from the first two sentences, rewrite them.

## Fixes and review feedback

Pushed history never contains `fixup!` or `squash!` commits, nor a standalone "Address review feedback" commit. Who wrote the commit being fixed decides how:

- **Your own commits:** squash the fix into the relevant commit (amend it, or `git commit --fixup=<sha>` then `git rebase --autosquash` locally before pushing). Reword the message if the change alters what it says.
- **The operator's commits:** squash into them the same way, keeping their author and existing trailers. Say in the PR reply or body which of their commits changed, so they can re-review the diff. If the run's environment doesn't name the operator's login, no commit counts as theirs.
- **Anyone else's commits:** never rewrite them (a conflict-only rebase is fine). Add a separate, normal commit with a real subject and the AI trailer.
