# Microsoft Store distribution

The Store channel uses MSIX. It is separate from the GitHub MSI/NSIS channel;
Store packages do not contribute to `latest.json` or use the Tauri updater key.
Store publication still requires a reserved application identity and certification.

## Build

Use the repository's Windows Rust/Node prerequisites and Windows SDK MakeAppx.
The standard Tauri capture and overlay need no .NET installation. The experimental
WinUI OverlayHost is not included or enabled in Store builds.

```powershell
./scripts/build-store.ps1 -Architecture arm64 `
  -PackageName '<Package/Identity/Name>' `
  -Publisher '<Package/Identity/Publisher>' `
  -PublisherDisplayName '<PublisherDisplayName>' `
  -PackageVersion '<major.minor.build.0>'
```

Copy identity values exactly from Partner Center. The package version is an
explicit, monotonically increasing Store submission number: its major must be
nonzero, all components must fit 16 bits, and its fourth component must be zero.
It is separate from the product version in `tauri.conf.json`; do not bump the
product version to build a Store package. Use the same package version and
identity for both architectures of a submission.

Keep the application version aligned across distribution channels when they
ship the same release. The Store package number is an independently increasing
delivery number, not a declaration that the product reached version 1.0. A
pre-1.0 application cannot copy its version directly into the Store identity,
because the Store requires a nonzero first component. Record product version,
package version, source commit and artifact hashes together for each submission.
Store certification may delay availability relative to the GitHub release.

The script fetches and verifies the reviewed Core release, runs Store-feature
library tests and the real Core handshake, builds through the pinned Tauri CLI,
checks both PE architectures, and invokes SDK MakeAppx validation. It emits an
unsigned `.msix`, `SHA256.json`, and a loose `layout` directory. Each run uses a
new output directory to prevent stale files entering a package. `-CargoTargetDir`
selects a reusable build cache; `-OutputDirectory` must name a nonexistent path.

`.github/workflows/store-package.yml` provides the same build on native x64 or
ARM64 hosted runners, with explicit identity/version inputs and trusted-actor
checks. It uploads artifacts only. It has no Store credentials and does not
submit, publish, or modify a GitHub release. Run both architectures before a
submission; neither result proves the other architecture.

The Store signs MSIX packages during publication. Downloaded runtime executables
and GitHub installers still need their own trust assessment. A valid package
signature does not guarantee that Defender will accept every runtime sample.

## Updates and data

The `store` Cargo feature removes the updater and restart plugins. The Store
Tauri configuration excludes their capabilities and updater endpoints. Settings
explains that updates come from the Microsoft Store Library; it does not offer
GitHub update checks, downloads, or an automatic-check preference.

Store builds use `com.meowcal.sub.store`; debug Store builds use
`com.meowcal.sub.store.dev`. Configuration, logs and WebView data are separate
from `com.meowcal.sub` and `com.meowcal.sub.dev`. They start with their own setup;
there is no automatic import of another channel's settings.

The Store's default Core base is `<package LocalCache>/<Store profile>/Core`, retaining Core's
`sub1/<profile>/<version>/<architecture>` partition structure. This keeps a
direct-install uninstaller from removing the Store's default engine files.
Windows `ApplicationData.LocalCacheFolder` supplies the physical path. A logical
AppData path can be redirected for the packaged parent while remaining invisible
to the downloaded inference process, which has no package identity. Resetting a
custom engine location restores that channel-specific default.
Users can still choose a custom storage directory; such files live outside
package-managed data and may remain after uninstall. Do not select another
installation's active engine directory when testing coexistence. Default data
retention on Store uninstall/reset must be checked with a signed installed
package before publication; loose registration does not establish it.

## Local validation

The MSIX declares `Microsoft.VCLibs.140.00.UWPDesktop` (minimum
`14.0.33728.0`) because the pinned Core requires the desktop C++ runtime.
Microsoft Store resolves this framework dependency. For sideloading or loose
registration, install the Microsoft-signed framework package matching the app's
architecture first, or supply it through `Add-AppxPackage -DependencyPath`.
A developer machine's existing Visual C++ runtime can hide a missing dependency;
verify activation on a clean Windows environment.
The app resolves the framework from its package dependency graph and prepends
that directory to Core's child-process `PATH`. Downloaded inference executables
have no package graph of their own; they inherit this search path without
changing the user's or system's environment.

Use `-Configuration Debug` and a clearly local package identity, for example
`MeowcalSub.StoreLocalTest`, `CN=Meowcal Sub Local Validation`, and package
version `1.0.0.0`. This is test identity/version data, not a Store reservation.
No certificate or trust-store change is required for loose registration when
Windows developer mode is already enabled:

```powershell
Add-AppxPackage -Register '<output>/layout/AppxManifest.xml'
```

