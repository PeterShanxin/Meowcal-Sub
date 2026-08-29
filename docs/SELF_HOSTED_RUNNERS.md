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
ARM64 test binaries. A same-repo pull request still gets both architectures
from CI. A fork pull request does not: those jobs never schedule on this host.

When no runner is online, a trusted job **queues**. It is not lost and it is not
failing. GitHub cancels a job that stays queued for 24 hours.

## Who may attach a runner

Only the repository owner, or a maintainer the owner has **explicitly approved
for this purpose**. Approval to contribute code is not approval to attach a
runner.

The reason is narrow and worth stating plainly: a same-repo pull request or a
push to `main` still executes on this host. Collaborator write access and host
trust remain the same trust. A runner is privileged infrastructure, closer to a
deploy key than to a build cache.

### Public is allowed only because fork pull requests never land here

These runners may stay attached after the repository is public **only because**
`.github/workflows/test.yml` skips every self-hosted job unless the event is a
`push` or a pull request whose `head.repo.full_name` equals `github.repository`.
GitHub evaluates that job-level `if:` before dispatch, so a fork / untrusted
pull request is never queued onto the owner machine. Workspace reuse
(`clean: false`) is acceptable on that path because untrusted code never runs.

That filter is local to this repository's workflows. **Never attach these
runners to any other public repository.** Do not register them at organization
or enterprise scope, where another repository could schedule work on them.
Register at **repository scope only**.

## Operating model: on-demand foreground

**Standing policy. This is how the runner is operated; it is not a temporary
state to be tidied up.**

The runner is **already registered with GitHub, permanently**. It runs as an
**on-demand foreground process**, started when a self-hosted run is needed and
stopped when that work is done. It is deliberately **not** a Windows service.

- **Do not re-register the runner** for normal work. Registration already
  happened; `-Mode Install` is for a new host, a lost host, or a label change.
