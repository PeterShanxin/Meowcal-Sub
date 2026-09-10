# Evidence: Stage 2 gate wall time (issue #184)

Date: 2026-09-10
Scope: the wall time of `.github/workflows/test.yml`, and whether parallel
rustc reproduces `STATUS_STACK_BUFFER_OVERRUN` on hosted `windows-11-arm`.

## Method

Job durations come from the Actions jobs API (`completed_at - started_at`), not
from the run summary, so queue time is excluded and each job is comparable
across runs. Step durations come from the same payload. Every run below is
GitHub-hosted and full scope.

## Before

Run 34479689279, push to `main` at `13a0c0f`, cold. The three pull request runs
immediately before it (34476740300, 34472329116, 34469199362) have the same
shape.

| Job | Runner | Wall |
| --- | --- | ---: |
| Lint & Format (Windows full) | `windows-11-arm` | 15.0 min |
| Tests (Windows full) | `windows-11-arm` | 28.0 min |
| Frontend & Browser (Windows full) | `windows-11-arm` | 15.0 min |
| **Critical path** | | **28.0 min** |

Where that went, by step:

| Step | Seconds |
| --- | ---: |
| Lint, aarch64 | 628 |
| Lint, x64 | 255 |
| Test, aarch64 | 1011 |
| Test, x64 under emulation | 631 |
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

## After, cold cache

Run 34494716189, first run of the change, every cache entry a miss.

| Job | Runner | Wall |
| --- | --- | ---: |
| Lint & Format (ARM64) | `windows-11-arm` | 5.5 min |
| Lint & Format (x64) | `windows-2025` | 5.3 min |
| Tests (ARM64) | `windows-11-arm` | 9.4 min |
| Tests (x64) | `windows-2025` | 9.4 min |
| Frontend & Browser (Windows) | `windows-11-arm` | 7.7 min |
| **Critical path** | | **9.4 min** |

Per verification step, against the same step before:

| Step | Before | After cold |
| --- | ---: | ---: |
| Lint, aarch64 | 628 s | 231 s |
| Lint, x64 | 255 s | 234 s |
| Test, aarch64 | 1011 s | 444 s |
| Test, x64 | 631 s | 433 s |
| Frontend stage, whole | 887 s | 370 s |

Nothing is cached in this run, so the aarch64 improvements are the
de-serialization on its own: 2.7x on lint, 2.3x on tests, 2.4x on the frontend
stage. The x64 figures also change hardware, from a cross-build executed under
the ARM64 host's emulation to a native `windows-2025` run.

## After, warm cache

Run 34497176385, every entry an exact hit. The warm run before it,
34496029684, agrees within a minute on every job.

| Job | Wall | Toolchain and restore | Verification step |
| --- | ---: | ---: | ---: |
| Lint & Format (x64) | 2.1 min | 62 s | 52 s |
| Lint & Format (ARM64) | 2.7 min | 66 s | 75 s |
| Tests (x64) | 3.8 min | 67 s | 148 s |
| Tests (ARM64) | 4.1 min | 82 s | 146 s |
| Frontend & Browser (Windows) | 5.4 min | 82 s | 222 s |
| **Critical path** | **5.4 min** | | |

Restoring costs 30-40 s more than the cold toolchain step and removes minutes
of compilation. An exact hit writes nothing back, so the save step is 1 s.

The frontend job is the slowest of the five. Of its 222 s, 130 s is the browser
smoke and 49 s is `npm ci`: the cache holds the dependency tree, but the crate
under test is rebuilt and linked every run, which is what the `cargo run`
behind the smoke has to do.

| | Before | After cold | After warm |
| --- | ---: | ---: | ---: |
| Critical path | 28.0 min | 9.4 min | 5.4 min |
| Run wall | 28.5 min | 9.7 min | 5.7 min |

## Parallel rustc on hosted `windows-11-arm`

The hosted image reports four processors, so the gate runs
`CARGO_BUILD_JOBS=4`. Three ARM64 jobs compiled the full dependency tree that
way against a cold target directory and all three passed, with no
`STATUS_STACK_BUFFER_OVERRUN` (`0xc0000409`). The failure the `verify.ps1`
default guards against did not reproduce here, so the hosted gate overrides
that default and the local one stays as it is.

## Cache footprint

Five entries, one per Windows job, 3.4 GB in total:

| Entry | Size |
| --- | ---: |
| `test_x64` | 852 MiB |
| `test_arm64` | 838 MiB |
| `frontend_windows` | 622 MiB |
| `lint_x64` | 565 MiB |
| `lint_arm64` | 557 MiB |

A repository gets 10 GB of Actions cache, shared here with the npm and CodeQL
entries, and GitHub evicts least-recently-used. Branches each writing their own
set would evict one another, so only `main` writes: the key carries no branch,
so a pull request restoring `main`'s entry gets the same exact hit it would
otherwise have written. The first `main` run after this lands is therefore a
cold one, and it is the run that populates the entries.

That is a budget decision, not the isolation boundary. A pull request's writes
land on its own ref either way - the five entries this branch wrote all carry
`refs/pull/188/merge` - so `main` never restores what a fork wrote.
