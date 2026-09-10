# Evidence: Stage 2 gate wall time (issue #184)

Date: 2026-09-10
Scope: the wall time of `.github/workflows/test.yml`, and whether parallel
rustc reproduces `STATUS_STACK_BUFFER_OVERRUN` on hosted `windows-11-arm`.

## Method

Job durations come from the Actions jobs API
(`completed_at - started_at`), not from the run summary, so queue time is
excluded and each job is comparable across runs. Step durations come from the
same payload. Every run below is GitHub-hosted, full scope, on a checkout with
no Rust cache unless the row says warm.

## Before

Run 34479689279, push to `main` at `13a0c0f`. Same shape on the three
pull request runs immediately before it (34476740300, 34472329116).

| Job | Runner | Wall |
| --- | --- | ---: |
| Lint & Format (Windows full) | `windows-11-arm` | 15 min |
| Tests (Windows full) | `windows-11-arm` | 28 min |
| Frontend & Browser (Windows full) | `windows-11-arm` | 15 min |
| **Gate critical path** | | **28 min** |

Where the time went, from the step timings of that run:

| Step | Seconds |
| --- | ---: |
| Lint, aarch64 target | 628 |
| Lint, x64 target | 255 |
| Test, aarch64 target | 1011 |
| Test, x64 target under emulation | 631 |
| Frontend stage, browser bridge smoke | 784 |
| Frontend stage, everything else | 103 |

The browser smoke figure is a Rust build, not a browser: Playwright starts
`npm run dev:backend`, which is `cargo run` against a cold target directory.

Three causes, each measurable on its own:

1. no Rust cache, so every job compiles the whole dependency tree;
2. `verify.ps1` serializes rustc when an ARM64 host meets a cold target
   directory, which a CI checkout always is, so all of that compiles on one
   core;
3. the two targets run one after the other inside one job, so the gate's wall
   time is their sum.

## After

To be recorded from the pull request's own runs.

## Parallel rustc on hosted `windows-11-arm`

To be recorded.
