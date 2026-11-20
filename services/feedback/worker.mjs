import { createIssue, findIssue, installationToken, validType } from "./github.mjs";
import { ARCHIVE_GRACE, ArchiveRetention, migrateRetention } from "./retention.mjs";

const MAX_BYTES = 16 * 1024 * 1024;
const DAILY_REPORTS = 120;
const AGE_HEADER = new TextEncoder().encode("age-encryption.org/v1\n");
const UUID = /^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/;
const VERSION = /^(0|[1-9]\d{0,3})\.(0|[1-9]\d{0,3})\.(0|[1-9]\d{0,3})$/;

const reply = (status, fields = {}) =>
  Response.json(fields, {
    status,
    headers: { "Cache-Control": "no-store", ...(status === 503 ? { "Retry-After": "30" } : {}) },
  });

function metadata(request) {
  const id = request.headers.get("X-Report-Id");
  const type = request.headers.get("X-Report-Type");
  const version = request.headers.get("X-App-Version");
  return UUID.test(id) && validType(type) && VERSION.test(version) ? { id, type, version } : null;
}

async function readCiphertext(request) {
  const size = Number(request.headers.get("Content-Length"));
  if (!Number.isSafeInteger(size) || size < 100 || size > MAX_BYTES || !request.body) return null;
  const reader = request.body.getReader();
  const bytes = new Uint8Array(size);
  let written = 0;
  try {
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      if (written + value.length > size) {
        await reader.cancel();
        return null;
      }
      bytes.set(value, written);
      written += value.length;
    }
    return written === size && AGE_HEADER.every((b, i) => bytes[i] === b) ? bytes : null;
  } finally {
    reader.releaseLock();
  }
}

async function mayDownload(request, env) {
  if (!env.ARCHIVE_READ_TOKEN || env.ARCHIVE_READ_TOKEN.length < 32) return false;
  const header = request.headers.get("Authorization") ?? "";
  if (header.length > 512) return false;
  const digest = async (input) =>
    new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(input)));
  const [actual, expected] = await Promise.all([
    digest(header),
    digest("Bearer " + env.ARCHIVE_READ_TOKEN),
  ]);
  return crypto.subtle.timingSafeEqual(actual, expected);
}

export default {
  async scheduled(_event, env) {
    const response = await env.REPORTS.get(env.REPORTS.idFromName("repository")).fetch(
      "https://ledger.internal/sweep",
      { method: "POST" },
    );
    if (!response.ok) throw new Error("Archive sweep incomplete");
  },
  async fetch(request, env) {
    try {
      const { pathname, search, protocol } = new URL(request.url);
      if (search || protocol !== "https:") return reply(404);
      if (request.method === "POST" && pathname === "/v1/reports") {
        if (
          !metadata(request) ||
          request.headers.get("Content-Type") !== "application/octet-stream"
        )
          return reply(400);
        const size = Number(request.headers.get("Content-Length"));
        if (!Number.isSafeInteger(size) || size < 100 || size > MAX_BYTES) return reply(413);
        const ip = request.headers.get("CF-Connecting-IP") || "unknown";
        if (!(await env.INTAKE_LIMIT.limit({ key: ip })).success) return reply(429);
      } else if (
        request.method === "GET" &&
        /^\/v1\/issues\/[1-9]\d{0,9}\/archive$/.test(pathname)
      ) {
        if (!(await mayDownload(request, env))) return reply(404);
      } else {
        return reply(404);
      }
      return await env.REPORTS.get(env.REPORTS.idFromName("repository")).fetch(request);
    } catch {
      return reply(503);
    }
  },
};

export class ReportLedger {
  constructor(ctx, env) {
    this.sql = ctx.storage.sql;
    this.env = env;
    this.uploading = new Set();
    this.sql.exec(`CREATE TABLE IF NOT EXISTS reports (
      id TEXT PRIMARY KEY, type TEXT NOT NULL, version TEXT NOT NULL, created INTEGER NOT NULL,
      attempted INTEGER, issue INTEGER UNIQUE
    )`);
    this.sql.exec("CREATE INDEX IF NOT EXISTS report_day ON reports(created)");
    migrateRetention(this.sql);
    this.retention = new ArchiveRetention(this);
  }

  one(query, ...args) {
    return this.sql.exec(query, ...args).toArray()[0];
  }

