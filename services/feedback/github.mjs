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
    redirect: "manual",
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
  if (!search.ok) throw new Error("Issue search unavailable");
  const reference = "Feedback reference: `" + report.id + "`";
  const { items, incomplete_results: incomplete, total_count: total } = await search.json();
  if (!Array.isArray(items) || incomplete !== false || total !== items.length)
    throw new Error("Incomplete issue search");
  const candidates = items.filter(
    (item) => item.body?.includes(reference) && Number.isSafeInteger(item.number),
  );
  // Ambiguous matches require a later retry or operator investigation. Bound
  // per-upload lookup work even when someone copies a reference into issues.
  if (candidates.length > 2) throw new Error("Ambiguous issue search");
  for (const item of candidates) {
    const full = await githubRequest("/repos/" + REPOSITORY + "/issues/" + item.number, token);
    if (!full.ok) throw new Error("Issue metadata unavailable");
    const issue = await full.json();
    if (
      String(issue.performed_via_github_app?.id) === String(appId) &&
      issue.body?.includes(reference)
    )
      return issue.number;
  }
  return null;
}

// Filter at GitHub so comments, labels and attachments never enter the Worker.
const HISTORY_QUERY = `query FeedbackState($issue:Int!, $after:String) {
  repository(owner:"PeterShanxin", name:"Meowcal-Sub") {
    issue(number:$issue) {
      number state closedAt
      timelineItems(first:50, after:$after, itemTypes:[CLOSED_EVENT,REOPENED_EVENT]) {
        nodes { __typename ... on ClosedEvent { createdAt } ... on ReopenedEvent { createdAt } }
        pageInfo { hasNextPage endCursor }
      }
    }
  }
}`;

function timestamp(value) {
  const time = typeof value === "string" ? Date.parse(value) : NaN;
  if (!Number.isFinite(time)) throw new Error("Invalid issue timestamp");
  return time;
}

export async function issueHistory(number, cursor, token) {
  const response = await githubRequest("/graphql", token, {
    method: "POST",
    body: JSON.stringify({ query: HISTORY_QUERY, variables: { issue: number, after: cursor } }),
  });
  if (!response.ok) throw new Error("Issue state unavailable");
  const result = await response.json();
  const issue = result.data?.repository?.issue;
  const items = issue?.timelineItems;
  const info = items?.pageInfo;
  if (
    result.errors?.length ||
    issue?.number !== number ||
    !["OPEN", "CLOSED"].includes(issue.state) ||
    (issue.state === "OPEN" && issue.closedAt !== null) ||
    !Array.isArray(items?.nodes) ||
    typeof info?.hasNextPage !== "boolean" ||
    (items.nodes.length &&
      (typeof info.endCursor !== "string" || !info.endCursor || info.endCursor === cursor)) ||
    (!items.nodes.length && info.hasNextPage)
  )
    throw new Error("Incomplete issue timeline");
  const events = items.nodes.map((node) => {
    if (!["ClosedEvent", "ReopenedEvent"].includes(node?.__typename))
      throw new Error("Unknown issue transition");
    return { type: node.__typename, time: timestamp(node.createdAt) };
  });
  return {
    state: issue.state,
    closedAt: issue.state === "CLOSED" ? timestamp(issue.closedAt) : null,
    events,
    cursor: info.endCursor,
    more: info.hasNextPage,
  };
}
