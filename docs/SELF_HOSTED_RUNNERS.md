# Self-hosted runners

Meowcal Sub runs its CI and Windows packaging on self-hosted Windows runners that
the repository owner controls. This document is the contract for those machines:
who may attach one, what the host must provide, and what happens when it is gone.

Workflow-side detail lives in `.github/workflows/`. The rationale and the
measurements behind the design are in issue #82.

## If you are a contributor, you do not need a runner

**Do not register your personal computer as a runner.** A self-hosted runner
executes repository code directly on the host, and attaching one is a decision
about who may run code on whose machine. It is not a way to speed up your own
pull request.

Run the same gate CI runs, locally:

```powershell
.\scripts\verify.ps1
```

That is the contributor handoff gate and it is the closest equivalent to CI. If
you want to mirror CI exactly, run the per-architecture form CI uses:

```powershell
.\scripts\verify.ps1 -Stage Lint -Target aarch64-pc-windows-msvc
.\scripts\verify.ps1 -Stage Lint -Target x86_64-pc-windows-msvc
.\scripts\verify.ps1 -Stage Test -Target aarch64-pc-windows-msvc
.\scripts\verify.ps1 -Stage Test -Target x86_64-pc-windows-msvc
.\scripts\verify.ps1 -Stage Frontend
```

On an x64 machine the `aarch64` lines will refuse to run, and that refusal is
correct: Windows x64 emulation runs one way only, so an x64 host cannot execute
ARM64 test binaries. Your pull request still gets both architectures from CI.

When no runner is online, your job **queues**. It is not lost and it is not
failing. GitHub cancels a job that stays queued for 24 hours.

## Who may attach a runner

Only the repository owner, or a maintainer the owner has **explicitly approved
for this purpose**. Approval to contribute code is not approval to attach a
runner.

The reason is narrow and worth stating plainly: anyone who can open a pull
request against this repository can cause arbitrary code to execute on every
attached runner. Collaborator trust and host trust are the same trust. A runner
is privileged infrastructure, closer to a deploy key than to a build cache.

### Never attach these runners to a public repository

This design depends on the repository being **private**, so that only invited
collaborators can open pull requests. There is no anonymous fork-pull-request
path onto the host today, and that is the precondition for the whole
arrangement.

If this repository is ever made public, **remove the runners first**. A public
repository accepts pull requests from anyone, GitHub will happily run them on a
self-hosted runner, and the result is a stranger executing code on a maintainer's
machine. The same applies to attaching these runners to any other repository, or
registering them at organization scope where another repository could schedule
work on them. Register at **repository scope only**.

## Runner labels

Workflows select runners by custom label, never by the bare `self-hosted` label
and never by a GitHub-hosted label.

| Label | Used by | Host requirement |
| --- | --- | --- |
| `meowcal-ci` | `test.yml` lint, tests, frontend | **ARM64 only** |
| `meowcal-package-x64` | `package.yml` when `architecture: x64` | ARM64 or x64 |
| `meowcal-package-arm64` | `package.yml` when `architecture: arm64` | ARM64 only |

`meowcal-ci` requires an ARM64 host because CI covers both shipped
architectures from one machine. The crate compiles genuinely different code per
architecture — see the `cfg(target_arch)` split in
`src-tauri/src/engine_launch.rs` — and `docs/AGENT_GUIDE.md` forbids treating one
architecture's evidence as the other's. An ARM64 host can build and *execute*
both; an x64 host cannot execute ARM64 binaries and so cannot supply that
evidence.

`scripts/setup-self-hosted-runner.ps1` enforces this. It refuses `-Role ci` on an
x64 host rather than registering a runner that would quietly halve CI coverage.

One host may carry all three labels. Splitting packaging onto architecture-native
hosts later is a registration change and needs no workflow edit.

## Machine prerequisites

Checked automatically; this list is what the check enforces.

- Windows 11 build 22621 or newer. `OverlayHost` targets
  `net9.0-windows10.0.22621.0`.
- Windows ARM64 or x64.
- PowerShell 7. Workflows use `shell: pwsh`.
- Git for Windows.
- Node.js and npm at the majors declared in `package.json` `engines`.
- Rust stable with the `rustfmt` and `clippy` components.
- The Rust targets the host's labels require.
- Visual Studio Build Tools with Desktop development with C++, including the
  MSVC cross-linker for every target the host will build, and the Windows SDK.
