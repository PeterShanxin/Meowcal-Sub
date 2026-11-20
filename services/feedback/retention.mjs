import { issueHistory } from "./github.mjs";

const DAY = 86400000;
export const ARCHIVE_GRACE = 30 * DAY;
const RETRY = 5 * 60000;
const BATCH = 8;

export function migrateRetention(sql) {
  // Old ledgers have only creation and receipt data. Start their first audit at
  // the beginning of the issue's timeline, regardless of the age of the upload.
  const existing = sql.exec("PRAGMA table_info(reports)").toArray();
  for (const [name, definition] of [
    ["next_check", "INTEGER NOT NULL DEFAULT 0"],
    ["history_cursor", "TEXT"],
    ["closed_at", "INTEGER"],
    ["archive_state", "INTEGER NOT NULL DEFAULT 0"], // 0 retained, 1 deletion pending, 2 deleted
  ]) {
    if (!existing.some((column) => column.name === name)) {
      sql.exec("ALTER TABLE reports ADD COLUMN " + name + " " + definition);
    }
  }
  sql.exec(
    "CREATE INDEX IF NOT EXISTS archive_due ON reports(next_check,id) WHERE archive_state<2",
  );
}

export class ArchiveRetention {
  constructor(ledger) {
    this.ledger = ledger;
    this.active = new Map();
    this.sweeping = false;
  }

  async review(row, getToken) {
    if (this.active.has(row.id)) return this.active.get(row.id);
    // Earlier rows in a sweep can await network I/O while this row changes.
    row = this.ledger.one("SELECT * FROM reports WHERE id=?", row.id);
    if (row.archive_state === 2) return "gone";
    if (this.ledger.uploading.has(row.id)) return "pending";
    const operation = this.check(row, getToken);
    this.active.set(row.id, operation);
    try {
      return await operation;
    } finally {
      this.active.delete(row.id);
    }
  }

  async purge(id) {
    const { sql, env } = this.ledger;
    // The tombstone prevents disclosure and re-upload even if R2 is temporarily down.
    sql.exec("UPDATE reports SET archive_state=1, next_check=? WHERE id=?", Date.now() + RETRY, id);
    await env.ARCHIVES.delete("reports/" + id);
    sql.exec("UPDATE reports SET archive_state=2 WHERE id=?", id);
    return "gone";
  }

  async check(row, getToken) {
    const { sql } = this.ledger;
    const now = Date.now();
    if (row.archive_state === 1) return this.purge(row.id);
    sql.exec("UPDATE reports SET next_check=? WHERE id=? AND archive_state=0", now + RETRY, row.id);
    if (!row.issue) {
      if (row.created + ARCHIVE_GRACE > now) {
        sql.exec("UPDATE reports SET next_check=? WHERE id=?", row.created + ARCHIVE_GRACE, row.id);
        return "pending";
      }
      if (!row.attempted) return this.purge(row.id);
      const found = await this.ledger.lookup(row, await getToken());
      if (!found) {
        // A maintainer can edit the issue body and remove its searchable reference.
        sql.exec("UPDATE reports SET next_check=? WHERE id=?", now + DAY, row.id);
        return "pending";
      }
      this.ledger.receipt(row.id, found);
      row = { ...row, issue: found };
    }
    const page = await issueHistory(row.issue, row.history_cursor, await getToken());
    let closed = row.closed_at;
    for (const event of page.events) {
      if (event.type === "ClosedEvent" && closed === null) {
        closed = event.time;
      } else if (event.type === "ReopenedEvent" && closed !== null && event.time >= closed) {
        if (event.time >= closed + ARCHIVE_GRACE) return this.purge(row.id);
        closed = null;
      } else {
        throw new Error("Inconsistent issue timeline");
      }
    }
    if (!page.more) {
      if ((closed === null) !== (page.state === "OPEN"))
        throw new Error("Incomplete issue history");
      // GitHub can publish the timeline event a second after the issue update.
      // A different closure may appear in one field before its events arrive.
      if (closed !== null) {
        if (Math.abs(closed - page.closedAt) > 5000)
          throw new Error("Issue closure timestamps disagree");
        closed = page.closedAt;
      }
      if (closed !== null && now >= closed + ARCHIVE_GRACE) return this.purge(row.id);
    }
    const next = page.more
      ? now
      : Math.min(now + DAY, closed === null ? Infinity : closed + ARCHIVE_GRACE);
    sql.exec(
      "UPDATE reports SET closed_at=?, history_cursor=?, next_check=? WHERE id=? AND archive_state=0",
      closed,
      page.cursor ?? row.history_cursor,
      next,
      row.id,
    );
    return page.more ? "pending" : "available";
  }

  async sweep() {
    if (this.sweeping) return;
    this.sweeping = true;
    try {
      const rows = this.ledger.sql
        .exec(
          "SELECT * FROM reports WHERE archive_state<2 AND next_check<=? ORDER BY next_check,id LIMIT ?",
          Date.now(),
          BATCH,
        )
        .toArray();
      let token;
      const getToken = () => (token ??= this.ledger.token());
      let failure = false;
      for (const row of rows) {
        try {
          await this.review(row, getToken);
        } catch {
          failure = true;
        }
      }
      if (failure) throw new Error("Archive sweep will retry");
    } finally {
      this.sweeping = false;
    }
  }
}
