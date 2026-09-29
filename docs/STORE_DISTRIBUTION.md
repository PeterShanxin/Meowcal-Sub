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

The Store's default Core base is `<Store app cache>/Core`, retaining Core's
`sub1/<profile>/<version>/<architecture>` partition structure. This keeps a
direct-install uninstaller from removing the Store's default engine files.
Resetting a custom engine location restores that channel-specific default.
Users can still choose a custom storage directory; such files live outside
package-managed data and may remain after uninstall. Do not select another
installation's active engine directory when testing coexistence. Default data
retention on Store uninstall/reset must be checked with a signed installed
package before publication; loose registration does not establish it.

## Local validation

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
- [Desktop application packaging preparation](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-prepare)
- [Loose-file registration](https://learn.microsoft.com/en-us/windows/apps/develop/testing/loose-file-registration)
- [Store policies](https://learn.microsoft.com/en-us/windows/apps/publish/store-policies)
- [ADR-0006](adr/0006-store-distribution-channel.md)
