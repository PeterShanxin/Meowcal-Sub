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
| `windows-11-arm` | The aarch64 and frontend jobs of the Stage 2 verify gate, and ARM64 packaging |
| `windows-2025` | The x64 jobs of the Stage 2 verify gate, and x64 packaging |

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

The standalone [Meowcal Core](../core/README.md) uses the same native runner
mapping through `.github/workflows/core-package.yml`. Its Cargo version is
independent of the application version. A Core release does not change the
application package, application tag, updater manifest, or product version.

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

## Standalone Core releases

Core version `X.Y.Z` is published under tag `core-vX.Y.Z` with four assets:

- `meowcal-core-vX.Y.Z-windows-x64.zip` and its `.zip.sha256` file
- `meowcal-core-vX.Y.Z-windows-arm64.zip` and its `.zip.sha256` file

Each ZIP contains only `meowcal-core.exe`, `meowcal-core.json`, and `LICENSE`.
The metadata binds the Core version, API version, architecture, executable hash,
and license hash. Packaging checks the PE machine type and runs the native
binary's `--version-json` contract after the Core test suite passes.

Use the Core preflight before publishing:

```powershell
gh workflow run core-release-preflight.yml --ref main
```

After it succeeds on both native runners, publish the matching Cargo version:

```powershell
gh workflow run core-release.yml --ref main -f version=0.1.0
```

`core-release.yml` accepts only `main`, reserves the immutable Core tag, and
creates the GitHub release with `--latest=false`. Only the trusted release
actors listed below can run these jobs. The application release remains the
repository's latest release and its `latest.json` updater contract is untouched.

Both applications pin the released ZIP digest, not the tag alone. Their product
builds fetch that artifact and never rebuild Core from the current source tree.
The fetch step verifies the ZIP digest before reading the archive, then verifies
metadata, license, executable digest, PE architecture, and the native handshake.
Development and source verification build an optimized candidate from `core/`.
Product release builds require the reviewed release lock and its verified asset.

Bootstrap a new Core version in this order:

1. Merge and run `core-release-preflight.yml`, then publish the Core release.
2. Download both published `.zip.sha256` files.
3. Run `scripts/write-meowcal-core-lock.ps1` in this repository and in the Sub 2
   checkout, using those two checksum files.
4. Review and commit the identical version, API, asset names, and digests in
   both `config/meowcal-core.lock.json` files.
5. Run the application preflight workflows. They now consume the immutable Core
   assets selected by those locks.

Before step 4, product packaging fails closed because there is no reviewed lock.
This ordering is required only when a Core version changes; ordinary application
releases keep consuming their existing pin.

## Proving a change to the pipeline without spending a version

`.github/workflows/release-preflight.yml` runs both package jobs, merges the
artifacts, and runs the same asset verification, checksum, and manifest code the
release runs — then stops. It reserves no tag, creates no release, and holds
`contents: read` only.

`.github/workflows/core-release-preflight.yml` applies the same rule to Core: it
packages both architectures and runs `scripts/verify-core-release-assets.ps1`,
but creates neither a tag nor a GitHub release.

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