- .NET 9 SDK or newer, for packaging hosts only.
- Google Chrome, for `meowcal-ci` hosts only. The browser smoke uses the system
  Chrome channel, and `npm ci --ignore-scripts` never downloads a browser. Set
  `MEOWCAL_BROWSER_CHANNEL` to use a different installed channel.
- About 40 GB free. Two Rust target directories and the npm cache are large.
- Long paths enabled is advisory but recommended; deep cargo and npm paths can
  exceed `MAX_PATH`.

Check without changing anything:

```powershell
.\scripts\setup-self-hosted-runner.ps1 -Mode Check
```

It reports every missing prerequisite in one pass, with the command that fixes
each, and exits non-zero if any are missing. It installs nothing.

## Registering a runner

Requires repository admin rights and an authenticated `gh`.

```powershell
.\scripts\setup-self-hosted-runner.ps1 -Mode Check
.\scripts\setup-self-hosted-runner.ps1 -Mode Install -Role all -InstallService
```

`-Mode Install` re-runs every prerequisite check and refuses to continue if any
fail, downloads the runner package matching the host architecture, verifies its
published SHA-256, registers it with the resolved labels, and prints the
resulting runner status.

Useful switches:

| Switch | Effect |
| --- | --- |
| `-Role ci` \| `package` \| `all` | Which labels to claim. Default `all`. |
| `-RunnerDirectory` | Default `C:\actions-runner\meowcal-sub`. |
| `-RunnerName` | Default `<COMPUTERNAME>-meowcal`. |
| `-InstallService` | Register as a Windows service that starts with the machine. |
| `-RunnerVersion` | Pin a runner version instead of taking the latest. |

### Registration tokens

The script mints a registration token through your authenticated `gh` session
and holds it in memory only. Registration tokens expire in one hour.

**Never commit a registration token, a removal token, a PAT, or a signing key.**
Nothing in this repository should ever contain one. If you must supply a token
yourself, pass it through the `RUNNER_REGISTRATION_TOKEN` environment variable
for the single command, and do not persist it to a profile or a file.

## Isolation

The runner directory is deliberately outside any repository checkout, because a
workspace inside one could be reached by `git clean -ffdx`.

Recommended, in rough order of value:

- Run the runner service under a **dedicated local account**, not your
  interactive account, and give that account no more than it needs.
- Keep the runner directory out of any synced folder (OneDrive, Dropbox).
- Keep unrelated personal credentials off the host: no personal SSH keys, cloud
  credential helpers, password manager CLIs, or `gh` sessions configured for the
  runner account.
- Do not reuse a machine that holds unrelated production secrets.
- Prefer a dedicated machine or VM if the host would otherwise hold anything you
  would not want a pull request to read.

Secret scoping is enforced in the workflows and should stay that way:

- The updater signing key reaches packaging jobs only.
- `RELEASE_MIRROR_TOKEN` never reaches a self-hosted host at all. The job that
  holds it stays on a GitHub-hosted Linux runner on purpose — see below.

## Workspace reuse, and why CI and packaging differ

CI checks out with `clean: false`. The runner workspace persists between runs, and
a clean checkout would run `git clean -ffdx` and delete `src-tauri/target` and
`node_modules`, forcing a cold Rust rebuild every run. Tracked files are still
force-checked-out; only ignored build output survives.

Packaging keeps the default clean checkout. A release build must not reuse
incremental artifacts, and the extra minutes are local compute.

That difference has one consequence worth knowing: if the same runner instance
serves both roles, a packaging run wipes the CI workspace and the next CI run
rebuilds cold. If that becomes annoying, register **two runner instances** in
separate directories — one `-Role ci`, one `-Role package` — so each keeps its
own `_work`.

## Service lifecycle

### The service account matters more than it looks

`rustup` installs into the **user** profile — `%USERPROFILE%\.cargo` and
`%USERPROFILE%\.rustup` — and adds itself to the user PATH only. A runner service
left on its default account, `NT AUTHORITY\NETWORK SERVICE`, therefore cannot see
`cargo` at all, and its `%USERPROFILE%\.cargo` is a different, empty directory.
Every Rust job then fails on a machine whose interactive shell builds the project
perfectly, which is a confusing place to begin debugging.

Install the service under the account that owns the toolchain:

```powershell
.\scripts\setup-self-hosted-runner.ps1 -Mode Install -Role all -InstallService `
  -ServiceAccount 'MACHINE\username'
