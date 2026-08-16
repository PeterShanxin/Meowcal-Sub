# ADR-0001: Curated Local Translation Stack

**Date:** 2026-07-29
**Status:** Accepted
**Decision owners:** Meowcal Sub maintainers

## Context

Meowcal Sub was built as a screen capture, Windows OCR, generic local-LLM translation, and overlay application. Its UI and backend expose Foundry Local concepts, model selection, endpoint configuration, fallback ordering, and context tuning.

The product goal is narrower: let a Windows user watch a Chinese or Japanese TV series with private, low-latency English subtitles without understanding local-model infrastructure.

A local HY-MT prototype proves that Tencent HY-MT1.5 1.8B can run through an app-managed llama.cpp runtime on this ARM64 device. The current prototype is not a complete product engine lifecycle and should not define the final module boundaries.

An older MeoCoSub2 design proposes a Python/OpenSubtitles rewrite and batch subtitle synchronization. That solves a different product shape and conflicts with the approved real-time Tauri/Rust direction.

## Decision

Meowcal Sub will remain a Tauri 2 and Rust Windows application using Windows OCR.

Tencent HY-MT will be the only supported translation engine in normal mode. The application will own:

- runtime and model compatibility selection;
- download and disk-space checks;
- file-size and cryptographic verification;
- install, adoption, repair, and rollback;
- exact child-process ownership;
- loopback endpoint allocation;
- start, health, warm-up, sample translation, restart, and shutdown;
- versioned engine state shared by Rust and the UI.

Normal mode will not expose model IDs, endpoint URLs, backend order, ports, Foundry CLI concepts, or raw tuning. Experimental compatible endpoints may remain in disabled-by-default developer mode.

The runtime implementation may use llama.cpp or another compatible local runtime. `FoundryLocalBackend` is not the product abstraction. Product code will depend on a curated engine contract and a translation contract.

Source OCR fallback is an explicit display/session state, not a successful translation backend.

The Python/OpenSubtitles MeoCoSub2 design is classified as a superseded historical alternative for this epic. It may be revived only through a new product decision.

## Consequences

### Positive

- one supportable installation path;
- consistent readiness and repair UX;
- model/runtime upgrades can be verified and rolled back;
- pipeline behavior can be tested against one supported engine;
- fewer user-facing settings and fewer invalid combinations;
- runtime internals can change without changing product language.

### Costs

- the app becomes responsible for large downloads, process lifecycle, manifests, and upgrade recovery;
- ARM64 and x64 assets need independent verification;
- model and runtime licenses/distribution terms must be reviewed before release;
- external model experimentation is intentionally less convenient.

### Risks

- hardcoded artifact metadata can become stale;
- fixed ports can collide;
- a failed update can strand the user without translation;
- runtime ownership bugs can leave stale processes;
- model quality may vary across language pairs.

Mitigations are a versioned manifest, hashes, last-known-good rollback, dynamic validated ports, owned process handles, staged rollout, and curated translation evaluation.

## Rejected alternatives

### Keep generic Foundry/model selection as the main product

Rejected. It transfers infrastructure decisions to viewers and creates a support matrix the project cannot verify.

### Merge `feat/hymt-foundry` unchanged

Rejected. It proves feasibility but adds engine lifecycle work to existing monoliths and lacks full process ownership, rollback, repair, and product-state boundaries.

### Rewrite in Python around OpenSubtitles

Rejected for this epic. It changes acquisition, sync, overlay, packaging, and runtime architecture at once and abandons working Windows/Tauri capabilities.

### Cloud translation

Rejected for the initial redesign. It conflicts with the local privacy promise and adds credentials, billing, availability, and policy concerns.

## Required follow-up

- Wave 1: define architecture, coding, verification, and decision-record contracts.
- Wave 2: repair OCR aliases, output validation, retry classification, and fallback semantics.
- Wave 3: build the curated engine boundary and selectively extract the prototype.
- Wave 4: replace backend UI with one guided engine flow.
- Wave 5: instrument and meet lifecycle/performance budgets.
- Wave 7: validate install, repair, upgrade, and a real episode on ARM64 and x64.

## Supersedes

For this epic, this ADR supersedes the product direction in:

- `docs/archive/plans/2026-03-04-meocosub2-design.md`

The archived file is retained as historical context.
