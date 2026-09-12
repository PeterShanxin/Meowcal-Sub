# Meowcal Core upgrade automation

`.github/workflows/core-upgrade.yml` checks the canonical
`PeterShanxin/Meowcal-Sub` releases once a day and on manual dispatch. It only
accepts a published, non-draft, non-prerelease `core-vX.Y.Z` release. The
updater downloads both Windows archives and checksum files, verifies the exact
SHA-256 values, archive contents, package metadata, PE architecture, and the
x64 API/capability contract, then writes `config/meowcal-core.lock.json`.

The workflow does nothing when no stable Core release exists or when the lock
already covers the newest release. It never merges a pull request. A generated
pull request is the review and CI boundary for the consumer integration tests,
including the native ARM64 gate.

To activate PR creation, configure a repository secret named
`CORE_UPGRADE_TOKEN`. It must be a maintainer-owned fine-grained token or GitHub
App token with Contents read/write and Pull requests read/write on this
repository. The token must be distinct from `GITHUB_TOKEN`: GitHub suppresses
new workflow runs for most events emitted with `GITHUB_TOKEN`, which would
leave the generated pull request without its required `pull_request` checks.

The secret and repository settings are intentionally not changed by this
workflow implementation. Until the secret is configured, a verified update is
reported in the workflow log and no branch or pull request is written.

Run the deterministic updater checks locally with:

```powershell
pwsh -NoProfile -File scripts/tests/core-upgrade.Tests.ps1
```
