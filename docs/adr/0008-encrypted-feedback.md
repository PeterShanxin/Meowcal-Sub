# ADR-0008: Encrypted feedback with public issue tracking

**Status:** Proposed

**Date:** 2026-11-19

## Context

A maintainer investigating translation quality may need the exact subtitle and
an OCR failure may require the captured image. The project tracks defects in a
public repository. A description or archive member name can also disclose what
someone watched.

## Decision

Create a local preview and require an explicit send action for each report.
Capture and translation continue to work offline. The main window selects
individual materials by snapshot-scoped ID. The desktop app encrypts the
entire ZIP, including the description and filenames, to a build-time public key.
The receiver stores ciphertext and creates a public issue with fixed metadata.

A protected GitHub Actions job may use the team's age identity after checking
the requesting maintainer's repository permission. It returns a package
encrypted for that invocation's ephemeral recipient. The maintainer's script
recovers the ZIP locally. Logs and public workflow artifacts must contain no
plaintext material or private keys.

## Consequences

The publisher must configure the receiver origin and public key for each
distribution channel. Loss of the team identity makes existing reports
unreadable. Rotation must account for reports from already installed clients.
Maintainers who open a report gain access to everything the user selected.
