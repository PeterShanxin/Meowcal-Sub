# ADR-0004: Versioned Meowcal Core runtime

**Date:** 2026-09-11

**Status:** Accepted

**Decision owners:** Meowcal maintainers

## Context

Meowcal Sub and Meowcal Sub 2 use the same HY-MT model and Windows OCR, but
maintain separate installation, verification, launch, and recognition code.
Sub 2 can also adopt files from Sub 1's cache. That makes an application-owned
directory an undocumented dependency without giving either product a stable
compatibility boundary.

Sub 1 is a Rust/Tauri application. Sub 2 has a Python backend and a Tauri shell.
Their subtitle policies differ: Sub 1 translates screen text directly, while
Sub 2 searches and aligns subtitle files, follows a playback timeline, and fills
missing translations. Those policies must remain independently changeable.

## Decision

Canonical Core source lives in `core/` in this repository. Core is an independent
Rust crate and Windows executable, with no Tauri or Sub 1 application dependency.
It has its own semantic version, lockfile, API major version, tests, and ARM64/x64
runtime artifacts. Application product versions do not determine Core versions.

Applications launch their pinned Core executable and communicate over inherited
standard input/output. Controls and responses use bounded JSON headers; OCR
requests append a length-delimited, tightly packed BGRA8 payload directly after
the header. Image bytes never pass through Base64 or JSON serialization. The
first request negotiates
the expected Core version, API major, and capabilities. An incompatible or
missing runtime is an explicit failure; consumers do not select a global
`latest` installation or fall back to another application's executable.

Each application owns its Core processes. OCR uses a separate process from model
inference so a slow model request cannot block recognition. Closing the transport
ends the process and its owned engine. Process handles and Windows job objects
bound child lifetimes, including abnormal exits.

Core owns:

- the embedded HY-MT manifest and architecture-specific runtime policy;
- artifact download, cryptographic verification, installation, recovery, and
  sample inference;
- managed model-process launch, health, bounded inference, and shutdown;
- the Windows OCR recognition and language boundary shared by both products.

Applications own capture timing and surfaces, preprocessing/pass selection,
subtitle eligibility, prompts and context selection, output acceptance, session
cancellation, and presentation. Sub 2 additionally owns subtitle search,
matching, BGE embedding semantics, gap filling, and timeline behavior. Application
updaters remain separate from Core distribution.

## Compatibility and storage

Consumers pin an exact Core version and artifact digest. API major 1 describes
the message contract; semantic versions describe the implementation. A new Core
release does not silently opt either consumer into changed behavior. Updating a
pin requires that consumer's contract and integration tests.

Shared data is partitioned by profile, Core version, and architecture under
`%LOCALAPPDATA%/Meowcal/Core`. A custom storage base retains those partitions.
Installation takes an exclusive cross-process lock; running engines retain a
read lease. One process cannot repair files that another process is executing.
Older version directories are retained for rollback; neither application
automatically removes another version or rewrites another application's pin.

Migration accepts explicit legacy installation candidates. Only verified
artifacts are copied into Core-owned storage. Runtime archives are re-extracted
so an executable hash does not implicitly authenticate adjacent DLLs. Migration
does not delete, rename, or hard-link the old installation. A previous application
release can continue using its own files after rollback.

Readiness can import a complete verified legacy installation offline. Older Sub 2
installers deleted their runtime archive; those installations reuse the verified
model through Install/Repair and download only the missing runtime archive.
The extracted legacy DLL tree is not an alternative trust source.

## Distribution trust

The embedded manifest remains the only authority for model/runtime URLs, sizes,
and hashes. Unsigned remote manifest refresh remains disabled. Core release
artifacts are distributed separately from application releases and must not
replace the application's GitHub `latest` release or updater manifest.

Core ZIP metadata records version, API, architecture, and file digests. Consumers
verify the archive against a reviewed pin before unpacking; hashes supplied by
the downloaded archive alone are not a trust anchor. Runtime artifacts contain
Core and license material, not the downloaded HY-MT weights.

## Consequences

Stable infrastructure has one implementation and one verification suite across
Rust and Python. Version isolation costs additional disk space; separate
application-owned inference processes may each load the model. Sharing model
residency would require a broker, leases, quotas, and crash recovery across
applications, and is outside this decision.

Preserving product preprocessing and translation policies avoids changing
recognition or translation quality as a side effect of extraction. Future policy
consolidation requires comparable datasets and latency/quality evidence.

The process boundary isolates native OCR hangs without requiring a Python/Rust
binding. Binary image transport avoids text encoding, repeated parsing, and
large intermediate strings. One outstanding image and bounded transfer/native
deadlines limit memory and stale work. Shared memory is deferred: the native
bitmap still copies pixels, while mapping introduces handle and reuse lifetimes
that must justify themselves through measured savings over the binary pipe.

Performance acceptance compares each application's complete preprocessing and
recognition policy against its pre-extraction baseline on identical fixtures.
Median latency may increase by at most the greater of 10 ms or 15%; P95 by the
greater of 15 ms or 20%. The proportion of reads exceeding the 250 ms capture
interval may increase by at most one percentage point. These are acceptance
budgets, not measured guarantees: cold starts, all samples, text/geometry
equivalence, and the tested hardware must also be reported. Screen capture and
native presentation require separate end-to-end evidence.

## Relationship to prior decisions

This decision replaces the application-owned engine and application-release
manifest ownership portions of ADR-0001 and ADR-0002. Their Tauri/Rust product
choice, curated HY-MT requirement, explicit source-only state, and prohibition on
unsigned remote manifest refresh remain in force. Supporting the independent
Sub 2 product does not revive the archived proposal to rewrite Sub 1 in Python.

## Verification

Core contract tests cover negotiation, framing, bounded requests, failure
responses, and shutdown. Installation tests cover integrity rejection, interrupted
promotion, migration, concurrent use, and version isolation. Both consumer suites
exercise their adapters against the same Core API. Native OCR, real model
inference, coexistence, installation, and rollback need fresh Windows evidence
on each claimed architecture; unit tests do not establish that evidence.