  async fetch(request) {
    if (request.method === "POST" && request.url === "https://ledger.internal/sweep") {
      await this.retention.sweep();
      return reply(200);
    }
    if (request.method === "GET") {
      const number = Number(new URL(request.url).pathname.split("/")[3]);
      const row = this.one("SELECT * FROM reports WHERE issue=?", number);
      if (!row) return reply(404);
      let token;
      const getToken = () => (token ??= this.token());
      let status;
      for (let page = 0; page < 8; page++) {
        status = await this.retention.review(row, getToken);
        if (status !== "pending" || this.uploading.has(row.id)) break;
      }
      if (status !== "available") return reply(status === "gone" ? 404 : 503);
      const object = await this.env.ARCHIVES.get("reports/" + row.id);
      return object
        ? new Response(object.body, {
            headers: { "Content-Type": "application/octet-stream", "Cache-Control": "no-store" },
          })
        : reply(404);
    }
    const report = metadata(request);
    if (!report) return reply(400);
    const { id, type, version } = report;
    const existing = this.one("SELECT * FROM reports WHERE id=?", id);
    if (existing) {
      if (existing.version !== version || existing.type !== type) return reply(409);
      if (existing.issue) return reply(200, { issueNumber: existing.issue });
      if (existing.archive_state || Date.now() >= existing.created + ARCHIVE_GRACE) {
        await this.retention.review(existing, () => this.token());
        const resolved = this.one("SELECT issue,archive_state FROM reports WHERE id=?", id);
        if (resolved.issue) return reply(200, { issueNumber: resolved.issue });
        return reply(resolved.archive_state ? 410 : 503);
      }
    }
    // One isolate also serves downloads. Keep aggregate buffered upload memory bounded.
    if (this.uploading.size >= 2 || this.uploading.has(id) || this.retention.active.has(id))
      return reply(503);
    this.uploading.add(id);
    try {
      if (existing?.attempted) {
        if (Date.now() - existing.attempted < 30000) return reply(503);
        return this.receipt(id, await this.lookup(report, await this.token()));
      }
      return await this.submit(request, report);
    } finally {
      this.uploading.delete(id);
    }
  }

  async submit(request, report) {
    const { id, version, type } = report;
    const bytes = await readCiphertext(request);
    if (!bytes) return reply(400);
    const now = Date.now();
    const today = now - (now % 86400000);
    this.sql.exec(
      `INSERT OR IGNORE INTO reports (id,type,version,created,next_check)
      SELECT ?,?,?,?,? WHERE (SELECT COUNT(*) FROM reports WHERE created>=?)<?`,
      id,
      type,
      version,
      now,
      now + ARCHIVE_GRACE,
      today,
      DAILY_REPORTS,
    );
    const row = this.one("SELECT * FROM reports WHERE id=?", id);
    if (!row) return reply(429);
    if (row.version !== version || row.type !== type) return reply(409);
    if (row.issue) return reply(200, { issueNumber: row.issue });
    // First ciphertext wins, including concurrent retries after a lost response.
    await this.env.ARCHIVES.put("reports/" + id, bytes, {
      onlyIf: new Headers({ "If-None-Match": "*" }),
    });
    const token = await this.token();
    const owned = this.one(
      "UPDATE reports SET attempted=? WHERE id=? AND attempted IS NULL AND issue IS NULL RETURNING id",
      now,
      id,
    );
    if (!owned) return reply(503);
    const created = await this.create(report, token);
    if (!created.ok) {
      // GitHub rejected this request before creating an issue. A network or 5xx
      // error is ambiguous and keeps the reservation for reconciliation.
      if ([400, 401, 403, 404, 410, 422, 429].includes(created.status)) {
        this.sql.exec("UPDATE reports SET attempted=NULL WHERE id=? AND issue IS NULL", id);
      }
      return reply(503);
    }
    const number = (await created.json()).number;
    return this.receipt(id, number);
  }

  token() {
    return installationToken(this.env);
  }
  create(report, token) {
    return createIssue(report, token);
  }

  lookup(report, token) {
    return findIssue(report, token, this.env.GITHUB_APP_ID);
  }

  receipt(id, number) {
    if (!Number.isSafeInteger(number) || number < 1) return reply(503);
    this.sql.exec(
      "UPDATE reports SET issue=?,next_check=? WHERE id=? AND archive_state=0",
      number,
      Date.now(),
      id,
    );
    return reply(200, { issueNumber: number });
  }
}
