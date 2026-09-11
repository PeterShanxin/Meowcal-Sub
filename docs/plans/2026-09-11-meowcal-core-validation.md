# Meowcal Core Windows validation — 2026-09-11

This record describes the initial extraction candidate. Its OCR timings and
artifact hashes predate the binary transport and persistent OCR engine changes;
see [the subsequent OCR comparison](2026-09-11-core-ocr-performance.md) for those
measurements. The native playback and release gates below remain outstanding.

## Scope and environment

Core 0.1.0, API 1, tested on Windows build 26200, ARM64, Qualcomm Adreno
X1-85 GPU, driver 31.0.148.0. The application baselines were Sub 1
`822ce922ae74dca16b216f8fe3ed6de228de07cf` and Sub 2
`a2d13f97254f7489c214154d96d18472affd798a`.

The ARM64 candidate archive contained an executable with SHA-256
`8b2549887c5d452ca14a4bbba123afdfacaf2800afb63b51d4882effefaf623a`.
These probes used isolated development storage and explicit legacy import roots.
They did not modify the existing production installation.

## Native model, migration, and coexistence

A fresh storage directory reported `installed: false`. Offline readiness copied
the verified legacy model and runtime archive, reconstructed and verified the DLL
tree, ran sample inference, and became GPU-ready in **42.942 seconds**, within
Sub 1's unchanged 90-second caller budget. The first completion took 0.912 seconds
and translated `今晚月色很好。` as `The moonlight is beautiful tonight.`

A second Core process, identifying as Sub 2, opened the same installation while
the first remained alive. Readiness took 21.616 seconds; the same translation
completed correctly in 0.637 seconds. Its repair request returned
`CORE_ASSETS_BUSY` after 10.358 seconds while the first process held its lease.
After the first process exited, the second became ready again in 25.639 seconds
and translated correctly in 0.921 seconds. Explicit shutdown and stdin closure
both exited successfully. A consumer requesting version 0.2.0 received
`VERSION_MISMATCH`. No probe-owned Core or model processes remained afterward.

An earlier coexistence probe produced one repetitive, incorrect model answer.
Two subsequent runs, five completions each using normal sampling and greedy
sampling respectively, produced correct answers; the final probe above also
passed. The cause of the isolated answer is unresolved. This evidence does not
establish long-running inference quality or justify a new GPU concurrency policy.

The actual Python consumer also passed cancellation and recovery probes. Cancelling
an active completion retained the same Core PID; the next correct translation
completed in 1.652 seconds. After terminating that owned Core process, the next
translation established a new ready process and completed correctly in 16.560
seconds. Removing a runtime DLL was repaired offline from the retained verified
archive. With both the DLL and archive unavailable and no legacy import root,
readiness rejected the installation and the consumer reported `failed`. Restoring
the verified archive allowed the Settings install operation to reconstruct the
runtime, clear the failure, and translate correctly again.

The final non-Rust Sub 2 verifier passed 648 Python tests (one optional executable
test skipped), 17 frontend tests, format/lint/type checks, dashboard smoke, and
maintainability/coverage gates. The executable test was separately run against
both actual Core package architectures.
The subsequent readiness-cache fix passed all 17 engine tests. The Rust shell
passed Clippy with warnings denied and all 15 tests.

Sub 1 passed 464 Rust unit tests (four opt-in tests ignored), 16 IPC integration
tests, and two command-contract tests. Its actual compiled Rust adapter separately
passed both opt-in Core tests: handshake/status/shutdown, and real model readiness
followed by active-completion cancellation, a correct next translation from the
same process, and shutdown. The latter took 17.15 seconds. Core itself passed
65 unit tests and nine service-contract tests; native GPU/process-lifetime probes
and the hash benchmark were run explicitly outside the ordinary ignored set.

Sub 1 also passed 379 frontend unit tests and four Chrome browser smoke tests,
with final Clippy, formatting, documentation, and maintainability checks passing.
The browser run used a prebuilt backend because the first cold build exceeded
the existing 15-minute startup budget. Browser smoke verifies the HTTP bridge and
setup screens; it does not exercise native capture or the overlay.

## OCR fidelity and latency

Sub 2's existing `ocr_image` policy was compared against its Core-backed adapter
using the same 960 × 160 RGB image, black background, white Arial 48 text:
`Meowcal Core reads this subtitle`. Each group had two warm-up calls and ten
measured calls, in baseline/Core/Core/baseline order.
Each measurement covers the existing `ocr_image` preprocessing and pass-selection
cycle, not just one raw Core protocol request.

| Group                   |    Median |   Maximum |
| ----------------------- | --------: | --------: |
| Original native adapter |  28.74 ms |  32.42 ms |
| Core release adapter    | 110.24 ms | 163.77 ms |
| Core release adapter    | 131.94 ms | 165.78 ms |
| Original native adapter |  26.99 ms |  40.71 ms |

All 40 answers matched. This comparison used the preceding release executable
before the file-hashing optimization; OCR code was unchanged by that optimization.
The process protocol adds measurable overhead. All 20 Core-backed OCR cycles were below
166 ms for this image, but that does not prove the 250 ms cadence for larger
regions, other scripts, or a complete episode. Development launchers build an
optimized Core executable because the debug build exceeded this cadence.

Native language enumeration, English language initialization, blank-image OCR,
and unsupported-language rejection were also exercised on this machine.
Capture, scaling, preprocessing, pass selection, and text acceptance remain
product-owned; this extraction does not claim that either product's policy is
universally better.

## Integrity performance

The canonical 1,133,080,512-byte model was hashed in alternating native/Rust
implementation order. Windows native SHA-256 took 2.493 and 2.315 seconds;
the previous Rust implementation took 8.239 and 9.935 seconds. All four digests
matched the embedded manifest. Core uses incremental native hashing on Windows
and retains verification after copying and before runtime launch.

## Outstanding release evidence

Both release archives passed structure, metadata, license, file-digest, PE-machine,
and executable version/API checks. Both consumer fetch scripts accepted the
actual archive pins. The Python adapter completed a real handshake/status/shutdown
test against each architecture; x64 execution used Windows emulation on this
ARM64 host. The x64 executable also recognized the same English test image
correctly through the real Python client with a 1000 ms native OCR budget.

| Candidate archive                    | SHA-256                                                            |
| ------------------------------------ | ------------------------------------------------------------------ |
| `meowcal-core-v0.1.0-windows-arm64.zip` | `a4b85d5646377da103d30e5c0741f770313cf12fcf95b69fff264c0d126dfd09` |
| `meowcal-core-v0.1.0-windows-x64.zip`   | `6fa9882cae5b4c312e3bb4920cbc8c602b745bfd8b39fa1dc89c46f09c550499` |

These are API and component probes, not a fresh 30-minute episode validation of
either packaged application. Native x64 OCR, GPU inference, installation, and
rollback on x64 hardware remain unverified. Package verification and cross-build
results do not replace those device checks.

Core publication and consumer release pins must use the reviewed release assets.
Local candidate archives and their actual digests support integration testing;
they are not evidence that those assets have been published. The repository's
manual and main-branch release gates remain applicable.
