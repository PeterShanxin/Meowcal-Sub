# ADR-0002: Shipped Engine Manifest Authenticity

**Date:** 2026-07-30  
**Status:** Accepted  
**Decision owners:** Meowcal Sub maintainers

## Context

Meowcal Sub installs a model and architecture-specific executable runtime. A
remote manifest that can replace executable URLs or hashes becomes a
supply-chain control plane. TLS alone does not prove that a manifest was
approved by the application maintainer, and the first redesign does not yet
have a separate manifest-signing and key-rotation system.

The existing MVP hardcoded artifact metadata in Rust. That prevented an
untrusted remote update but did not provide one typed, versioned compatibility,
rollback, requirement, support-code, and licensing contract.

## Decision

The first curated engine manifest is tracked as
`config/engine-manifest.v1.json` and embedded into the application binary at
compile time. The typed Rust parser rejects unknown schemas, architectures,
unsafe paths, missing hashes, non-HTTPS artifacts, incomplete license
references, unsafe host policy, and incomplete rollback metadata.

Remote manifest refresh is disabled. A manifest update is accepted only as part
of a reviewed application release and inherits that release's distribution
integrity. Production distribution must satisfy the application's signing and
release controls before this policy can be described as cryptographically
signed to users.

The manifest represents ARM64 OpenCL/Adreno and x64 Vulkan separately. It
contains SHA-256 and expected size for each downloaded archive, the installed
runtime executable, and the HY-MT model. Installation code consumes the parsed
manifest rather than duplicating artifact constants.

## Consequences

- an unsigned remote response cannot redirect executable installation;
- manifest and application compatibility change atomically;
- offline installation and deterministic tests remain possible;
- emergency model/runtime metadata changes require an application release;
- a future remote refresh mechanism requires a new ADR covering signatures,
  trusted keys, rotation, replay/downgrade protection, and recovery.

The manifest records upstream license references and a distribution-review
state. It does not itself grant redistribution rights. The Tencent model entry
remains `requiredBeforeRelease`; llama.cpp records its upstream MIT license.

## Alternatives considered

### Unsigned remote JSON over HTTPS

Rejected. It creates an executable replacement path without maintainer
authenticity or downgrade protection.

### Continue with Rust constants

Rejected. It lacks one versioned parser/validator and makes compatibility,
rollback, support-code, and licensing rules implicit.

### Signed remote manifest now

Deferred. It requires trusted-key storage, rotation, revocation, replay
protection, and an operational signing process that are not established in the
first redesign.

## Verification and follow-up

- Parser tests cover the shipped document, corrupt JSON, unknown schema and
  architecture, invalid hashes, unsafe paths, upgrades, and rollback rules.
- Installer verification covers archives, the extracted executable, and the
  model using manifest values.
- Transactional promotion and last-known-good preservation remain owned by
  Issue #20.
- Dynamic port selection and runtime recovery remain owned by Issue #21.
- Model redistribution review and application signing remain Wave 7 release
  gates.

## Upstream references

- Tencent HY-MT model:
  <https://huggingface.co/tencent/HY-MT1.5-1.8B-GGUF>
- Tencent HY Community License Agreement:
  <https://huggingface.co/tencent/HY-MT1.5-1.8B/blob/main/LICENSE>
- llama.cpp MIT license:
  <https://github.com/ggml-org/llama.cpp/blob/master/LICENSE>