```

`config.cmd` prompts for that account's password itself. This repository's
scripts never accept, store, or log a Windows password. `-Mode Check` reports
this as an advisory whenever `cargo` is user-scoped.

The alternative, if you would rather not run a service under your own account, is
to run the runner in the foreground, which inherits your environment as-is:

```powershell
C:\actions-runner\meowcal-sub\run.cmd
```

A foreground runner works exactly like the service for job execution. It just
stops when you close the window or log out.

With `-InstallService`, the runner is a Windows service named
`actions.runner.<owner>-<repo>.<runner-name>`.

```powershell
Get-Service actions.runner.*                       # status
Stop-Service actions.runner.*                      # stop; running jobs finish
Start-Service actions.runner.*
```

Without the service, run it in the foreground and stop it with Ctrl+C:

```powershell
C:\actions-runner\meowcal-sub\run.cmd
```

A stopped runner does not fail jobs. They queue.

### Updating

The runner self-updates when GitHub requires a newer version, so routine updates
need nothing from you. To force one, remove and re-register.

### Replacing or re-labelling

Re-run `-Mode Install`. The script passes `--replace`, so re-registering the same
name updates the labels rather than creating a duplicate.

### Removing

```powershell
.\scripts\setup-self-hosted-runner.ps1 -Mode Remove
```

This mints a removal token and unregisters the runner. Delete the runner
directory afterwards if you are decommissioning the host.

Revoke immediately if the host is lost, shared, sold, or compromised. Removing
the runner from the repository is the revocation; a runner that cannot
authenticate cannot take jobs.

## Checking status

```powershell
.\scripts\setup-self-hosted-runner.ps1 -Mode Status
```

Lists every registered runner with its status, whether it is busy, and its
labels. If nothing is listed, self-hosted jobs will queue until a runner comes
online.

## Jobs stay queued rather than falling back

No workflow names a GitHub-hosted **Windows** runner, and none may. There is no
expression that selects `windows-latest` or `windows-11-arm` when a self-hosted
runner is unavailable.

This is deliberate. An automatic fallback would resume spending hosted minutes
silently, and it would do so hardest exactly when the host is down and nobody is
watching. Queuing is visible; a bill is not. Returning to hosted runners is a
reviewed revert of the workflow change, never an automatic failover.

### The jobs that stay hosted, and why

Three jobs remain on `ubuntu-latest`:

| Job | Reason |
| --- | --- |
| `release.yml` / `validate` | Linux, 1x billing, about a minute; holds `contents: write` to reserve the release tag |
| `release.yml` / `draft-release` | Linux, 1x billing; holds release-write permission |
| `publish-update.yml` / `publish` | Holds `RELEASE_MIRROR_TOKEN` |

`RELEASE_MIRROR_TOKEN` can publish to the endpoint every installed copy checks
for updates. Keeping the job that holds it on an ephemeral hosted runner keeps it
off a long-lived machine that also executes contributor pull request code.
Moving these would save about a minute per release and would put the most
dangerous credential in the system on the least ephemeral host in it.

## Troubleshooting

**A job stays queued forever.** No online runner carries every requested label.
Check `-Mode Status`. Remember `meowcal-ci` requires all of `self-hosted`,
`Windows`, `ARM64`, and `meowcal-ci`.

**Rust fails with `STATUS_STACK_BUFFER_OVERRUN` (`0xc0000409`) on several
unrelated dependencies.** Parallel rustc exhausting its compiler stack on ARM64,
not a dependency problem. `verify.ps1` and `build-package.ps1` both set
`CARGO_BUILD_JOBS=1` on an ARM64 host; if you invoked cargo directly, set it
yourself.

**The browser smoke cannot find a browser.** Install Google Chrome, or set
`MEOWCAL_BROWSER_CHANNEL` to an installed channel such as `msedge`.

**A build fails on a missing `resources\OverlayHost.exe`.** Run
`scripts\prepare-validation-resources.ps1`. A fresh workspace needs it before its
first build.

**Packaging fails to link the other architecture.** The MSVC cross-linker for
that target is not installed. Run `-Mode Check`, which names the exact linker
path it looked for.

**The workspace is in a strange state.** Stop the runner, delete
`<runner>\_work\Meowcal-Sub`, and start it again. The next run re-clones.

**Disk fills up.** Two Rust target directories are the usual cause. Stop the
runner and delete `_work` to reclaim, accepting one cold rebuild.
