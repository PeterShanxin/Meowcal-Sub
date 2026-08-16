# Architecture Decision Records

An ADR records one durable, cross-cutting decision and the reasoning that made
it. It explains *why* the repository is shaped the way it is. It is not a
description of how the code works today - `docs/ARCHITECTURE.md` owns that - and
it is not a plan.

## When an ADR is required

Write one when a decision is **cross-cutting** (more than one module, or the
product boundary itself), **durable** (later work is expected to obey it), and
**expensive to reverse** (undoing it means reworking shipped behavior, a trust
boundary, or a user's installation).

The three that met that bar: the curated HY-MT product stack (ADR-0001), the
engine-manifest trust boundary (ADR-0002), and the incremental Lit frontend
(ADR-0003).

No ADR is required for:

- a bounded refactor that gives an existing responsibility a clearer owner -
  record the owner in `docs/ARCHITECTURE.md`;
- a bug fix, a test, a dependency bump, or a ratchet change;
- a dated investigation, benchmark, or execution plan - that is `docs/plans/`;
- repository process rules - `docs/CHANGE_CONTRACT.md`, `CONTRIBUTING.md`, and
  `docs/AGENT_GUIDE.md` own those.

When unsure, ask whether a contributor a year from now would be confused by the
*absence* of the reasoning. If yes, write the ADR; if no, the decision belongs
in one of the documents below.

## Where a decision belongs

| Document class | Holds | Authoritative |
|---|---|---|
| `docs/adr/` | why a durable cross-cutting decision was made | yes, while Accepted |
| `docs/ARCHITECTURE.md` | current and target module ownership | yes |
| `CONTRIBUTING.md`, `docs/AGENT_GUIDE.md`, `docs/CODING_STANDARDS.md`, `docs/CHANGE_CONTRACT.md` | how to work in this repository | yes |
| `docs/plans/`, `docs/superpowers/` | dated plans, designs, and evidence | no - context only |
| `docs/archive/` | superseded direction, preserved for reconstruction | no - historical |

Normative guidance and live source win over an ADR on implementation detail. An
ADR wins on the decision itself: change the ADR before changing the boundary.

## Status values

- **Proposed** — under review and not yet binding.
- **Accepted** — active decision.
- **Superseded** — replaced by a named later ADR.
- **Rejected** — considered and intentionally not adopted.

Nothing is deleted. A Superseded or Rejected ADR keeps its file, its number, and
its original reasoning, because the reasoning is the part that stays useful.

## Lifecycle

1. Copy [the template](template.md) to `NNNN-short-title.md`, taking the next
   free number. Open it as **Proposed** in the pull request that argues for it.
2. Merging that pull request with the status set to **Accepted** is the
   acceptance; there is no separate ceremony.
3. To replace a decision, write a new ADR naming what it replaces. In the same
   change, set the old one to `**Status:** Superseded by ADR-NNNN` and update
   the index. Both directions of the link are required: an ADR that only points
   forward leaves the reader of the new one unable to find what changed.
4. To record a decision not to act, merge the ADR as **Rejected** with the
   reasoning intact.

Update the index below in the same change. An ADR that is not indexed does not
exist for anyone who did not write it.

## Index

| ADR | Status | Decision |
|---|---|---|
| [0001](0001-curated-local-translation-stack.md) | Accepted | Curated Tauri/Rust/Windows OCR and Tencent HY-MT product stack |
| [0002](0002-shipped-engine-manifest-authenticity.md) | Accepted | Shipped engine manifest and application-release trust boundary |
| [0003](0003-incremental-lit-frontend.md) | Accepted | Incremental Vite, TypeScript, and Lit frontend architecture |

## Superseded direction

The Python and OpenSubtitles "MeoCoSub2" direction was superseded by ADR-0001
before it was implemented. It is preserved as historical context in
[`docs/archive/plans/2026-03-04-meocosub2-design.md`](../archive/plans/2026-03-04-meocosub2-design.md)
and is not authoritative for any current work. Do not implement it, and do not
convert it - or any other historical plan - into an ADR.

## Verification

`npm run docs:check` resolves every relative link in every tracked Markdown
file, including this index. It runs inside `scripts/verify.ps1` and in CI, so a
renamed or deleted ADR cannot leave a dangling reference behind it.
