# AGENTS.md

This repository's canonical working contract lives in
[`docs/AGENT_GUIDE.md`](docs/AGENT_GUIDE.md).

Read it completely before making changes, then read
[`docs/CODING_STANDARDS.md`](docs/CODING_STANDARDS.md) and the relevant accepted
ADRs under [`docs/adr/`](docs/adr/).

## Normative documentation

Write standing guidance as current-state rules, not as a narrative of how the
repository reached them. Omit transition commentary about retired mechanisms or
prior directions when the current rule is sufficient. Keep historical rationale
in issues, ADRs, changelogs, or dated plans unless it is needed to apply a current
safety, compatibility, or unsupported-behavior boundary. Preserve negative wording
when it defines a real invariant; remove stale or redundant guidance instead of
accumulating exceptions.

Keep this file a short entrypoint. Other contract changes belong in the guide.
