# Public-readiness audit

Point-in-time audit of `PeterShanxin/Meowcal-Sub` for epic
[#134](https://github.com/PeterShanxin/Meowcal-Sub/issues/134).
This document is findings only. It does not change repository visibility,
rewrite git history, or apply remediations.

| Field | Value |
|---|---|
| Audit date (UTC) | 2026-08-29 |
| Scanned tip | `ec72a32` (`main` at audit start; subject: Keep social-preview.png in the public showcase export.) |
| History span | 410 commits, 76 merges, 12 tags; first commit `f4198f1` (2026-01-11) |
| Scanners | gitleaks 8.30.1 (`git --log-opts="--all --full-history"` + `dir` at HEAD); trufflehog 3.97.1 (`git file://`); targeted `git grep -I -l` over every commit |
| Visibility | **Do not flip.** This report does not authorize a public launch. |

## Verdict

**SAFE TO REMEDIATE**

No live secret values were found in git history or at HEAD. Nothing from this
scan requires credential rotation or a history rewrite.

The repository is **not** safe to make public at the scanned tip. The
self-hosted `pull_request` CI path would execute untrusted fork code on the
owner's Windows ARM64 machine. Governance files required by Checkpoint A
(`LICENSE`, CLA, `SECURITY.md`, `TRADEMARKS.md`) are also absent. Those are
ordinary follow-up changes. They are not a poisoned-history problem.

Do not change visibility until the before-public list below has landed.

## Secret-history table (redacted)

Scanners reported **zero** leaked credentials. gitleaks: 331 commits scanned
(~6.0 MB; merge commits typically add no unique blobs, which is why this is
below the 410-commit rev-list), no leaks. trufflehog: 4655 chunks / ~6.3 MB,
`verified_secrets=0`, `unverified_secrets=0`. Targeted patterns
(`BEGIN *PRIVATE KEY`, OpenSSH/PGP/age, `AKIA…`, `ghp_`, `github_pat_`,
`hf_`, `npm_`, Slack `xox*`, Cloudflare/R2 assignment names, `TAURI_SIGNING_PRIVATE_KEY=`,
`minisign secret key`, `Bearer …`) produced **zero** file hits across all 410
commits.

No `.env`, `*.pem`, `*.key`, `id_rsa`, `id_ed25519`, `*.p12`, or `*.pfx` path
has ever been in git. No model/runtime blobs (`.gguf`, `.onnx`, `.safetensors`)
have ever been in git. Largest tracked blob is a 1.4 MB product PNG.

Reviewed non-secrets (not leaks; listed so a later scan is not surprised):

| Rule / equivalent | Path | Commit | At HEAD | Recommended action |
|---|---|---|---|---|
| *(none — gitleaks)* | — | — | — | — |
| *(none — trufflehog)* | — | — | — | — |
| GitHub PAT placeholder (`ghp_` + `xxx` only) | `.claude/skills/mcp-builder/reference/evaluation.md` | present in current tree | yes | ignore-false-positive |
| Minisign **public** key (updater `pubkey`) | `src-tauri/tauri.conf.json` | present in current tree | yes | ignore-false-positive (expected public; private key must stay a repository secret and must never be committed) |
| Actions secret **names** only (`secrets.TAURI_SIGNING_PRIVATE_KEY`, `secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, `secrets.RELEASE_MIRROR_TOKEN`) | `.github/workflows/package.yml`, `.github/workflows/publish-update.yml` | present in current tree | yes | ignore-false-positive |
| Test fixture path `C:\Users\tester\…` | `src-tauri/src/app_logging.rs` | present in current tree | yes | ignore-false-positive |
| Documented runner `.env` path on the owner host (file is **not** in git) | `docs/SELF_HOSTED_RUNNERS.md` | present in current tree | yes | ignore-false-positive; keep that file off-repo forever |

If a later scan reports a real credential: rotate it, then decide rewrite vs
leave-and-rotate. This audit forbids rewriting history in the same change as
the report.

## Targeted review

| Area | Result |
|---|---|
| `.env`, `*.pem`, `*.key`, `id_rsa` | Never present in history. `.gitignore` ignores Python `env/` / `venv/` but **does not** ignore `.env`. |
| Cloudflare / R2 | No matches in tracked text. |
| `TAURI_SIGNING_*` | Used only as GitHub Actions secret references. Evidence JSON states a throwaway packaging key was generated outside the repository and never committed. The matching **public** key is in `tauri.conf.json` by design. Rotating the private key without shipping a new public key bricks the updater. |
| Tokens | No GitHub / Hugging Face / npm / Slack token shapes in history. One documentation placeholder (`ghp_` + `xxx`). |
| Screenshots / images | Product icons, logo, social preview, and an abstract UI background. No UI captures, subtitle text, or personal desktop. PNG `tEXt`/`iTXt`/`zTXt` chunks: none. |
| Logs / fixtures | `*.log` is gitignored. `evals/subtitle-eval-v1.json` is project-authored privacy-safe cases. `docs/evidence/*.json` follows that contract (IDs, timings, hashes; no OCR/model text). |
| Personal filesystem paths | Test-only `C:\Users\tester\…`. `.gitignore` mentions `D:/cargo-build/` (host volume layout, not a home directory). Runner docs name `C:\actions-runner\meowcal-sub\` as the owner’s runner directory. |
| Private URLs | GitHub repo and public mirror URLs only. Loopback `localhost` ports in browser-mode docs. No internal/corp/R2 hosts. |
| Model / runtime artifacts | Not in git. `config/engine-manifest.v1.json` points at public Hugging Face / llama.cpp release URLs plus SHA-256. Those hashes are supply-chain pins, not credentials. |
| Git identity | Some historical commits use a personal university author email (redacted). Also GitHub noreply and `noreply@anthropic.com`. Do not rewrite history to hide this. Use `users.noreply.github.com` going forward. |
| Evidence host fingerprint | `docs/evidence/2026-08-07-arm64-host-cross-builds-x64-package.json` records CPU model, RAM, and tool versions of the packaging host. Not a secret. Optional later redaction; not a launch gate. |

## CI threat model

Verified from workflow files, not assumed.

No workflow uses `pull_request_target` or `workflow_run`.

### Public-launch blockers

| Workflow | Trigger | Runner | Why it blocks a public launch |
|---|---|---|---|
| `.github/workflows/test.yml` (`CI`) | `pull_request` and `push` to `main` | `[self-hosted, Windows, ARM64, meowcal-ci]` | Checks out the pull request (`actions/checkout@v4`) and runs `./scripts/verify.ps1` (lint, tests, frontend) from that tree. On a public repository, anyone can open a fork PR and run arbitrary code on the owner’s machine. Confirmed. |
| same | same | same | `clean: false` on every job. The runner workspace is reused. Untrusted code can leave files for the next job. |
| same | same | same | No workflow `permissions:` block. Default `GITHUB_TOKEN` applies. `persist-credentials` is left at checkout default (`true`). Combined with a durable self-hosted workspace, that is the wrong default for untrusted PRs. |

`docs/SELF_HOSTED_RUNNERS.md` already states this design depends on a **private**
repository and tells operators to **remove the runners first** if the repository
becomes public. That warning is correct and still unheeded in `test.yml`.

### Not public-launch blockers (post-public cleanup / maintainer-only)

| Workflow | Trigger | Runner | Secrets | Notes |
|---|---|---|---|---|
| `.github/workflows/change-contract.yml` | `pull_request` (including `edited`) | `ubuntu-latest` | none | `permissions: contents: read`. Runs `scripts/check-commit-contract.mjs` from the PR checkout on an ephemeral GitHub-hosted VM. Normal public-repo pattern. Not a self-hosted execution path. |
| `.github/workflows/package.yml` | `workflow_dispatch`, `workflow_call` | `meowcal-package-x64` / `meowcal-package-arm64` | `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Maintainer-only. Fork PRs cannot start it. Keep it that way. |
| `.github/workflows/release.yml` | `workflow_dispatch` | `ubuntu-latest` (validate + draft) plus `package.yml` | signing secrets via `secrets: inherit` | Maintainer-only. `contents: write` stays on GitHub-hosted Ubuntu, not on the PR runner. |
| `.github/workflows/publish-update.yml` | `workflow_dispatch` | `ubuntu-latest` | `RELEASE_MIRROR_TOKEN` | Maintainer-only. Publishes to `PeterShanxin/Meowcal-Sub-releases`. Mirror/updater split is an epic follow-on, not a visibility gate. Clone step interpolates the token into an `https://x-access-token:…` URL; tighten later so a failed log cannot print it. |

## Governance snapshot (HEAD)

| File | Status |
|---|---|
| `LICENSE` | **Missing.** `README.md` says “MIT.” `package.json` and `src-tauri/Cargo.toml` also declare MIT. Epic #134 wants AGPL-3.0-only plus a commercial path. |
| `CONTRIBUTING.md` | Present. Internal contributor contract. No CLA, no DCO, no AGPL grant. |
| `SECURITY.md` | **Missing.** |
| `TRADEMARKS.md` | **Missing.** |
| CLA / relicensing grant | **Missing.** |

## Recommended before-public remediations

Only items that must land **before** visibility changes. Do not treat this list
as work for this pull request.

1. **Stop untrusted PR code on privileged runners.** Remove `pull_request` from
   `.github/workflows/test.yml`, or gate self-hosted jobs on maintainer
   approval / a trusted-ref check so fork PRs never schedule on
   `meowcal-ci`. `docs/SELF_HOSTED_RUNNERS.md` still applies: do not attach
   these runners to a public repository until that is true. Until then, do
   not flip visibility.
2. **Harden the CI job that remains on the owner host** (if any trusted
   self-hosted path stays): explicit `permissions:` (least privilege),
   `persist-credentials: false`, and `clean: true` (or a disposable workspace)
   so one job cannot seed the next.
3. **Add governance files** required by Checkpoint A: `LICENSE` (AGPL-3.0-only
   per #134), CLA (or equivalent relicensing grant), `SECURITY.md`,
   `TRADEMARKS.md`, and CONTRIBUTING updates so AGPL + CLA match. Align
   `package.json`, `Cargo.toml`, and README license lines with the chosen
   license. Do not leave “MIT” in metadata after switching.
4. **Ignore `.env` in git** (and common key suffixes) so a runner or local
   secret file cannot be added by accident after the repo is public.
5. **Finish the AGPL compatibility pass** for third-party model/runtime
   licenses (HY-MT community terms vs AGPL distribution). This scan did not
   close that gate; #134 still lists it as required before visibility.

Do **not** rewrite git history for the items in this report. Do **not** rotate
the Tauri signing key unless it has actually leaked (it has not, per this
scan). Do **not** change repository visibility from this pull request.

## Out of scope here

- Implementing the remediations above.
- Showcase/README polish.
- Making this repository’s GitHub Releases the canonical updater source.
- Archiving `Meowcal-Sub-releases`.
- Post-public GitHub-hosted Windows CI.
- Dependency-license legal opinion beyond “the gate is still open.”
