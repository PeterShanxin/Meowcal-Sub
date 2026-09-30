export const PRODUCT_ID = "9NNK2X23VLWT";
export const STORE_IDENTITY = Object.freeze({
  name: "ShanxinLi.MeowcalSub",
  publisher: "CN=FCD37627-8F13-4157-933C-E729F9F08408",
  publisherDisplayName: "Shanxin Li",
});
const API = "https://manage.devcenter.microsoft.com/v1.0/my";

export class SubmissionError extends Error {
  constructor(code) {
    super(code);
    this.code = code;
  }
}

export function requireCondition(condition, code) {
  if (!condition) throw new SubmissionError(code);
}

export function compareStoreVersions(left, right) {
  const parse = (value) => {
    requireCondition(/^[1-9]\d*\.\d+\.\d+\.0$/.test(value), "invalid_store_version");
    const parts = value.split(".").map(Number);
    requireCondition(
      parts.every((part) => part <= 65535),
      "invalid_store_version",
    );
    return parts;
  };
  const a = parse(left);
  const b = parse(right);
  for (let i = 0; i < 4; i++) if (a[i] !== b[i]) return Math.sign(a[i] - b[i]);
  return 0;
}

export function assertAppIdentity(app) {
  requireCondition(
    app.id === PRODUCT_ID &&
      app.packageIdentityName === STORE_IDENTITY.name &&
      app.publisherName === STORE_IDENTITY.publisher,
    "store_app_identity_mismatch",
  );
}

export function assertApp(app) {
  assertAppIdentity(app);
  requireCondition(!app.pendingApplicationSubmission, "existing_submission_protected");
  requireCondition(
    /^\d+$/.test(app.lastPublishedApplicationSubmission?.id),
    "published_submission_missing",
  );
}

export function updateSubmission(submission, release, publishMode = "Manual") {
  requireCondition(["Manual", "Immediate"].includes(publishMode), "publish_mode_invalid");
  requireCondition(submission.status === "PendingCommit", "submission_not_draft");
  requireCondition(
    Array.isArray(submission.applicationPackages) && submission.applicationPackages.length > 0,
    "published_packages_missing",
  );
  requireCondition(submission.pricing?.priceId === "Free", "free_product_required");
  const result = structuredClone(submission);
  result.targetPublishMode = publishMode;
  for (const pkg of result.applicationPackages) {
    requireCondition(pkg.fileStatus === "Uploaded", "unexpected_package_state");
    requireCondition(
      compareStoreVersions(release.storeVersion, pkg.version) > 0,
      "store_version_not_increasing",
    );
    pkg.fileStatus = "PendingDelete";
  }
  for (const arch of ["x64", "arm64"]) {
    result.applicationPackages.push({
      fileName: `MeowcalSub-${release.storeVersion}-${arch}-Release.msix`,
      fileStatus: "PendingUpload",
      minimumDirectXVersion: "None",
      minimumSystemRam: "None",
    });
  }
  const listings = Object.values(result.listings ?? {});
  requireCondition(listings.length > 0, "published_listings_missing");
  requireCondition(
    typeof release.notes === "string" &&
      release.notes.trim().length > 0 &&
      release.notes.length <= 1500,
    "store_release_notes_invalid",
  );
  for (const listing of listings) {
    const base = listing.baseListing;
    requireCondition(base && typeof base === "object", "invalid_listing");
    base.releaseNotes = release.notes;
    const images = (base.images ??= []);
    requireCondition(Array.isArray(images), "invalid_listing_images");
    for (const image of images) {
      if (image.imageType === "Icon") image.fileStatus = "PendingDelete";
    }
    images.push({
      fileName: "store-listing-logo-300.png",
      fileStatus: "PendingUpload",
      imageType: "Icon",
    });
  }
  return result;
}

export function safeFailure(error) {
  return error instanceof SubmissionError ? error.code : "unexpected_failure_redacted";
}

