# Feedback intake and maintainer access

The desktop app encrypts a ZIP for the team's age recipient before submission. The Worker stores the ciphertext in a private R2 bucket. A singleton SQLite-backed Durable Object associates each submission with one public issue in `PeterShanxin/Meowcal-Sub`. Its GitHub App creates the issue with a fixed title, app version, random feedback reference and the `source:app` label.

## Prepare the service

1. Create a GitHub App with **Issues: Read and write**. Install it on `PeterShanxin/Meowcal-Sub` only. Create the `source:app` label in that repository. The Worker requests installation tokens restricted to that repository and permission.
2. From `services/feedback`, create the private R2 bucket `meowcal-feedback-archives`. Disable public access and apply an object lifecycle rule to expire the `reports/` prefix after 30 days. Leave Worker observability disabled. The receipt ledger retains the issue number, category, version, random submission ID and timestamps for idempotency.
3. Configure Worker secrets `GITHUB_APP_ID`, `GITHUB_INSTALLATION_ID`, `GITHUB_APP_PRIVATE_KEY` and `ARCHIVE_READ_TOKEN`. The app key must be PKCS#8 PEM. Convert GitHub's generated RSA key with `openssl pkcs8 -topk8 -nocrypt` using private input and output files. Generate at least 32 random bytes for the archive-read credential and encode them as hex. Provision secrets through a trusted console or standard input.
4. Deploy the Worker with its checked-in Wrangler configuration and an HTTPS origin. Rate-limit namespace 340 must be available in the chosen Cloudflare account. Its edge limit is six uploads per network address per minute. The ledger admits up to 120 new report IDs per UTC day across the service. Each encrypted archive can contain up to 16 MiB.
5. Create an X25519 team identity with `age-keygen` in a protected location. Keep a recoverable backup in the team's secret store. Configure repository variables `MEOWCAL_FEEDBACK_URL` (the HTTPS origin only) and `MEOWCAL_FEEDBACK_RECIPIENT` (the public `age1…` recipient) **before** building the application.
6. Create the GitHub Environment `feedback-access`. Restrict deployments to branch `main` and protect changes to the retrieval workflow on that branch. Store the identity as the Environment Secret `FEEDBACK_AGE_IDENTITY`. Set `FEEDBACK_ARCHIVE_READ_TOKEN` there to the Worker's archive-read credential. Only this environment should hold these two secrets. Environment approvals can be enabled if the team wants a second maintainer to authorize each retrieval.

When rotating the team key, include both old and new identities (one per line) in the Environment Secret. Older app versions keep encrypting with their shipped recipient. Retiring a key makes reports sent to that recipient unreadable unless a recoverable copy remains.

## Open a report

On Windows 11, sign in to GitHub CLI with an account that has write access to the repository. Run this command from a trusted checkout in PowerShell 7:

```powershell
.\scripts\open-feedback.ps1 184
```

The number is the GitHub issue number. The script downloads a checksum-pinned age CLI, creates an in-memory temporary identity, and submits its public recipient to the workflow on `main`. The workflow checks the caller's current repository access, fetches ciphertext by issue number, and streams the decrypted ZIP directly into encryption for that recipient. Only the re-encrypted archive becomes a workflow artifact. The temporary identity expires with the script process, so if retrieval is interrupted, run the command again.

By default the script writes the ZIP to `%LOCALAPPDATA%\meowcal-sub\feedback\feedback-184.zip`. Pass `-OutputDirectory` to use another private folder. Existing output is preserved. Treat the returned ZIP as user-provided data and keep its contents out of public issues, logs and pull requests.

## Delivery and recovery

`POST /v1/reports` accepts binary age v1 data with a known content length, `Content-Type: application/octet-stream`, a random v4 UUID in `X-Report-Id`, a three-part release version in `X-App-Version`, and a category in `X-Report-Type`. A confirmed delivery returns `{"issueNumber":184}`. Editing a draft creates a new ID. Retrying an unchanged draft keeps its ID and first accepted ciphertext.

The ledger reserves issuance before calling GitHub. If GitHub confirms rejection, the next user retry may create the issue. If GitHub might have accepted the request, the service keeps the reservation and looks up the fixed feedback reference on retry after 30 seconds. Only an issue created by this GitHub App can complete recovery. Search indexing delay can keep a report pending. If delivery remains uncertain, inspect the app's issue history and the ledger before clearing a reservation. A second create request could duplicate the report.

The private `GET /v1/issues/{number}/archive` route requires the archive-read bearer credential. Knowledge of the issue number or feedback reference alone does not authorize a download. If the bucket lifecycle has removed the archive, the public issue remains accessible.

Run `node --test services/feedback/worker.test.mjs` from the repository root for the intake, privacy, authorization, concurrency and retry tests. With Wrangler 4.165.0, `wrangler deploy --dry-run` from this directory verifies the bundle and binding declarations locally.

On Linux, run `npm ci --ignore-scripts` followed by `npm test` in this directory to also exercise the Worker in workerd. The test uses real local R2 and Durable Object bindings and substitutes a local response for GitHub. It checks the public issue fields, repeat delivery, and authenticated retrieval of the exact ciphertext.
