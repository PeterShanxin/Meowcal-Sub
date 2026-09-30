import { writeFile } from "node:fs/promises";

// Run against an owned Debug Tauri app with WebView2 remote debugging enabled.
// This measures native RPCs and manager latency, not capture-to-visible latency.
const [output, port = "9241"] = process.argv.slice(2);
if (!output) throw new Error("Usage: node scripts/benchmark-native-refresh.mjs output.json [port]");
const pages = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = pages.find((entry) => entry.url === "http://tauri.localhost/");
if (!page) throw new Error("Native main WebView not found");
const socket = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  socket.addEventListener("open", resolve, { once: true });
  socket.addEventListener("error", reject, { once: true });
});
const result = new Promise((resolve, reject) => {
  socket.addEventListener("message", ({ data }) => {
    const reply = JSON.parse(data);
    if (reply.id !== 1) return;
    if (reply.error || reply.result.exceptionDetails) reject(new Error(JSON.stringify(reply)));
    else resolve(reply.result.result.value);
  });
});
socket.send(
  JSON.stringify({
    id: 1,
    method: "Runtime.evaluate",
    params: {
      awaitPromise: true,
      returnByValue: true,
      expression: `(${async function () {
        const { window, document } = globalThis;
        const bridge = window.TauriBridge;
        const controller = document.querySelector("meowcal-app").controller;
        const original = bridge.invoke;
        const calls = [];
        bridge.invoke = async function (command, args) {
          const call = { command, startMs: performance.now() };
          calls.push(call);
          try {
            return await original.call(this, command, args);
          } finally {
            call.endMs = performance.now();
          }
        };
        const rows = [];
        const inputs = [
          "The train leaves in ten minutes.",
          "Please close the door behind you.",
          "We should wait until tomorrow.",
          "I left my keys on the table.",
          "There is nobody in the room.",
        ];
        try {
          for (let pass = 0; pass < 2; pass++) {
            for (const [index, text] of inputs.entries()) {
              const refresh = (index + pass) % 2 === 1;
              const begin = performance.now();
              const firstCall = calls.length;
              const translation = bridge
                .invoke("translate_once", {
                  text,
                  sourceLanguage: "en-US",
                  targetLanguage: "zh-CN",
                })
                .then((outcome) => ({ outcome, elapsedMs: performance.now() - begin }));
              await new Promise((resolve) => setTimeout(resolve, 100));
              const refreshed = refresh
                ? Promise.all(Array.from({ length: 3 }, () => controller.refresh()))
                : Promise.resolve();
              const translated = await translation;
              await refreshed;
              rows.push({ text, refresh, ...translated, calls: calls.slice(firstCall) });
            }
          }
          return { userAgent: navigator.userAgent, rows };
        } finally {
          bridge.invoke = original;
        }
      }.toString()})()`,
    },
  }),
);
try {
  const data = await result;
  await writeFile(output, JSON.stringify(data, null, 2) + "\n");
  console.log(
    JSON.stringify(
      data.rows.map(({ elapsedMs, refresh, outcome, calls }) => ({
        elapsedMs,
        refresh,
        outcome,
        readyCalls: calls.filter((call) => call.command === "make_engine_ready").length,
      })),
      null,
      2,
    ),
  );
} finally {
  socket.close();
}
