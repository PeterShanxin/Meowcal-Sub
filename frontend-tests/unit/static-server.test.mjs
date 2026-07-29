import { once } from "node:events";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";

import { createStaticServer } from "../../scripts/serve-frontend.mjs";

const cleanupTasks = [];

afterEach(async () => {
  await Promise.all(cleanupTasks.splice(0).map((cleanup) => cleanup()));
});

async function startFixtureServer() {
  const rootDirectory = await mkdtemp(path.join(tmpdir(), "meowcal-static-test-"));
  await writeFile(path.join(rootDirectory, "index.html"), "<h1>Meowcal fixture</h1>", "utf8");

  const server = createStaticServer({ rootDirectory });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");

  cleanupTasks.push(
    () => new Promise((resolve) => server.close(resolve)),
    () => rm(rootDirectory, { recursive: true, force: true }),
  );

  const address = server.address();
  return `http://127.0.0.1:${address.port}`;
}

describe("createStaticServer", () => {
  it("serves index.html from loopback", async () => {
    const baseUrl = await startFixtureServer();
    const response = await fetch(`${baseUrl}/`);

    expect(response.status).toBe(200);
    expect(response.headers.get("content-type")).toContain("text/html");
    expect(await response.text()).toBe("<h1>Meowcal fixture</h1>");
  });

  it("returns 404 for a missing asset", async () => {
    const baseUrl = await startFixtureServer();
    const response = await fetch(`${baseUrl}/missing.js`);

    expect(response.status).toBe(404);
  });

  it("rejects non-read methods", async () => {
    const baseUrl = await startFixtureServer();
    const response = await fetch(`${baseUrl}/`, { method: "POST" });

    expect(response.status).toBe(405);
  });
});
