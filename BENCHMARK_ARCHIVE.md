# Reusable subtitle benchmark archive

Archived 2026-09-15. This branch preserves the experimental harness at
`ee24711b350a8f1b484af39a232fffd697e475f6`, plus portable evaluation runners.
It is an experiment archive, not a product release or a merge into current main.

## Restore

```powershell
git clone --single-branch --branch archive/benchmark-infra-2026-09-15 https://github.com/PeterShanxin/Meowcal-Sub.git benchmark-infra
Set-Location benchmark-infra
git rev-parse HEAD
git merge-base --is-ancestor ee24711b350a8f1b484af39a232fffd697e475f6 HEAD
```

Record the resulting commit SHA with every new experiment. Original source is
recoverable from the preserved ancestor even if the runners later change.
Tracked lockfiles and the project-authored `evals/subtitle-eval-v1.json` fixture
are included. Build caches, downloaded models and private session data are not.

## Set up a new experiment

Use Windows 11, PowerShell 7, Git, Rust/MSVC with the Windows SDK, and the
checkout's pinned Node/npm versions when running the full project verification.
See [CONTRIBUTING.md](CONTRIBUTING.md) for compiler environment setup.

Prepare the validation resources before compiling the Tauri crate:

```powershell
$env:CARGO_BUILD_JOBS = '1' # ARM64 cold-build safeguard
.\scripts\prepare-validation-resources.ps1
cargo build --locked --release --manifest-path .\src-tauri\Cargo.toml --bin subtitle-eval
```

Obtain a compatible `llama-server` and model separately, from their official
upstreams. Record full SHA-256 hashes, runtime revision, architecture and launch
arguments. Historical model/runtime details are in
[the preparation record](docs/plans/2026-08-08-hy-mt2-winarm-prep.md).
Its truncated MT2 hash is not sufficient to verify a newly downloaded file.
Do not infer that a new model supports this historical prompt/wire contract.

The per-engine and batch runners expose their parameters through
`Get-Help .\scripts\run-engine-eval.ps1 -Full` and
`Get-Help .\scripts\run-bench-batch.ps1 -Full`.
Pass model/runtime locations explicitly. Use a separate evaluation app config;
keep all generated configs, reports and model outputs outside tracked files.
Start with the project-authored fixture, then a small, consented evaluation set.

Example for an isolated candidate (replace the local asset paths):

```powershell
$candidate = @{
    Config = 'c'
    RuntimePath = '.\eval-results\assets\llama-server.exe'
    ModelPath = '.\eval-results\assets\candidate.gguf'
    ModelAlias = 'candidate'
    ServerPort = 11652
    AppConfigPath = '.\eval-results\app-config.json'
    Dataset = '.\evals\subtitle-eval-v1.json'
    Seed = 2026
}
.\scripts\run-engine-eval.ps1 @candidate -PlanOnly
# After inspecting the plan and preparing the assets/config:
.\scripts\run-engine-eval.ps1 @candidate
```

`PlanOnly` prints parameters without loading models or writing result files.
The live invocation starts a local server and compiles/runs the Rust harness.
The batch runner accepts separate B/C runtime, model, alias and server settings,
plus `FullDatasetPaths` and `StabilityDatasetPaths`. Use a fresh `BenchDir` for
each battery; completed runs are recorded after each iteration in its manifest.

The Rust CLI remains the underlying entry point:

```powershell
.\src-tauri\target\release\subtitle-eval.exe --help
```

The binary path above assumes no `CARGO_TARGET_DIR` override. Use that directory
instead when an override is configured. Run the deterministic fixture gate
before opting into live inference.

## Data and analysis

Dataset schema and safe examples: `evals/subtitle-eval-v1.json`.
Reference-free datasets can set `allowUnreferencedCases: true`; this validates
output shape/language, not semantic translation accuracy. Keep stable case IDs,
source/target language, expected actions, references where available, and hashes
of the exact input set. Do not upload private OCR, viewing history or translations.

The batch runner writes a manifest and per-run reports. Aggregate them with:

```powershell
.\scripts\analyze-bench.ps1 -BenchDir <local-benchmark-directory>
```

`aggregate.json` contains sample model outputs and is not a public-safe artifact.
Export only reviewed aggregate fields when sharing results.
`build-crosscheck-pack.ps1` consumes an existing first-pass review pack/key and
results; it is not a standalone dataset-to-blind-review generator. The private
inputs for the historical review are deliberately absent from GitHub.

For future models, screen roughly 50 cases first, expand to targeted noise and
disagreement cases, and run long sessions only for finalists. Hold the runtime,
seed, prompt, sampling, model quantization and server lifecycle constant where
possible. Record changed variables explicitly. Report cold start separately
from warm request latency; do not turn median latency into throughput.

## Historical results (August 2026)

Windows 11 build 26200; Snapdragon X Elite X1E80100; CPU-only, 8 threads,
`-ngl 0`, context 2048, one request slot. Five private sessions, 2,653 lines
per arm. Fifteen full-session runs and thirty stability runs were recorded.

| Arm | Model/runtime | Harness failures | Per-session p50 | Per-session p95 |
| --- | --- | --- | --- | --- |
| A | HY-MT1.5 Q4_K_M, b10155, managed/unseeded | 75/2653 | 413–698 ms | 929–3069 ms |
| B | HY-MT1.5 Q4_K_M, b10327, seed 2026 | 87/2653 | 393–598 ms | 891–1890 ms |
| C | Hy-MT2 Q4_K_M, b10327, seed 2026 | 104/2653 | 360–620 ms | 808–3333 ms |

These are ranges of five per-session percentiles, not pooled percentiles.
Arm C had a 60,043 ms maximum including a timeout. Harness failure is not an
independent human judgment of translation correctness.

The historical-free 50-case blind review reported means A/B/C of
3.38/3.30/3.34. Noise-pool means were 2.30/2.40/1.90. These descriptive figures
do not establish statistical equivalence. The frozen decision was to retain
MT1.5 as the default, given MT2's noise robustness and latency-tail results.
This is a historical decision, not a fresh comparison of today's models.

Detailed methodology: [real-session record](docs/plans/2026-08-08-real-session-benchmark.md).
Public-safe machine-readable metrics: [summary](docs/archive/benchmark-2026-08-summary.json).
The private input datasets, per-case outputs and review sheets remain local;
the archive supports new experiments but cannot alone reproduce those exact
historical sessions or independently re-score their translations.

## Corrections to historical documentation

The old record says the reusable tools were committed to main. Live Git
inspection on 2026-09-15 found this three-commit experiment branch unmerged and
without a remote backup. This archive preserves it without changing main.
The old guide's private-repository statement is also historical: the destination
was verified public before this archive was published.

The historical architecture document says evaluation reports omit translated
text. This experimental harness captures outputs for review, so that statement
does not describe these reports. Treat them as private, as described above.

Only safe aggregate data is included. Privacy-sensitive extraction/review
scripts and raw artifacts are retained locally rather than published.

## Validation scope

Archival integrity and runner checks are documented in the cleanup report.
The no-model runner check is
`pwsh -NoProfile -File scripts/tests/benchmark-runners.Tests.ps1`.
No new live model comparison or native product acceptance is claimed. Before
using this historical checkout as a product change, run its full
`scripts/verify.ps1` gate and any applicable native manual validation.
