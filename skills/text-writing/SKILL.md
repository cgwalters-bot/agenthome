---
name: text-writing
description: Shared rules for all generated text for people (commit messages, pull request descriptions, issues, comments, reviews) - attribution, concision and avoiding duplication. Load before writing any of them; medium-specific skills such as commit-messages and upstream-text build on this one.
---

# text-writing

Shared rules for generated text, whatever the medium. The medium-specific skills (`commit-messages`, `upstream-text`) layer on top of this; load them too when they apply, and where they set a structure or length, theirs wins.

## Attribution

Generated text carries the line `Generated-by: https://github.com/cgwalters/#llms`; for a commit it is a trailer, for a pull request, review or comment the last line. Leave it out of a commit when the run's instructions say the harness adds its own attribution trailer, so a commit never carries two. Never name a model, provider or tool. A deployment that sets its own attribution URL or trailer is authoritative for every medium, and a project's own guidelines win over both.

## Avoid text dumps

Most text should be one or two paragraphs that say what the reader needs. Details sometimes matter: link to their sources (repositories, commits, issues, run logs, documents) instead of copying them in. Where the medium renders it and its own skill allows it, put unavoidable long detail in a collapsed `<details>` section; a commit message has no such section, so leave that detail out.

## Avoid duplicating content

If existing text already carries something (the issue description, a previous comment, the commit message a pull request contains, the message you are amending), don't repeat it; link or refer to it.