Launch through the registered application entry (Start menu or package
activation), not by running the EXE directly. Verify real package identity,
the Store settings text, rejection of updater IPC, isolated data paths,
Core processes, capture/OCR/inference/overlay, and process cleanup. A debug
package must never be submitted. Do not bypass protection or add Defender
exclusions to make validation pass.

## Hosted lifecycle checks

`store-validation.yml` builds native x64 and ARM64 Release packages. Its
Windows 11 ARM job signs two test versions with a short-lived certificate,
installs the initial package, downloads the engine into an empty cache,
translates through the packaged application, upgrades with configuration
retention, translates after restart, and uninstalls the package and private
cache. After upgrading it captures an actual hosted desktop fixture through
Windows OCR and checks that the native overlay displays the resulting translation.
It also seeds a custom engine directory, checks translation after restart,
resets the app through Windows, and verifies that reset and uninstall retain the
external model while removing package-owned settings and cache. This tests a
persisted custom configuration, not a directory-picker UI.
The same runner installs the hash-pinned GitHub v0.8.6 ARM64 NSIS baseline,
opens it with a distinct saved language preference, and checks that Store
setup does not inherit that setting. After Store reset and removal, the direct
installation and configuration must remain unchanged and launch successfully;
the baseline is then uninstalled. This covers separate startup and data
isolation, not simultaneous translation by both channels.
Test signing and certificate trust are restricted to disposable hosted
runners; the workflow does not use production signing secrets.

Artifacts retain per-step results, package hashes, runtime module paths and
Defender status. Hosted desktop checks do not replace real-device capture/OCR,
full WACK, or Store certification; a disabled protection status does not count
as protection-enabled acceptance.

The x64 job also attempts WACK when the runner has `appcert.exe` and an active
user session. Its artifact records tool availability, OS, exit status and the
unmodified report. Missing tooling is reported as not run; an unsuccessful
invocation fails the job. Inspect the report for failed and skipped tests even
when the process exits successfully. Required failures and partial reports fail
the job; optional findings remain visible as warnings and in the result artifact.
Windows Server diagnostics do not close
the Windows 11 client certification gate.

WACK's optional blocked-executables check also concerns Windows S mode. Core
and inference use child processes, and the optional OCR language installer
invokes elevated PowerShell. Do not claim S mode support from a passing overall
WACK result; see Microsoft's [Desktop Bridge test definitions](https://learn.microsoft.com/en-us/windows/uwp/debug-test-perf/windows-desktop-bridge-app-tests).

## Model license release gate

The pinned HY-MT1.5 GGUF model is governed by the
[Tencent HY Community License](https://huggingface.co/tencent/HY-MT1.5-1.8B-GGUF/blob/265b2e615a7dc9b06c435dc878829ad99a512ba2/License.txt),
separately from this application's AGPL license. Its territory excludes the
European Union, United Kingdom and South Korea. Without separate authorization,
worldwide Store availability is not an acceptable release configuration.
Market filtering alone does not resolve every downstream-use restriction.

Before submission, resolve the license's requirements for a license copy,
Notice file, actual-provider identification, Tencent non-affiliation statement,
and downstream use restrictions, including the Acceptable Use Policy. A model
download outside the MSIX does not remove these obligations for a product
using the model. The current Core manifest still marks distribution review as
required and links to `LICENSE`, while the upstream file is `License.txt`.
Correcting that embedded metadata must follow the reviewed Core release and
application pin workflow. Do not treat successful package validation as license
clearance or apply the model's restrictions to the application's AGPL source.

## Before submission

- Reserve the name and supply actual Partner Center identity values.
- Build release MSIX packages for both architectures and run WACK.
- Test signed package installation and upgrades with normal protection,
  including GitHub/Store coexistence, uninstall/reset, and custom storage.
- Check cold-machine WebView2 availability and a complete first-time model and
  runtime download. No model weights are bundled in the MSIX.
- Validate capture, Windows OCR, local inference and overlay on real hardware.
  GPU and long-duration claims require their own evidence.
- Review HY-MT/runtime distribution terms, capture privacy disclosures, and
  the `runFullTrust` capability explanation for certification.

## References

- [Microsoft package requirements and signing](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-package-requirements)
- [Desktop C++ runtime framework packages](https://learn.microsoft.com/en-us/troubleshoot/developer/visualstudio/cpp/libraries/c-runtime-packages-desktop-bridge)
- [Desktop application packaging preparation](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-prepare)
- [Loose-file registration](https://learn.microsoft.com/en-us/windows/apps/develop/testing/loose-file-registration)
- [Store policies](https://learn.microsoft.com/en-us/windows/apps/publish/store-policies)
- [ADR-0006](adr/0006-store-distribution-channel.md)