- **Do not install it as a Windows service** unless the repository owner asks
  for that explicitly. See [the service account
  section](#the-service-account-matters-more-than-it-looks) for the constraint
  that makes a service more than a convenience toggle.
- **Never fall back to a GitHub-hosted Windows runner** because the runner
  happens to be offline. Starting it is the fix; queuing is the safe default.

### Finding the runner

Do not hard-code the path. The canonical directory is the `-RunnerDirectory`
default on `scripts/setup-self-hosted-runner.ps1`, so read it from there:

```powershell
$runnerDirectory =
    (Get-Help .\scripts\setup-self-hosted-runner.ps1 -Parameter RunnerDirectory).defaultValue
```

Registration status always comes from GitHub, never from a local guess:

```powershell
.\scripts\setup-self-hosted-runner.ps1 -Mode Status
```

### The procedure for a self-hosted CI or packaging run

1. **Check status first.** If the runner is already `online`, skip to step 4 —
   do not start a second one.
2. **Start the existing runner through the sanitized start path.** Do not launch
   `run.cmd` yourself: a foreground runner inherits the environment of whatever
   shell started it, so an IDE or agent session becomes CI configuration. That
   is how a `NODE_OPTIONS` preload and a managed Node 22 once failed `npm ci` on
   a correctly registered runner (#88).

   ```powershell
   .\scripts\setup-self-hosted-runner.ps1 -Mode Start
   ```

   It corrects your environment in two places rather than replacing it: PATH
   becomes the machine-then-user composition, so a shell-local toolchain cannot
   shadow the host's, and Node's preload hooks are dropped. Everything else you
   are carrying is left alone — rebuilding the environment instead was tried and
   produced a runner that could not find Program Files or `LOCALAPPDATA`. It also
   refreshes [the action archive cache](#the-action-archive-cache) first, because
   the runner reads `.env` exactly once at listener start. It then
   refuses to start when the resolved Node/npm majors do not match
   `package.json` engines, and prints the toolchain it will use. Add
   `-VerifyEnvironmentOnly` to see all of that without starting anything.

   Keep enough identity to stop the right process later. The long-lived process
   is `Runner.Listener`, living under the runner directory; matching on that path
   is what distinguishes this runner from any other on the machine:

   ```powershell
   $listener = Get-Process -Name Runner.Listener -ErrorAction SilentlyContinue |
       Where-Object { $_.Path -like "$runnerDirectory\*" }
   ```

3. **Wait until GitHub reports it `online`** before relying on it. A started
   process is not yet a registered, connected runner.
4. **Trigger the work, or let an existing queued run service itself.** If a run
   is already queued because the runner was offline, bringing the runner online
   drains that queue. **Do not re-trigger it** — that produces a duplicate run
   competing for the same single runner.
5. **Wait for every relevant job to finish**, then inspect results. With one
   runner, jobs execute sequentially, so a run with three jobs is three
   sequential waits, not one.
6. **Stop the runner only when nothing relevant is left.** Check both that the
   runner is not `busy` and that no queued or in-progress job remains that
   should complete — including runs you did not trigger:

   ```powershell
   gh api repos/PeterShanxin/Meowcal-Sub/actions/runs `
     --jq '.workflow_runs[] | select(.status != "completed") | "\(.id) \(.name) \(.status)"'
   ```

   Then stop the process you recorded in step 2:

   ```powershell
   $listener | Stop-Process
   ```

**Never stop a busy runner.** Killing the listener mid-job fails that job and
leaves the workspace part-written. If several unrelated jobs are queued, let them
drain rather than stopping after the first one finishes.

A stopped runner does not fail anything. Later jobs queue until it is started
again, and GitHub cancels a job that stays queued for 24 hours.

## The action archive cache

**The runner deletes `_work\_actions` at the start of every job.** That is not a
setting; it is what `ActionManager.PrepareActionsAsync` does when a job begins:

```csharp
// We are running at the start of a job
if (rootStepId == default(Guid))
{
    IOUtil.DeleteDirectory(HostContext.GetDirectory(WellKnownDirectory.Actions), …);
}
```

The runner does keep a per-action watermark that short-circuits a repeat
download — but it lives inside the directory being deleted, so it can never
survive into the next job. Every job on this host therefore re-downloaded
`actions/checkout` from `codeload.github.com`: 302 of 302 recorded jobs, zero
cache hits (#132).

That download happens during job **initialization**, before any step exists.
Nothing in a workflow can retry it — not `timeout-minutes`, not a retry loop in a
`run:` block. The runner tries three times and then fails the job with `Caught
exception from JobExtension Initialization`, naming the action, which reads like
a broken workflow rather than an infrastructure limit. `test.yml` runs three
self-hosted jobs and each one checks out, so one CI run asked codeload for the
same bytes three times.

The runner's own escape hatch is a directory it consults before the network:

```
# C:\actions-runner\meowcal-sub\.env
ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE=C:\actions-runner\meowcal-sub\action-archive-cache
```

`ActionManager` then looks for `<cache>\<owner>_<repo>\<resolved-sha>.zip` and
copies it instead of asking codeload. (The same code writes `.tar.gz` on Linux;
only the `#if OS_WINDOWS` branch differs.)

`scripts/sync-action-cache.ps1` keeps that directory current, and `-Mode Start`
runs it, so the ordinary start path needs nothing extra. Run it by hand to
inspect or repair:

```powershell
.\scripts\sync-action-cache.ps1 -WhatIfOnly   # show the plan, change nothing
.\scripts\sync-action-cache.ps1              # download, verify, prune, write .env
```

What it does, and why each part is the way it is:

- **The list comes from the workflows**, not from a list in a script. It reads
  every job whose `runs-on` selects one of our hosts and collects the actions
  those jobs use, so adding a step that uses a new action extends the cache with
  no second edit. A hosted Linux job's actions are deliberately excluded: the
  cache is a property of this machine and that job cannot read it.
- **Archives are named by the resolved commit SHA**, never the ref. A moved tag
  therefore misses the cache and re-downloads, rather than serving stale bytes
  under a name that no longer means them.
- **The repository name comes from the API**, not from the workflow text. A
  renamed repository still answers under its old name, and the runner files the
  archive under the name it *resolved*.
- **The bytes are verified, including entries already present.** A rate-limit
  page or a truncated response is a perfectly good file of plausible length, and
  a corrupt archive in the cache fails *every* job — worse than the download it
  replaces. A size check would make that damage permanent and invisible.
- **It never restarts the runner.** `.env` is read once, in
  `Program.LoadAndSetEnv`, when the listener starts; restarting a live listener
  can sever a dispatched job and there is no drain API. This runner is started on
  demand, so the next start picks the setting up — and until it does,
  `C:\actions-runner\meowcal-sub\.env` existing is not proof the running listener
  read it.

Two things worth knowing before you judge whether it is working:

- **The job log does not tell you.** The runner prints `Download action
  repository 'actions/checkout@v4' (SHA:…)` *before* it consults the cache, so
  that line appears either way. The real signal is `Found action archive '<file>'
  in cache directory '<dir>'` in `<runner>\_diag\Worker_*.log`.
- **The cache directory is written by the owner's tooling and read by every
  job.** Every job on this runner executes trusted same-repo or `main` code as
  the account that started the runner, so a job could write there. That is a
  property of the on-demand foreground model, not something this cache
  introduces; it is one more reason to start the runner only when work needs it.
  For the same reason, do not point a second repository's runner at this
  directory — sharing it would widen the set of code that reads it, to save a
  few megabytes.

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

**The runner on this machine is already registered.** This section is for a new
host, a replaced host, or a label change — not for day-to-day work. To use the
existing runner, follow [the operating model](#operating-model-on-demand-foreground)
instead.

Requires repository admin rights and an authenticated `gh`.

```powershell
.\scripts\setup-self-hosted-runner.ps1 -Mode Check
.\scripts\setup-self-hosted-runner.ps1 -Mode Install -Role all
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
| `-InstallService` | Register as a Windows service. **Not the operating model here** — only on the owner's explicit request. |
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

- Be aware of what the on-demand foreground model costs here: the runner inherits
  the interactive account that started it, so anything that account can reach, a
  trusted pull request's build scripts can reach too, for as long as the runner
  is running. Fork pull requests never reach this host; see
  [public is allowed only because fork pull requests never land
  here](#public-is-allowed-only-because-fork-pull-requests-never-land-here).
  Starting it only when work needs it and stopping it afterwards is what bounds
  that window, which is a reason to follow [the operating
  model](#operating-model-on-demand-foreground) rather than leave it running.
- A **dedicated local account** narrows that reach further, and is the stronger
  isolation if the host holds anything sensitive. It requires a service, so it is
  the owner's call — see
  [running it as a service](#running-it-as-a-windows-service-if-that-is-ever-asked-for).
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
force-checked-out; only ignored build output survives. That reuse is acceptable
because untrusted fork pull requests never run on this host.

Packaging keeps the default clean checkout. A release build must not reuse
incremental artifacts, and the extra minutes are local compute.

That difference has one consequence worth knowing: if the same runner instance
serves both roles, a packaging run wipes the CI workspace and the next CI run
rebuilds cold. If that becomes annoying, register **two runner instances** in
separate directories — one `-Role ci`, one `-Role package` — so each keeps its
own `_work`.

## Running it as a Windows service, if that is ever asked for

**Not the current model.** The runner is operated on demand in the foreground,
per [the operating model](#operating-model-on-demand-foreground). Do not install
a service unless the repository owner explicitly asks. This section records what
that would take, and why it is not a one-switch decision.

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

This constraint is a large part of why the foreground model is the one in use: a
foreground runner inherits your environment as-is, so it needs no password and no
account decision. It executes jobs identically to a service; it simply stops when
you stop it or log out, which is exactly the intent.

With `-InstallService`, the runner is a Windows service named
`actions.runner.<owner>-<repo>.<runner-name>`.

```powershell
Get-Service actions.runner.*                       # status
Stop-Service actions.runner.*                      # stop; running jobs finish
Start-Service actions.runner.*
```

Starting and stopping the foreground runner, which is the model actually in use,
is covered in [the operating
model](#operating-model-on-demand-foreground) — including how to find the runner
directory without hard-coding it and how to avoid stopping a busy runner.

A stopped runner does not fail jobs. They queue.

### Updating

The runner self-updates when GitHub requires a newer version, so routine updates
need nothing from you. To force one, remove and re-register.

### Replacing or re-labelling

Re-run `-Mode Install`. The script passes `--replace`, so re-registering the same
name updates the labels rather than creating a duplicate. This is for a changed
host or a changed label set only — re-registering is not part of using the
runner, and normal work must not do it.

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

Four jobs remain on `ubuntu-latest`:

| Job | Reason |
| --- | --- |
| `release.yml` / `validate` | Linux, 1x billing, about a minute; holds `contents: write` to reserve the release tag |
| `release.yml` / `draft-release` | Linux, 1x billing; holds release-write permission |
| `publish-update.yml` / `publish` | Holds `RELEASE_MIRROR_TOKEN` |
| `change-contract.yml` / `Change Contract` | Reads Git metadata only; reports a misnamed commit or pull request title in seconds without starting the single Windows runner, and never queues ahead of a real build |

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

**A job fails during initialization naming `actions/checkout`.** codeload
refused the download, and no workflow-level retry can reach that point. Check
that [the action archive cache](#the-action-archive-cache) is populated and that
the listener was started after `.env` gained the setting — `.env` is read once,
at listener start. `.\scripts\sync-action-cache.ps1 -WhatIfOnly` shows what
should be there.

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
