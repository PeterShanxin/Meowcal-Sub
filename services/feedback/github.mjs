const REPOSITORY = "PeterShanxin/Meowcal-Sub";
const API = "https://api.github.com";
const titles = Object.freeze({
  translation: "Translation quality",
  ocr: "Text recognition",
  subtitles: "Missing subtitles",
  error: "Application error",
  suggestion: "Suggestion",
  other: "General feedback",
});

export function validType(value) {
  return Object.hasOwn(titles, value);
}

function publicIssue(report) {
  return {
    title: "App feedback · " + titles[report.type],
    body: "App version: `" + report.version + "`\n\nFeedback reference: `" + report.id + "`",
    labels: ["source:app"],
  };
}

function base64url(bytes) {
  return btoa(String.fromCharCode(...bytes))
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replace(/=+$/, "");
}

function segment(json) {
  return base64url(new TextEncoder().encode(JSON.stringify(json)));
}

async function githubRequest(path, token, options = {}) {
  return fetch(API + path, {
    ...options,
    redirect: "error",
    signal: AbortSignal.timeout(10000),
    headers: {
      "User-Agent": "Meowcal-Sub-feedback",
      Accept: "application/vnd.github+json",
      Authorization: "Bearer " + token,
      "Content-Type": "application/json",
      "X-GitHub-Api-Version": "2022-11-28",
    },
  });
}

export async function installationToken(env) {
  const pem = env.GITHUB_APP_PRIVATE_KEY.replace(/-----[^-]+-----/g, "").replace(/\s/g, "");
  const key = await crypto.subtle.importKey(
    "pkcs8",
    Uint8Array.from(atob(pem), (c) => c.charCodeAt(0)),
    { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const now = Math.floor(Date.now() / 1000);
  const message =
    segment({ alg: "RS256", typ: "JWT" }) +
    "." +
    segment({ iat: now - 60, exp: now + 300, iss: env.GITHUB_APP_ID });
  const sig = await crypto.subtle.sign("RSASSA-PKCS1-v1_5", key, new TextEncoder().encode(message));
  const jwt = message + "." + base64url(new Uint8Array(sig));
  if (!/^[1-9][0-9]*$/.test(env.GITHUB_INSTALLATION_ID)) throw new Error("Invalid installation");
  const response = await githubRequest(
    "/app/installations/" + env.GITHUB_INSTALLATION_ID + "/access_tokens",
    jwt,
    {
      method: "POST",
      body: JSON.stringify({ repositories: ["Meowcal-Sub"], permissions: { issues: "write" } }),
    },
  );
  if (!response.ok) throw new Error("Installation unavailable");
  return (await response.json()).token;
}

export function createIssue(report, token) {
  return githubRequest("/repos/" + REPOSITORY + "/issues", token, {
    method: "POST",
    body: JSON.stringify(publicIssue(report)),
  });
}

// A lost create response leaves a durable reservation. A matching issue from this
// installation is proof of delivery; an empty search is never proof of failure.
export async function findIssue(report, token, appId) {
  const q = "repo:" + REPOSITORY + ' is:issue in:body "' + report.id + '"';
  const search = await githubRequest("/search/issues?q=" + encodeURIComponent(q), token);
  if (!search.ok) return null;
  const match = publicIssue(report);
  const { items } = await search.json();
  for (const item of items.slice(0, 10)) {
    if (item.body !== match.body || !Number.isSafeInteger(item.number)) continue;
    const full = await githubRequest("/repos/" + REPOSITORY + "/issues/" + item.number, token);
    if (!full.ok) continue;
    const issue = await full.json();
    if (String(issue.performed_via_github_app?.id) === String(appId) && issue.body === match.body)
      return issue.number;
  }
  return null;
}