export function createStoreClient(credentials, fetchImpl = fetch) {
  for (const name of ["AZURE_AD_TENANT_ID", "AZURE_AD_APPLICATION_CLIENT_ID"]) {
    requireCondition(/^[a-f\d-]{36}$/i.test(credentials[name] ?? ""), "credentials_not_configured");
  }
  requireCondition(
    credentials.AZURE_AD_APPLICATION_SECRET?.length > 0 && credentials.SELLER_ID?.length > 0,
    "credentials_not_configured",
  );
  let token;
  async function request(url, options, code) {
    try {
      const response = await fetchImpl(url, {
        ...options,
        redirect: "error",
        signal: AbortSignal.timeout(120000),
      });
      requireCondition(response.ok, code);
      return response;
    } catch {
      throw new SubmissionError(code);
    }
  }
  async function api(method, suffix = "", body) {
    const allowed =
      method === "GET" ||
      (method === "POST" && suffix === "/submissions") ||
      (method === "POST" && /^\/submissions\/\d+\/commit$/.test(suffix)) ||
      (method === "PUT" && /^\/submissions\/\d+$/.test(suffix));
    requireCondition(allowed, "operation_forbidden");
    const response = await request(
      `${API}/applications/${PRODUCT_ID}${suffix}`,
      {
        method,
        headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
        ...(body ? { body: JSON.stringify(body) } : {}),
      },
      "store_request_failed",
    );
    try {
      return await response.json();
    } catch {
      throw new SubmissionError("store_response_invalid");
    }
  }
  return {
    async authenticate() {
      const response = await request(
        `https://login.microsoftonline.com/${credentials.AZURE_AD_TENANT_ID}/oauth2/token`,
        {
          method: "POST",
          body: new URLSearchParams({
            grant_type: "client_credentials",
            client_id: credentials.AZURE_AD_APPLICATION_CLIENT_ID,
            client_secret: credentials.AZURE_AD_APPLICATION_SECRET,
            resource: "https://manage.devcenter.microsoft.com",
          }),
        },
        "authentication_failed",
      );
      try {
        token = (await response.json()).access_token;
      } catch {
        throw new SubmissionError("authentication_failed");
      }
      requireCondition(typeof token === "string" && token.length > 0, "authentication_failed");
    },
    getApp: () => api("GET"),
    getSubmission: (id) => {
      requireCondition(/^\d+$/.test(id), "invalid_submission_id");
      return api("GET", `/submissions/${id}`);
    },
    getStatus: (id) => {
      requireCondition(/^\d+$/.test(id), "invalid_submission_id");
      return api("GET", `/submissions/${id}/status`);
    },
    create: () => api("POST", "/submissions"),
    update: (id, body) => {
      requireCondition(/^\d+$/.test(id), "invalid_submission_id");
      return api("PUT", `/submissions/${id}`, body);
    },
    commit: (id) => {
      requireCondition(/^\d+$/.test(id), "invalid_submission_id");
      return api("POST", `/submissions/${id}/commit`);
    },
    async upload(url, bytes, md5) {
      let parsed;
      try {
        parsed = new URL(url);
      } catch {
        throw new SubmissionError("upload_url_invalid");
      }
      requireCondition(
        parsed.protocol === "https:" &&
          /^[a-z0-9]+\.blob\.core\.windows\.net$/.test(parsed.hostname) &&
          !parsed.username &&
          !parsed.password &&
          parsed.searchParams.has("sig"),
        "upload_url_invalid",
      );
      await request(
        url,
        {
          method: "PUT",
          headers: {
            "x-ms-blob-type": "BlockBlob",
            "Content-Type": "application/zip",
            "Content-MD5": md5,
            "x-ms-blob-content-md5": md5,
          },
          body: bytes,
        },
        "upload_failed_draft_retained",
      );
      const response = await request(
        url,
        { method: "HEAD" },
        "upload_verification_failed_draft_retained",
      );
      requireCondition(
        Number(response.headers.get("content-length")) === bytes.length &&
          response.headers.get("content-md5") === md5,
        "partial_upload_draft_retained",
      );
    },
  };
}

async function stageSubmission(client, release, bundle, publishMode, onCreated) {
  const app = await client.getApp();
  assertApp(app);
  const published = await client.getSubmission(app.lastPublishedApplicationSubmission.id);
  requireCondition(published.status === "Published", "published_submission_not_ready");
  // Validate all metadata/version changes before creating any draft.
  updateSubmission({ ...published, status: "PendingCommit" }, release, publishMode);
  const current = await client.getApp();
  assertApp(current);
  requireCondition(
    current.lastPublishedApplicationSubmission.id === app.lastPublishedApplicationSubmission.id,
    "published_submission_changed",
  );
  const created = await client.create();
  requireCondition(/^\d+$/.test(created.id), "invalid_created_submission");
  onCreated(created.id);
  const payload = updateSubmission(created, release, publishMode);
  const ownership = await client.getApp();
  requireCondition(
    ownership.pendingApplicationSubmission?.id === created.id,
    "submission_ownership_changed",
  );
  await client.update(created.id, payload);
  await client.upload(created.fileUploadUrl, bundle.bytes, bundle.md5);
  const after = await client.getSubmission(created.id);
  requireCondition(
    after.status === "PendingCommit" && after.targetPublishMode === publishMode,
    "staged_state_unconfirmed",
  );
  for (const pkg of payload.applicationPackages.filter(
    (pkg) => pkg.fileStatus === "PendingUpload",
  )) {
    requireCondition(
      after.applicationPackages?.some(
        (candidate) =>
          candidate.fileName === pkg.fileName &&
          ["PendingUpload", "Uploaded"].includes(candidate.fileStatus),
      ),
      "staged_packages_unconfirmed",
    );
  }
  for (const [locale, listing] of Object.entries(payload.listings)) {
    const actual = after.listings?.[locale]?.baseListing;
    requireCondition(
      actual?.releaseNotes === listing.baseListing.releaseNotes &&
        actual.images?.some(
          (image) =>
            image.imageType === "Icon" &&
            image.fileName === "store-listing-logo-300.png" &&
            image.fileStatus !== "PendingDelete",
        ),
      "staged_listing_unconfirmed",
    );
  }
  return {
    productId: PRODUCT_ID,
    submissionId: created.id,
    status: "PendingCommit",
    committed: false,
  };
}

