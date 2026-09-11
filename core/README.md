# Meowcal Core

Independent Windows runtime for Meowcal Sub and Meowcal Sub 2. Core owns native
OCR and the managed HY-MT engine. It does not depend on Tauri or either
application executable. Source is licensed under AGPL-3.0-only; upstream runtime
and model licenses remain separate.

## Build and verify

From the repository root on Windows with Rust and the MSVC toolchain:

```powershell
cargo test --manifest-path core/Cargo.toml --locked
cargo clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings
.\scripts\build-core.ps1 -Architecture auto
.\scripts\package-core.ps1 -Architecture arm64 -OutputDirectory core/dist
```

On a local ARM64 host, set `CARGO_BUILD_JOBS=1` before a cold direct Cargo build.
The build script applies that default. Core builds without application assets or
Node dependencies. Release packaging builds x64 and ARM64 separately.

## API 1

Run `meowcal-core.exe` with inherited stdin/stdout pipes. Messages are UTF-8 JSON
objects terminated by a newline, with binary bodies for OCR requests. Only one
request may be outstanding. Logs never
share stdout with protocol frames.

The first request pins the version and client profile:

```json
{
  "id": 1,
  "api": 1,
  "method": "hello",
  "params": { "client": "sub2", "profile": "production", "expectedVersion": "0.1.0" }
}
```

Optional `storageRoot` is an absolute storage base. Core appends profile, version,
and architecture. Optional `legacyRoots` lists up to eight absolute import roots.
Neither field authorizes running an arbitrary executable or changing artifact
metadata.

Replies are `{"id":1,"result":{...}}` or
`{"id":1,"error":{"code":"...","message":"..."}}`. Install progress uses
`{"id":1,"event":"progress","message":"..."}` before the final reply.
Consumers verify version, API, required capabilities, IDs, and reply shape.

| Method             | Parameters                                                       | Result                                                                                     |
| ------------------ | ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `hello`            | Client, profile, expected version, optional storage/import roots | Version, API, capabilities, model, storage root                                            |
| `status`           | `{}`                                                             | Installed/ready state and installation paths; no installation or launch                    |
| `install`          | `{}`                                                             | Verified installation state; progress frames precede the result                            |
| `ready`            | `{}`                                                             | Ready state after verified launch; may import matching local artifacts without downloading |
| `complete`         | `request`: bounded chat-completion payload; `timeoutMs`: 1–90000 | Chat-completion response from the owned HY-MT runtime                                      |
| `ocrLanguages`     | `{}`                                                             | Installed Windows OCR language tags                                                        |
| `ocrInitialize`    | Optional `language` tag; null selects user-profile languages     | Resolved OCR language                                                                      |
| `ocrRecognizeBgra` | Language, width, height, stride, timeout; raw BGRA body          | Raw native text, lines, line boxes, frame width                                            |
| `shutdown`         | `{}`                                                             | Acknowledgement, then process exit                                                         |

OCR pixels are packed BGRA8, `stride = width * 4`; alpha is ignored. Dimensions
must fit Windows OCR's native limit and the Core limit of 4096 per axis and
64 MiB of pixels. No rescaling or thresholding occurs in this API. All JSON
headers and responses are limited to 256 KiB. OCR timeouts are 1–30000 ms.

`ocrRecognizeBgra` requires top-level `payloadBytes = width * height * 4` and
parameters `language`, `width`, `height`, `stride`, and `timeoutMs`. Exactly that
many raw BGRA bytes follow the header's newline, with no trailing delimiter.
The next JSON header begins immediately after the body. Other methods permit
only an absent or zero `payloadBytes`. Consumers require the `ocrRecognizeBgra`
capability; there is no Base64 transport.

Native recognition waits for WinRT's completion notification instead of polling.
An expired native deadline returns `OCR_TIMEOUT`; loss of the completion channel
returns `OCR_PROCESS_UNUSABLE`. Both close the session, and consumers discard the
owned process before accepting another image. Cancellation remains best effort
inside WinRT; process termination is the final bound on unresponsive native work.

Core validates the binary envelope, geometry and timeout before allocating or
reading pixels. Invalid binary headers, truncated bodies and bodies incomplete
five seconds after header validation close the session without resynchronization.
Only one binary body can be resident across reading, queuing and execution;
concurrent binary requests close the session. An OCR timeout ends that process;
the consumer must launch a replacement before sending another frame.

Applications use a separate Core instance for OCR so model inference cannot
occupy the recognition channel. Cancelling an active completion discards its
result while the consumer drains the response within the original deadline,
preserving the loaded model. Queued cancellations send no request. Closing stdin,
a transport timeout, or a broken protocol ends the owned session. Consumers must
bound writes as well as reads and reap the exact process they launched.

## Installation and compatibility

The embedded `config/engine-manifest.v1.json` is the sole authority for HY-MT
assets. Remote manifest refresh is disabled. Its schema retains the historical
`minimumAppVersion` and `embeddedApplicationRelease` names; those fields are not
the Core API/version negotiation contract.

Core storage defaults to
`%LOCALAPPDATA%/Meowcal/Core/<profile>/<version>/<architecture>`. Running engines
hold shared leases; install/repair requires exclusive access. Legacy migration
copies verified archives and models and reconstructs the runtime tree. Old
application installations and older Core versions are retained for rollback.

Applications pin an exact version and archive digest. A new release cannot
silently replace another application's selected version. Runtime archives use
`meowcal-core-v<version>-windows-<arm64|x64>.zip` and independent `core-v<version>`
tags. Core releases do not update the application's `latest.json`.

See [ADR-0004](../docs/adr/0004-versioned-meowcal-core.md) for ownership and
distribution decisions. Native OCR, model inference, coexistence, migration,
and rollback require real Windows verification in addition to contract tests.
