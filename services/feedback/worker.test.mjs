import assert from "node:assert/strict";
import { timingSafeEqual } from "node:crypto";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import worker, { ReportLedger } from "./worker.mjs";
import { installationToken } from "./github.mjs";

const privateSample = "synthetic-subtitle-only-for-tests";
const requestId = "93fbdec5-1c22-4efd-8926-c547809064b3";
function submit({ id = requestId, type = "translation", version = "0.10.0", body, headers } = {}) {
  const bytes = new TextEncoder().encode(
    body ?? "age-encryption.org/v1\n" + privateSample.repeat(5),
  );
  return new Request("https://feedback.test/v1/reports", {
    method: "POST",
    body: bytes,
    headers: {
      "X-Report-Id": id,
      "X-Report-Type": type,
      "X-App-Version": version,
      "Content-Type": "application/octet-stream",
      "Content-Length": String(bytes.length),
      ...headers,
    },
  });
}

function fixture(t) {
  const db = new DatabaseSync(":memory:");
  t.after(() => db.close());
  const sql = {
    exec: (query, ...args) => {
      const result = db.prepare(query).all(...args);
      return { toArray: () => result };
    },
  };
  const stored = new Map();
  const issues = [];
  let maybeCreated = false;
  const env = {
    INTAKE_LIMIT: { limit: async () => ({ success: true }) },
    ARCHIVES: {
      async put(key, value, { onlyIf }) {
        assert.equal(onlyIf.get("If-None-Match"), "*");
        if (!stored.has(key)) stored.set(key, value);
      },
      get: async (key) => stored.has(key) && { body: stored.get(key) },
    },
    GITHUB_APP_ID: "123",
    ARCHIVE_READ_TOKEN: "x".repeat(48),
  };
  const ledger = new ReportLedger({ storage: { sql } }, env);
  ledger.token = async () => "test-token";
  t.mock.method(globalThis, "fetch", async (url, opts) => {
    if (url.includes("/search/issues?")) {
      return Response.json({ items: issues.map((item, i) => ({ ...item, number: i + 1 })) });
    }
    if (opts.method === "POST") {
      assert.equal(url, "https://api.github.com/repos/PeterShanxin/Meowcal-Sub/issues");
      issues.push(JSON.parse(opts.body));
      if (maybeCreated) throw new Error("network timed out after GitHub accepted POST");
      return Response.json({ number: issues.length }, { status: 201 });
    }
    assert.match(url, /\/issues\/1$/);
    return Response.json({ ...issues[0], number: 1, performed_via_github_app: { id: 123 } });
  });
  env.REPORTS = { idFromName: () => 1, get: () => ledger };
  return {
    db,
    stored,
    issues,
    env,
    doFetch: (req) => worker.fetch(req, env),
    failCreate: () => {
      maybeCreated = true;
    },
  };
}

test("publishes only enumerated metadata and retains the first private body across retries", async (t) => {
  const f = fixture(t);
  const first = await f.doFetch(submit());
  assert.deepEqual(await first.json(), { issueNumber: 1 });
  assert.equal(f.issues.length, 1);
  assert.deepEqual(f.issues[0], {
    title: "App feedback · Translation quality",
    body: "App version: `0.10.0`\n\nFeedback reference: `" + requestId + "`",
    labels: ["source:app"],
  });
  const saved = [...f.stored.values()][0];
  assert.ok(new TextDecoder().decode(saved).includes(privateSample));
  assert.equal(
    (await f.doFetch(submit({ body: "age-encryption.org/v1\n" + "b".repeat(100) }))).status,
    200,
  );
  assert.strictEqual([...f.stored.values()][0], saved);
  assert.equal(f.issues.length, 1);
  assert.equal((await f.doFetch(submit({ type: "error" }))).status, 409);
});

test("concurrent requests grant one create operation", async (t) => {
  const f = fixture(t);
  const responses = await Promise.all(Array.from({ length: 6 }, () => f.doFetch(submit())));
  assert.ok(responses.some((r) => r.status === 200));
  assert.ok(responses.every((r) => [200, 503].includes(r.status)));
  assert.equal(f.issues.length, 1);
  assert.equal((await f.doFetch(submit())).status, 200);
});

