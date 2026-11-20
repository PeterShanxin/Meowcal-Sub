import assert from "node:assert/strict";
import { generateKeyPairSync, randomBytes, randomUUID } from "node:crypto";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";

const root = dirname(fileURLToPath(import.meta.url));

test("workerd persists one private archive and publishes only issue metadata", async (t) => {
  const { privateKey } = generateKeyPairSync("rsa", { modulusLength: 2048 });
  const token = randomBytes(32).toString("hex");
  const issues = [];
  const events = [];
  const runtime = new Miniflare(
    convertV4MiniflareOptions({
      modules: ["worker.mjs", "github.mjs", "retention.mjs"].map((name) => ({
        type: "ESModule",
        path: join(root, name),
      })),
      modulesRoot: root,
      compatibilityDate: "2026-05-01",
      durableObjects: { REPORTS: { className: "ReportLedger", useSQLite: true } },
      r2Buckets: ["ARCHIVES"],
      ratelimits: { INTAKE_LIMIT: { namespace_id: "340", simple: { limit: 6, period: 60 } } },
      bindings: {
        GITHUB_APP_ID: "123",
        GITHUB_INSTALLATION_ID: "456",
        GITHUB_APP_PRIVATE_KEY: privateKey.export({ type: "pkcs8", format: "pem" }),
        ARCHIVE_READ_TOKEN: token,
      },
      async outboundService(request) {
        const url = new URL(request.url);
        assert.equal(url.origin, "https://api.github.com");
        if (url.pathname === "/app/installations/456/access_tokens")
          return Response.json({ token: "local-test" });
        if (url.pathname === "/graphql") {
          const { variables } = await request.json();
          const offset = Number(variables.after ?? 0);
          const nodes = events.slice(offset);
          const last = events.at(-1);
          const closed = last?.__typename === "ClosedEvent";
          return Response.json({
            data: {
              repository: {
                issue: {
                  number: variables.issue,
                  state: closed ? "CLOSED" : "OPEN",
                  closedAt: closed ? last.createdAt : null,
                  timelineItems: {
                    nodes,
                    pageInfo: {
                      hasNextPage: false,
                      endCursor: nodes.length ? String(events.length) : null,
                    },
                  },
                },
              },
            },
          });
        }
        assert.equal(url.pathname, "/repos/PeterShanxin/Meowcal-Sub/issues");
        issues.push(await request.json());
        return Response.json({ number: 183 + issues.length }, { status: 201 });
      },
    }),
  );
  t.after(() => runtime.dispose());
  const id = randomUUID();
  // The receiver treats the payload after the protocol header as opaque.
  const bytes = Buffer.concat([Buffer.from("age-encryption.org/v1\n"), randomBytes(256)]);
  const headers = {
    "X-Report-Id": id,
    "X-Report-Type": "translation",
    "X-App-Version": "0.10.0",
    "Content-Type": "application/octet-stream",
    "Content-Length": String(bytes.length),
  };
  const send = (reportId = id) =>
    runtime.dispatchFetch("https://feedback.test/v1/reports", {
      method: "POST",
      headers: { ...headers, "X-Report-Id": reportId },
      body: bytes,
    });
  const receipt = await send();
  assert.equal(receipt.status, 200);
  assert.deepEqual(await receipt.json(), { issueNumber: 184 });
  assert.equal((await send()).status, 200);
  assert.deepEqual(issues, [
    {
      title: "App feedback · Translation quality",
      body: "App version: `0.10.0`\n\nFeedback reference: `" + id + "`",
      labels: ["source:app"],
    },
  ]);
  const url = "https://feedback.test/v1/issues/184/archive";
  assert.equal((await runtime.dispatchFetch(url)).status, 404);
  const archive = await runtime.dispatchFetch(url, {
    headers: { Authorization: "Bearer " + token },
  });
  assert.equal(archive.status, 200);
  assert.deepEqual(Buffer.from(await archive.arrayBuffer()), bytes);
  const second = randomUUID();
  assert.deepEqual(await (await send(second)).json(), { issueNumber: 185 });
  events.push(
    { __typename: "ClosedEvent", createdAt: new Date(Date.now() - 40 * 86400000).toISOString() },
    { __typename: "ReopenedEvent", createdAt: new Date(Date.now() - 5 * 86400000).toISOString() },
  );
  const expired = await runtime.dispatchFetch(url, {
    headers: { Authorization: "Bearer " + token },
  });
  assert.equal(expired.status, 404);
  const bucket = await runtime.getR2Bucket("ARCHIVES");
  assert.equal(await bucket.get("reports/" + id), null);
  assert.equal((await send()).status, 200);
  assert.equal(await bucket.get("reports/" + id), null);
  const entry = await runtime.getWorker();
  await entry.scheduled();
  assert.equal(await bucket.get("reports/" + second), null);
});