export async function stageDraft(
  client,
  release,
  bundle,
  { stage = false, commit = false, onCreated = () => {} } = {},
) {
  requireCondition(!commit, "commit_forbidden");
  requireCondition(stage === true, "explicit_stage_required");
  await client.authenticate();
  return stageSubmission(client, release, bundle, "Manual", onCreated);
}

const SUBMITTED = new Set([
  "CommitStarted",
  "PreProcessing",
  "Certification",
  "Release",
  "PendingPublication",
  "Publishing",
  "Published",
]);
const FAILED = new Set([
  "CommitFailed",
  "PreProcessingFailed",
  "CertificationFailed",
  "ReleaseFailed",
  "PublishFailed",
  "Canceled",
]);

export function submissionStatus(status) {
  requireCondition(typeof status === "string", "store_status_invalid");
  requireCondition(!FAILED.has(status), `store_${status.toLowerCase()}`);
  requireCondition(SUBMITTED.has(status), "store_status_unexpected");
  return status;
}

export async function submitRelease(client, release, bundle, onCreated = () => {}) {
  await client.authenticate();
  const app = await client.getApp();
  assertAppIdentity(app);
  if (app.pendingApplicationSubmission) {
    const pending = await client.getSubmission(app.pendingApplicationSubmission.id);
    const files = ["x64", "arm64"].map(
      (arch) => `MeowcalSub-${release.storeVersion}-${arch}-Release.msix`,
    );
    requireCondition(
      pending.targetPublishMode === "Immediate" &&
        Object.values(pending.listings ?? {}).length > 0 &&
        Object.values(pending.listings).every(
          (listing) => listing.baseListing?.releaseNotes === release.notes,
        ) &&
        files.every((name) => pending.applicationPackages?.some((pkg) => pkg.fileName === name)),
      "existing_submission_protected",
    );
    const current = await client.getStatus(pending.id);
    return { submissionId: pending.id, status: submissionStatus(current.status), reused: true };
  }
  const published = await client.getSubmission(app.lastPublishedApplicationSubmission?.id);
  requireCondition(published.status === "Published", "published_submission_not_ready");
  const versions = published.applicationPackages?.map((pkg) => pkg.version) ?? [];
  requireCondition(versions.length > 0, "published_packages_missing");
  if (versions.every((version) => compareStoreVersions(release.storeVersion, version) === 0)) {
    requireCondition(
      ["x64", "arm64"].every((arch) =>
        published.applicationPackages.some(
          (pkg) => pkg.fileName === `MeowcalSub-${release.storeVersion}-${arch}-Release.msix`,
        ),
      ) &&
        Object.values(published.listings ?? {}).length > 0 &&
        Object.values(published.listings).every(
          (listing) => listing.baseListing?.releaseNotes === release.notes,
        ),
      "store_version_not_increasing",
    );
    return { submissionId: published.id, status: "Published", reused: true };
  }
  const staged = await stageSubmission(client, release, bundle, "Immediate", onCreated);
  const current = await client.getApp();
  requireCondition(
    current.pendingApplicationSubmission?.id === staged.submissionId,
    "submission_ownership_changed",
  );
  let committed;
  try {
    committed = await client.commit(staged.submissionId);
  } catch {
    // A transport failure can follow a successful POST. Read status; never send a second commit.
    committed = await client.getStatus(staged.submissionId);
  }
  return {
    submissionId: staged.submissionId,
    status: submissionStatus(committed.status),
    reused: false,
  };
}

export async function reportSubmissionStatus(client) {
  await client.authenticate();
  const app = await client.getApp();
  assertAppIdentity(app);
  const id = app.pendingApplicationSubmission?.id ?? app.lastPublishedApplicationSubmission?.id;
  requireCondition(/^\d+$/.test(id), "submission_missing");
  const status = await client.getStatus(id);
  return { submissionId: id, status: submissionStatus(status.status) };
}
