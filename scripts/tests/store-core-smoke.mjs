import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import {
  mkdirSync,
  existsSync,
  readFileSync,
  writeFileSync,
  appendFileSync,
  createReadStream,
} from "node:fs";
import { createHash } from "node:crypto";
import { resolve, join } from "node:path";

if (process.env.GITHUB_ACTIONS !== "true" || !process.env.RUNNER_TEMP) {
  throw new Error("This fresh-download check runs only in the proposed Actions job.");
}
const root = resolve(process.env.RUNNER_TEMP, "meowcal-store-core-smoke");
if (existsSync(root)) throw new Error("Refusing an existing test directory.");
mkdirSync(root);
const lock = JSON.parse(readFileSync("config/meowcal-core.lock.json", "utf8"));
const metadata = JSON.parse(readFileSync("src-tauri/resources/core/meowcal-core.json", "utf8"));
const executable = resolve("src-tauri/resources/core/meowcal-core.exe");
const sha256 = createHash("sha256").update(readFileSync(executable)).digest("hex");
if (
  metadata.architecture !== "x64" ||
  metadata.coreVersion !== lock.coreVersion ||
  sha256 !== metadata.executableSha256
) {
  throw new Error("Core resource does not match the reviewed x64 package metadata.");
}
const evidence = {
  started: new Date().toISOString(),
  executable,
  sha256,
  storageRoot: join(root, "cache"),
  freshDownload: true,
  legacyRoots: [],
  desktopLifecycleVerified: false,
  pass: false,
  steps: [],
};
const save = () => writeFileSync(join(root, "result.json"), JSON.stringify(evidence, null, 2));
save();
const child = spawn(executable, [], { windowsHide: true, stdio: ["pipe", "pipe", "pipe"] });
evidence.pid = child.pid;
let sequence = 0;
let pending;
let startupError;
function rejectPending(error) {
  if (!pending) return;
  clearTimeout(pending.timer);
  pending.reject(error);
  pending = undefined;
}
child.on("error", (error) => {
  startupError = error;
  rejectPending(error);
});
child.stdin.on("error", rejectPending);
const exited = new Promise((resolveExit) =>
  child.once("close", (code, signal) => {
    evidence.exit = { code, signal };
    rejectPending(new Error("Core exited before replying."));
    resolveExit();
  }),
);
child.stderr.on("data", (bytes) => appendFileSync(join(root, "stderr.log"), bytes));
createInterface({ input: child.stdout }).on("line", (line) => {
  let frame;
  try {
    frame = JSON.parse(line);
  } catch (error) {
    rejectPending(error);
    return;
  }
  if (frame.event) {
    writeFileSync(
      join(root, "progress.json"),
      JSON.stringify({ time: new Date().toISOString(), ...frame }, null, 2),
    );
    return;
  }
  if (!pending || frame.id !== pending.id) return;
  const request = pending;
  pending = undefined;
  clearTimeout(request.timer);
  if (frame.error) request.reject(new Error(JSON.stringify(frame.error)));
  else request.resolve(frame.result);
});
function request(method, params = {}, timeout = 90000) {
  if (startupError) return Promise.reject(startupError);
  if (pending) return Promise.reject(new Error("Only one Core request may be active."));
  return new Promise((resolveRequest, reject) => {
    const id = ++sequence;
    pending = {
      id,
      resolve: resolveRequest,
      reject,
      timer: setTimeout(() => rejectPending(new Error(`${method} timed out`)), timeout),
    };
    child.stdin.write(JSON.stringify({ id, api: 1, method, params }) + "\n", (error) => {
      if (error) rejectPending(error);
    });
  });
}
async function record(method, params, timeout) {
  const result = await request(method, params, timeout);
  evidence.steps.push({ [method]: result });
  save();
  return result;
}
try {
  await record("hello", {
    client: "sub1",
    profile: "development",
    expectedVersion: lock.coreVersion,
    forceCpu: true,
    storageRoot: evidence.storageRoot,
    legacyRoots: [],
  });
  const before = await record("status");
  if (before.installed) throw new Error("Expected an empty engine cache.");
  await record("install", {}, 20 * 60 * 1000);
  const ready = await record("ready");
  if (!ready.ready || ready.acceleration !== "cpu") throw new Error("CPU runtime is not ready.");
  const manifest = JSON.parse(readFileSync("core/config/engine-manifest.v1.json", "utf8"));
  const hash = createHash("sha256");
  for await (const bytes of createReadStream(ready.installPaths.model)) hash.update(bytes);
  evidence.modelHash = hash.digest("hex");
  if (evidence.modelHash !== manifest.model.artifact.sha256)
    throw new Error("Downloaded model digest mismatch.");
  const completion = await record("complete", {
    request: {
      model: manifest.model.id,
      messages: [{ role: "user", content: "Translate into Chinese: Hello, how are you?" }],
      max_tokens: 128,
      temperature: 0,
      stream: false,
    },
    timeoutMs: 30000,
  });
  if (!/\p{Script=Han}/u.test(completion.choices?.[0]?.message?.content ?? "")) {
    throw new Error("The sample returned no Chinese translation.");
  }
  evidence.pass = true;
} catch (error) {
  evidence.error = String(error);
  process.exitCode = 1;
} finally {
  if (child.exitCode === null && !startupError) {
    try {
      await request("shutdown", {}, 5000);
    } catch (error) {
      evidence.shutdownError = String(error);
      evidence.pass = false;
      process.exitCode = 1;
    }
    child.stdin.end();
    let timer;
    await Promise.race([
      exited,
      new Promise((resolveWait) => {
        timer = setTimeout(resolveWait, 3000);
      }),
    ]);
    clearTimeout(timer);
    if (child.exitCode === null) {
      evidence.forcedTermination = true;
      evidence.pass = false;
      process.exitCode = 1;
      child.kill();
      await exited;
    }
  }
  if (evidence.exit?.code !== 0) {
    evidence.pass = false;
    process.exitCode = 1;
  }
  evidence.finished = new Date().toISOString();
  save();
}
