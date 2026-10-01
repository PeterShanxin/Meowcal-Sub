# Store release submission setup

The [release sync workflow](../.github/workflows/store-release-sync.yml) is
enabled by `STORE_RELEASE_AUTOMATION_ENABLED=true` in the
`store-submission-draft` GitHub environment. It is currently enabled. Publishing a
stable GitHub release by a trusted maintainer verifies its complete assets,
stages both MSIX packages and the canonical 300 × 300 logo, commits the Store
submission for certification, and requests `Immediate` publication after Store
approval. A scheduled read-only Store status check reports certification and
publication progress each hour. Microsoft still decides whether certification
succeeds and when the package becomes available.

Do not enable the workflow while an existing Store submission is pending. It
never deletes or changes an existing draft. A matching submission already in
certification is reported without another commit; a pending uncommitted draft
stops the run for deliberate recovery. A failed upload leaves the created draft
in place. GitHub release assets are retried for up to 15 minutes while upload
metadata is incomplete, then the run fails without contacting Partner Center.

The GitHub release must already be published for the event to fire. This
repository creates a draft release first and a maintainer publishes it after
checking assets. A release published by another workflow with the repository
`GITHUB_TOKEN` does not trigger `release.published`; use an independently
authorized GitHub App token if release publication itself later becomes fully
automated. The sync workflow does not create tags or releases.

The [Prepare Store Submission Draft](../.github/workflows/store-submission-draft.yml)
workflow is limited to Meowcal Sub product `9NNK2X23VLWT`. Its default run checks
a published GitHub release without contacting Partner Center. An explicit
`stage_draft` run can create **one uncommitted, API-managed** submission that
contains both production MSIX packages, the 300 × 300 listing icon, and release
notes. The workflow contains no commit, certification or publication operation.

## Before enabling automatic submission

1. In Partner Center, confirm that the associated MeowcalStudio Entra application
   has the **Manager** role and the existing account can use the submission API.
   A protected scheduled job has authenticated and read the published submission;
   creation and certification submission have not yet been tested. Confirm the
   product has a completed first submission with age ratings, is a free product,
   and uses neither mandatory app updates nor Store-managed consumables. The
   Store CLI GitHub recipe supports free products; this workflow uses the
   [MSIX submission API](https://learn.microsoft.com/en-us/windows/uwp/monetize/create-and-manage-submissions-using-windows-store-services).
2. The `store-submission-draft` GitHub environment currently has these four
   secret names:
   `AZURE_AD_TENANT_ID`, `AZURE_AD_APPLICATION_CLIENT_ID`,
   `AZURE_AD_APPLICATION_SECRET`, and `SELLER_ID`. Their values have not been
   read or verified. The environment currently has no required reviewers and
   no branch restriction; activation therefore grants unattended release-time
   access to these credentials. Review that access policy before enabling it.
   `SELLER_ID` is recorded as part of the account setup; this
   particular REST endpoint authenticates via the tenant/client application
   and fixes the target product ID in source. Enter secrets only in GitHub's
   protected environment, never in workflow inputs, a PR, a local config or
   logs. Store the secret expiration date in an approved credential inventory;
   arrange rotation before expiration and revoke the old key afterward.
3. Check Partner Center for a pending submission, including a manually edited
   draft. If one exists, finish or cancel it through its existing owner before
   publishing another release. The workflow refuses to mutate an existing draft.
4. Confirm `STORE_RELEASE_AUTOMATION_ENABLED=true` before relying on unattended
   submission. Set it to `false` to pause future release syncs. The existing
   manual draft workflow has a separate
   `STORE_DRAFT_STAGING_ENABLED` switch; neither switch enables the other.

For a read-only Store inventory, open **Actions → Sync GitHub Release to Store →
Run workflow** on `main`. This manually dispatched path authenticates through
the configured environment and reads the product identity, last published
package versions, and any pending submission ID/status. It does not create or
commit a submission; the release submission job runs only for a stable published
GitHub release. Treat a pending submission as a blocker and resolve it through
its existing owner before publishing another release.

## Manual verification and draft workflow

Open **Actions → Prepare Store Submission Draft → Run workflow** on `main`.
Provide the tag of an already published canonical release and its **exact**
40-character tag commit. For v0.8.7, the pair is `v0.8.7` and
`c19afcd4eb1d8f7f579b608601e1f219dc3af4fa`. Leave `stage_draft` off
for a read-only verification run. A staging run requires setting it to true,
the enabled environment and its reviewer approval and four configured secrets.
Neither mode accepts package URLs or package identity from inputs.

Verification binds the tag, commit, eight published release assets and their
SHA256 digests, combined checksums, application version and release notes. The
Store listing receives complete fix/improvement bullets that fit its 1,500
character release-notes field, followed by a link to the full GitHub notes.
The exact tagged release document is the source; a malformed or overlong
summary fails before draft creation. Verification also
examines both MSIX manifests and PE architectures, checks the independent Store
version and compares all three package icons with the tag's source. It copies
the exact 300 × 300 listing icon from that commit. After checking the target
product's identity, last published submission and absence of a pending draft,
staging copies published metadata, marks old packages and `Icon` listing images
for replacement, and uploads one ZIP with both MSIX packages and the new icon.
Other screenshots, descriptions, pricing and certification notes are retained;
`targetPublishMode` is set to `Manual`.

The run prints the new submission ID, never the bearer token or upload SAS URL.
If staging fails after creation, **the new uncommitted draft remains**. Check
its ID and status with the same API and resolve that draft deliberately; reruns
will refuse it. Review the staged package versions, architectures, listing icon,
release notes and retained metadata before any later certification action.
Microsoft warns that editing an API-created submission in the Partner Center
UI can leave it impossible to change or commit through the API. Keep edits and
future commit through the API, or stop and plan an explicit recovery. The
automatic release workflow will not adopt or commit this manual draft.

Microsoft's [`msstore publish --noCommit`](https://learn.microsoft.com/en-us/windows/apps/publish/msstore-dev-cli/commands)
can stage a CLI-managed draft, but `msstore publish` recreates a draft from the
last published submission. Do not run it against this API-managed draft.
This workflow calls the
[`manage.devcenter.microsoft.com` MSIX app submission API](https://learn.microsoft.com/en-us/windows/uwp/monetize/manage-app-submissions),
not the separate EXE/MSI Store API. Its single ZIP uses the upload URL returned
by the newly created submission. The Store may not validate package content
until commit; an uncommitted draft is not a certified update. `Immediate`
requests publication after successful certification; `CommitStarted` is not
proof of certification or availability.
