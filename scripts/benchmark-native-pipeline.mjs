import { writeFile, readFile } from "node:fs/promises";
import { join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { connectNativePage } from "./native-benchmark-client.mjs";

const [output, fixture, port = "9241"] = process.argv.slice(2);
if (!output || !fixture)
  throw new Error("Usage: benchmark-native-pipeline.mjs output.json fixture-directory [port]");
const main = await connectNativePage("http://tauri.localhost/", port);
const overlay = await connectNativePage("http://tauri.localhost/overlay.html", port);
try {
  await main.evaluate(
    `(async()=>{window.benchmarkEvents = []; window.benchmarkUnlisten = await window.TauriBridge.event.listen("translation-update", e => window.benchmarkEvents.push({at:Date.now(),payload:e.payload}));})()`,
  );
  await overlay.evaluate(`window.benchmarkPaints = []; window.benchmarkObserver = new MutationObserver(() => {
    const text = document.getElementById("subtitle-text").textContent;
    requestAnimationFrame(() => requestAnimationFrame(() => window.benchmarkPaints.push({at:Date.now(),text})));
  }); window.benchmarkObserver.observe(document.getElementById("subtitle-text"), {subtree:true,childList:true,characterData:true});`);
  const inputs = [
    "Please close the door behind you.",
    "We should wait until tomorrow.",
    "I left my keys on the table.",
    "There is nobody in the room.",
    "The train leaves in ten minutes.",
  ];
  const cues = [];
  for (let pass = 0; pass < 2; pass++) {
    for (const [index, text] of inputs.entries()) {
      const cue = { id: `${Date.now()}-${pass}-${index}`, text, submittedAt: Date.now() };
      cues.push(cue);
      await writeFile(join(fixture, "cue.json"), JSON.stringify(cue));
      await delay(4000);
    }
  }
  const events = await main.evaluate("window.benchmarkEvents");
  const paints = await overlay.evaluate("window.benchmarkPaints");
  const fixturePaints = (await readFile(join(fixture, "cues.jsonl"), "utf8"))
    .trim()
    .split(/\r?\n/)
    .map(JSON.parse)
    .filter((row) => cues.some((cue) => cue.id === row.id));
  await writeFile(
    output,
    JSON.stringify(
      {
        cues,
        fixturePaints,
        events,
        paints,
        limitation:
          "Fixture WM_PAINT completion to overlay double requestAnimationFrame. This is a render proxy, not proof of desktop presentation.",
      },
      null,
      2,
    ) + "\n",
  );
  console.log(
    JSON.stringify({ cues: cues.length, events: events.length, paintChanges: paints.length }),
  );
} finally {
  await main.evaluate("window.benchmarkUnlisten?.()");
  await overlay.evaluate("window.benchmarkObserver?.disconnect()");
  main.close();
  overlay.close();
}
