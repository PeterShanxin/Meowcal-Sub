# Release packaging and the runner policy

Every GitHub Actions job in this repository runs on a GitHub-hosted runner.
There is no self-hosted runner, and nothing in Actions waits on a machine
somebody has to keep online.

`scripts/check-workflow-runners.mjs` enforces the policy below on every file in
`.github/workflows`, and runs as part of `scripts/verify.ps1 -Stage Frontend`.

## The runners a job may name

| Runner | What runs there |
| --- | --- |
| `ubuntu-24.04` | Change Contract, the pull request scope classifier, the required-check wrappers |
| `ubuntu-latest` | Release administration, release preflight asset validation, the legacy updater bridge |
| `windows-11-arm` | The Stage 2 Windows verify gate, and ARM64 packaging |
| `windows-2025` | x64 packaging |

Anything else is a policy violation, including `windows-latest`, `windows-2022`,
macOS, and any indirect value such as a repository variable or a matrix key. An
indirect value can resolve to a runner nobody reviewed, so `runs-on` must name
its runner in the workflow text. The one exception is the packaging expression,
which selects between two named images from the architecture input and is
checked through the literals it can produce.

## Packaging is native per architecture

`.github/workflows/package.yml` is a reusable workflow taking one input,
`architecture`, which is `x64` or `arm64`.

| Architecture | Runner | Rust target |
| --- | --- | --- |
| `x64` | `windows-2025` | `x86_64-pc-windows-msvc` |
| `arm64` | `windows-11-arm` | `aarch64-pc-windows-msvc` |

Each architecture builds on its own hardware. No step depends on Windows x64
emulation or on an MSVC cross-linker, so a packaging failure is a failure of the
code rather than of the host it happened to land on.

An architecture the workflow does not recognize falls through to the x64 image
and is then refused by the "Resolve package contract" step, before any toolchain
work. That step also refuses to continue when the runner's own architecture does
not match the requested one, so a wrong-image build can never reach
`upload-artifact`.

The job declares its own toolchain rather than inheriting one from the image:
Node from `actions/setup-node`, the .NET SDK from `actions/setup-dotnet`, and the
Rust toolchain plus the one target it needs from `rustup`. It restores no Rust
cache: a release build starts cold.

## What a release run produces

`.github/workflows/release.yml` reserves the tag, calls `package.yml` once per
architecture, then merges the artifacts and validates them. Each architecture
must contribute exactly one MSI, one NSIS setup executable, and one updater
signature. `scripts/verify-release-assets.ps1` enforces that and writes
`SHA256SUMS.txt`; `scripts/build-updater-manifest.mjs` writes `latest.json`,
which is where the updater reads the per-architecture installer URL and its
minisign signature.

The `.sig` files are not published. The updater reads the signature text out of
`latest.json` and never fetches a separate file.

## Proving a change to the pipeline without spending a version

`.github/workflows/release-preflight.yml` runs both package jobs, merges the
artifacts, and runs the same asset verification, checksum, and manifest code the
release runs — then stops. It reserves no tag, creates no release, and holds
`contents: read` only.

Both workflows call the same two scripts rather than each carrying a copy of the
logic, because a preflight that validated assets differently from the release
would pass while the release failed.

## Who may run a privileged job

Host trust is the logins `PeterShanxin` and `ianmeowmeow`, not write access in
general. Every job that can reach `TAURI_SIGNING_PRIVATE_KEY`,
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, or `RELEASE_MIRROR_TOKEN` carries a
job-level `if:` on those two actors and excludes `dependabot[bot]`. GitHub
evaluates that `if:` before dispatch. A fail-closed step repeats the allowlist
as the job's first action, so a workflow edit that loosens the `if:` still stops
before checkout.

Packaging and release workflows have no `pull_request` trigger. Contributor and
fork code therefore never runs in a job holding a signing key; a reusable
workflow inherits no secrets unless its caller passes `secrets: inherit`, and
`package.yml` fails immediately with a named error when the key arrives empty.

The Stage 2 gate in `test.yml` runs fork and Dependabot code and must never
interpolate any of those secrets.

## Real-device validation is not a CI job

A GitHub-hosted ARM64 runner is a virtual machine. It cannot capture a screen,
place an overlay, exercise Windows OCR installation, or run the Adreno engine
path, so no Actions workflow can honestly claim to verify this application on a
Snapdragon device.

That evidence comes from the manual Windows gate described in
[`CONTRIBUTING.md`](../CONTRIBUTING.md) and
[`docs/CHANGE_CONTRACT.md`](CHANGE_CONTRACT.md): a human runs the change on real
hardware and records the commit, the architecture and Windows build, the
scenario, and the observed result.

The automated half of that local check is `scripts/verify.ps1`, which is what
the hosted gate runs. On an ARM64 machine it covers both shipped architectures,
because Windows x64 emulation runs one way only:

```powershell
./scripts/verify.ps1 -Stage All -Target aarch64-pc-windows-msvc
./scripts/verify.ps1 -Stage All -Target x86_64-pc-windows-msvc
```

Running it locally proves the same things CI proves. It does not replace the
manual gate, and the manual gate does not replace it.
