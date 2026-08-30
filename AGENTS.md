# AGENTS.md

This repository's canonical working contract lives in
[`docs/AGENT_GUIDE.md`](docs/AGENT_GUIDE.md).

Read it completely before making changes, then read
[`docs/CODING_STANDARDS.md`](docs/CODING_STANDARDS.md) and the relevant accepted
ADRs under [`docs/adr/`](docs/adr/).

## Anti-slop quality bar

Treat every artifact as maintainer-owned, not as a trace of an AI session. Apply
this quality bar to code, comments, documentation, PR and issue text, UI copy,
architecture, configuration, and handoff notes.

- Do not narrate the prompt, agent, implementation journey, discarded approaches,
  or direction changes unless future maintainers need that rationale.
- Do not add boilerplate prose, obvious comments, duplicate summaries or rules,
  ceremonial files or checklists, or placeholder documentation merely to make a
  change look complete.
- Do not introduce wrappers, abstractions, fallbacks, compatibility paths, feature
  flags, or configuration "just in case". Each extra mechanism must satisfy a
  current requirement or documented risk.
- Prefer direct code and concise human-quality prose. Comments should explain
  non-obvious reasons, invariants, or trade-offs rather than restating the code.
- Current docs, UI copy, PRs, and issues should state current behavior directly.
  Put history in issues, ADRs, changelogs, or dated plans unless it is required to
  apply a live safety, compatibility, or unsupported-behavior boundary.
- Before handoff, inspect the diff specifically for AI slop and remove words,
  files, layers, and indirection that add neither required behavior nor durable
  information.

Keep this file a short entrypoint. Other contract changes belong in the guide.
