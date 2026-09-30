import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

const [directory] = process.argv.slice(2);
if (!directory) throw new Error("Usage: summarize-native-comparison.mjs native-final-directory");
const read = async (name) => JSON.parse(await readFile(join(directory, name + ".json"), "utf8"));
function describe(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const quantile = (p) => {
    const index = (sorted.length - 1) * p;
    const low = Math.floor(index);
    return sorted[low] + (sorted[Math.ceil(index)] - sorted[low]) * (index - low);
  };
  return {
    n: values.length,
    p50: quantile(0.5),
    p95: quantile(0.95),
    min: sorted[0],
    max: sorted.at(-1),
    values,
  };
}
async function pipeline(name) {
  const data = await read(name);
  const rows = data.cues.map((cue, index) => {
    const start = data.fixturePaints.find((paint) => paint.id === cue.id).paintedAt;
    const end =
      data.fixturePaints.find((paint) => paint.id === data.cues[index + 1]?.id)?.paintedAt ??
      start + 4000;
    const event = data.events.find(
      (item) => item.at >= start && item.at < end && item.payload.original === cue.text,
    );
    if (!event || event.payload.displayState !== "translated")
      throw new Error(`Missing translation: ${cue.id}`);
    const paint = data.paints.find(
      (item) => item.at >= event.at && item.at < end && item.text === event.payload.translated,
    );
    if (!paint) throw new Error(`Missing overlay render: ${cue.id}`);
    return {
      text: cue.text,
      translated: event.payload.translated,
      modelMs: event.payload.modelMs,
      totalMs: event.payload.totalMs,
      renderMs: paint.at - start,
    };
  });
  return {
    rows,
    total: describe(rows.map((row) => row.totalMs)),
    model: describe(rows.map((row) => row.modelMs)),
    render: describe(rows.map((row) => row.renderMs)),
  };
}
async function deadline(name) {
  const rows = await read(name);
  return {
    long: describe(rows.map((row) => row.requests[0].elapsedMs)),
    following: describe(rows.map((row) => row.requests[1].elapsedMs)),
    unavailableLong: rows.filter((row) => row.requests[0].outcome.displayState !== "translated")
      .length,
    unavailableFollowing: rows.filter(
      (row) => row.requests[1].outcome.displayState !== "translated",
    ).length,
  };
}
const refresh = (await read("r3-refresh")).rows;
const result = {
  quantiles:
    "Linear interpolation; small samples describe these runs, not a reliable population tail estimate.",
  baselinePipeline: await pipeline("baseline-repeat-pipeline"),
  r1Pipeline: await pipeline("r1-pipeline"),
  r2Pipeline: await pipeline("r2-pipeline"),
  r3Pipeline: await pipeline("r3-pipeline"),
  baselineDeadline: await deadline("baseline-repeat-deadline"),
  r1Deadline: await deadline("r1-deadline"),
  r2Stop: describe((await read("r2-stop-normal")).map((row) => row.stoppedMs)),
  r2Delayed: await read("r2-stop-delayed"),
  r3Quiet: describe(refresh.filter((row) => !row.refresh).map((row) => row.elapsedMs)),
  r3Busy: describe(refresh.filter((row) => row.refresh).map((row) => row.elapsedMs)),
  r3ReadinessCalls: refresh
    .flatMap((row) => row.calls)
    .filter((call) => call.command === "make_engine_ready").length,
};
await writeFile(join(directory, "summary.json"), JSON.stringify(result, null, 2) + "\n");
console.log(JSON.stringify(result));
