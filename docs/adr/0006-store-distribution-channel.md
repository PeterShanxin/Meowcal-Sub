# ADR-0006: Separate Microsoft Store distribution channel

**Date:** 2026-09-29

**Status:** Proposed

**Decision owners:** Meowcal Sub maintainers

**Related:** #62, ADR-0004, ADR-0005

## Context

Microsoft Store signs MSIX packages during publication. It does not sign our
GitHub installers. A Store installation must not replace itself with a GitHub
MSI/NSIS update, and uninstalling either channel must not remove the other's
default configuration or engine installation.

## Decision

Use an explicit Store build with no Tauri updater registration or permissions.
Microsoft Store owns its application updates. Keep the normal Tauri capture and
overlay; exclude the experimental WinUI helper from this channel.

Give Store builds their own Tauri data identity and default Core base. Retain
Core's client/profile/version/architecture layout within that base, and retain
explicit custom storage selection. Do not migrate settings between channels
automatically. Development Store builds get a separate `.dev` identity.

Require Partner Center identity and an explicit Store package version at build
time. Store versions obey Microsoft's four-component rules independently of
the existing application version. Build and validate through one local script
also used by a manual, artifact-only CI workflow.

## Consequences

GitHub and Store releases can progress independently. Store users must finish
their own initial setup, and default model storage may be duplicated across
channels. Custom external data can survive package uninstall. The downloaded
runtime remains outside the MSIX signature's coverage.

## Alternatives considered

- Reuse MSI/NSIS in the Store: still requires a separate Authenticode signer.
- Share settings/default Core storage: couples independent updates and cleanup.
- Detect a package at runtime and keep a single updater-enabled binary: leaves
  an installer replacement path in the Store binary and relies on detection at
  every caller. The explicit build omits that path at plugin registration.

## Verification and follow-up

Test identity separation, resetting custom storage, Store update presentation,
updater IPC rejection, package manifest validation and both PE architectures.
Real capture/OCR/translation/overlay, signed installation/upgrade/uninstall,
WebView2 prerequisites, full downloads, WACK and Store certification remain
required before publishing a supported Store release.