test("a lost GitHub response is reconciled without a second create", async (t) => {
  const f = fixture(t);
  f.failCreate();
  assert.equal((await f.doFetch(submit())).status, 503);
  assert.equal((await f.doFetch(submit())).status, 503);
  f.db.exec("UPDATE reports SET attempted=1");
  assert.deepEqual(await (await f.doFetch(submit())).json(), { issueNumber: 1 });
  assert.equal(f.issues.length, 1);
});

test("only authenticated archive reads return stored bytes", async (t) => {
  if (!crypto.subtle.timingSafeEqual) {
    Object.defineProperty(crypto.subtle, "timingSafeEqual", {
      configurable: true,
      value: timingSafeEqual,
    });
    t.after(() => delete crypto.subtle.timingSafeEqual);
  }
  const f = fixture(t);
  await f.doFetch(submit());
  const url = "https://feedback.test/v1/issues/1/archive";
  for (const token of ["", "Bearer bad"]) {
    assert.equal(
      (await f.doFetch(new Request(url, { headers: { Authorization: token } }))).status,
      404,
    );
  }
  const result = await f.doFetch(
    new Request(url, { headers: { Authorization: "Bearer " + f.env.ARCHIVE_READ_TOKEN } }),
  );
  assert.equal(result.status, 200);
  assert.equal(result.headers.get("cache-control"), "no-store");
  assert.equal(await result.text(), await submit().text());
  f.db.exec("UPDATE reports SET created=1");
  const expired = await f.doFetch(
    new Request(url, { headers: { Authorization: "Bearer " + f.env.ARCHIVE_READ_TOKEN } }),
  );
  assert.equal(expired.status, 404);
});

test("malformed reports, oversized or unencrypted bodies and quota failures create no issues", async (t) => {
  const f = fixture(t);
  const cases = [
    [{ id: "../issues" }, 400],
    [{ type: "__proto__" }, 400],
    [{ version: "0.1.0-private-text" }, 400],
    [{ body: privateSample.repeat(5) }, 400],
    [{ headers: { "Content-Length": String(16 * 1024 * 1024 + 1) } }, 413],
    [{ headers: { "Content-Length": "101" } }, 400],
  ];
  for (const [params, status] of cases)
    assert.equal((await f.doFetch(submit(params))).status, status);
  f.env.INTAKE_LIMIT.limit = async () => ({ success: false });
  assert.equal((await f.doFetch(submit())).status, 429);
  assert.equal(f.stored.size, 0);
  assert.equal(f.issues.length, 0);
});

test("the global quota also applies across different client addresses", async (t) => {
  const f = fixture(t);
  const reserve = f.db.prepare(
    "INSERT INTO reports(id,type,version,created) VALUES (?, 'other','0.10.0',?)",
  );
  for (let i = 0; i < 120; i++) reserve.run(crypto.randomUUID(), Date.now());
  assert.equal((await f.doFetch(submit())).status, 429);
  assert.equal(f.issues.length, 0);
});

test("the signed installation credential is scoped to the target repository and issues", async (t) => {
  const keys = await crypto.subtle.generateKey(
    {
      name: "RSASSA-PKCS1-v1_5",
      hash: "SHA-256",
      modulusLength: 2048,
      publicExponent: new Uint8Array([1, 0, 1]),
    },
    true,
    ["sign", "verify"],
  );
  const key = Buffer.from(await crypto.subtle.exportKey("pkcs8", keys.privateKey)).toString(
    "base64",
  );
  t.mock.method(globalThis, "fetch", async (url, options) => {
    assert.equal(url, "https://api.github.com/app/installations/246/access_tokens");
    assert.deepEqual(JSON.parse(options.body), {
      repositories: ["Meowcal-Sub"],
      permissions: { issues: "write" },
    });
    const [head, claims, sig] = options.headers.Authorization.slice(7).split(".");
    assert.equal(JSON.parse(Buffer.from(claims, "base64url")).iss, "123");
    assert.ok(
      await crypto.subtle.verify(
        "RSASSA-PKCS1-v1_5",
        keys.publicKey,
        Buffer.from(sig, "base64url"),
        new TextEncoder().encode(head + "." + claims),
      ),
    );
    return Response.json({ token: "installation-token" });
  });
  const token = await installationToken({
    GITHUB_APP_ID: "123",
    GITHUB_INSTALLATION_ID: "246",
    GITHUB_APP_PRIVATE_KEY: "-----BEGIN PRIVATE KEY-----\n" + key + "\n-----END PRIVATE KEY-----",
  });
  assert.equal(token, "installation-token");
});
