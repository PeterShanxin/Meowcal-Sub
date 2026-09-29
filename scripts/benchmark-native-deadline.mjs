import { writeFile } from "node:fs/promises";
import { connectNativePage } from "./native-benchmark-client.mjs";

const [output, port = "9241"] = process.argv.slice(2);
if (!output) throw new Error("Usage: benchmark-native-deadline.mjs output.json [port]");
const main = await connectNativePage("http://tauri.localhost/", port);
try {
  const result = await main.evaluate(
    `(${async function () {
      const rows = [];
      const long =
        "When we arrived at the station, the last train had already left. We decided to walk back through the empty streets, carrying our heavy bags and wondering whether anyone would still be awake to open the door. The rain was getting stronger, and neither of us had brought an umbrella for the journey.";
      for (let sample = 0; sample < 5; sample++) {
        const row = { sample, requests: [] };
        for (const text of [long, "Please close the door behind you."]) {
          const startedAt = Date.now();
          const started = performance.now();
          const outcome = await window.TauriBridge.invoke("translate_once", {
            text,
            sourceLanguage: "en-US",
            targetLanguage: "zh-CN",
          });
          row.requests.push({ text, startedAt, elapsedMs: performance.now() - started, outcome });
        }
        rows.push(row);
      }
      return rows;
    }.toString()})()`,
  );
  await writeFile(output, JSON.stringify(result, null, 2) + "\n");
  console.log(
    JSON.stringify(
      result.map((row) =>
        row.requests.map((request) => ({
          ms: request.elapsedMs,
          state: request.outcome.displayState,
        })),
      ),
    ),
  );
} finally {
  main.close();
}
