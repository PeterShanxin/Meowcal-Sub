# Store submission draft setup

The [Prepare Store Submission Draft](../.github/workflows/store-submission-draft.yml)
workflow is limited to Meowcal Sub product `9NNK2X23VLWT`. Its default run checks
a published GitHub release without contacting Partner Center. An explicit
`stage_draft` run can create **one uncommitted, API-managed** submission that
contains both production MSIX packages, the 300 × 300 listing icon, and release
notes. The workflow contains no commit, certification or publication operation.

## Before enabling staging

1. In Partner Center, check this account's **Account settings → User
   management / Microsoft Entra applications** and whether an Entra directory
   and application can be associated with the account. This has not been
   verified for the existing individual account. It is an eligibility check,
   not a request to change the account type or register a company. Confirm the
   product has a completed first submission with age ratings, is a free product,
   and uses neither mandatory app updates nor Store-managed consumables. The
   Store CLI GitHub recipe supports free products; this workflow uses the
   [MSIX submission API](https://learn.microsoft.com/en-us/windows/uwp/monetize/create-and-manage-submissions-using-windows-store-services).
2. If eligible and separately approved, associate a dedicated Microsoft Entra
   application with Partner Center. Microsoft's API guide requires the
   **Manager** role for that application. The tenant administrator may need to
   create or link the directory/application first. Record the tenant ID,
   application client ID and Seller ID from the account. Do not create a tenant,
   grant Manager, or generate a client secret as part of this code PR.
3. If separately approved, create the GitHub environment
   `store-submission-draft` with required reviewers, then enter its secrets
   `AZURE_AD_TENANT_ID`, `AZURE_AD_APPLICATION_CLIENT_ID`,
   `AZURE_AD_APPLICATION_SECRET`, and `SELLER_ID`. Set the environment variable
   `STORE_DRAFT_STAGING_ENABLED` to `true` only after access is tested and
   approved. `SELLER_ID` is recorded as part of the account setup; this
   particular REST endpoint authenticates via the tenant/client application
   and fixes the target product ID in source. Enter secrets only in GitHub's
   protected environment, never in workflow inputs, a PR, a local config or
   logs. Store the secret expiration date in an approved credential inventory;
   arrange rotation before expiration and revoke the old key afterward.
4. Check Partner Center for a pending submission, including a manually edited
   draft. If one exists, finish or cancel it through its existing owner before
   staging. The workflow also checks `pendingApplicationSubmission` twice and
   refuses to mutate it. It never deletes/recreates someone else's draft.

## Running the workflow

Open **Actions → Prepare Store Submission Draft → Run workflow** on `main`.
Provide the tag of an already published canonical release and its **exact**
40-character tag commit. For v0.8.7, the pair is `v0.8.7` and
`c19afcd4eb1d8f7f579b608601e1f219dc3af4fa`. Leave `stage_draft` off
for a read-only verification run. A staging run requires setting it to true,
the enabled environment and its reviewer approval and four configured secrets.
Neither mode accepts package URLs or package identity from inputs.

Verification binds the tag, commit, eight published release assets and their
SHA256 digests, combined checksums, application version and release notes. It
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
future commit through the API, or stop and plan an explicit recovery. No later
commit or publication workflow is included here.

Microsoft's [`msstore publish --noCommit`](https://learn.microsoft.com/en-us/windows/apps/publish/msstore-dev-cli/commands)
can stage a CLI-managed draft, but `msstore publish` recreates a draft from the
last published submission. Do not run it against this API-managed draft.
This workflow calls the
[`manage.devcenter.microsoft.com` MSIX app submission API](https://learn.microsoft.com/en-us/windows/uwp/monetize/manage-app-submissions),
not the separate EXE/MSI Store API. Its single ZIP uses the upload URL returned
by the newly created submission. The Store may not validate package content
until a later explicit commit; an uncommitted draft is not a certified update.
