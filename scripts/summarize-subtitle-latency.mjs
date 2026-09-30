import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

const [directory] = process.argv.slice(2);
if (!directory) throw new Error("Usage: summarize-subtitle-latency.mjs results-directory");
const read = async (name) => JSON.parse(await readFile(join(directory, name), "utf8"));
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
const pipeline = await read("pipeline.json");
const proxy = pipeline.cues.map((cue) => {
  const source = pipeline.fixturePaints.find((paint) => paint.id === cue.id);
  const event = pipeline.events.find(
    (item) => item.at >= source.paintedAt && item.payload.original === cue.text,
  );
  if (!event || event.payload.displayState !== "translated")
    throw new Error(`Missing translation: ${cue.id}`);
  const paint = pipeline.paints.find(
    (item) => item.at >= event.at && item.text === event.payload.translated,
  );
  if (!paint) throw new Error(`Missing overlay render proxy: ${cue.id}`);
  return paint.at - source.paintedAt;
});
const refresh = (await read("refresh.json")).rows;
const deadline = await read("deadline.json");
const result = {
  quantiles:
    "Linear interpolation; small samples describe these runs, not reliable population tail latency.",
  pipelineRenderProxyMs: describe(proxy),
  acceptedFrameTotalMs: describe(pipeline.events.map((item) => item.payload.totalMs)),
  managerMs: describe(pipeline.events.map((item) => item.payload.modelMs)),
  normalStopMs: describe((await read("stop-normal.json")).map((item) => item.stoppedMs)),
  delayedOcr: await read("stop-delayed.json"),
  quietTranslateMs: describe(refresh.filter((row) => !row.refresh).map((row) => row.elapsedMs)),
  refreshTranslateMs: describe(refresh.filter((row) => row.refresh).map((row) => row.elapsedMs)),
  readinessRpcCount: refresh
    .flatMap((row) => row.calls)
    .filter((call) => call.command === "make_engine_ready").length,
  slowTranslateMs: describe(deadline.map((row) => row.requests[0].elapsedMs)),
  followingTranslateMs: describe(deadline.map((row) => row.requests[1].elapsedMs)),
};
await writeFile(join(directory, "summary.json"), JSON.stringify(result, null, 2) + "\n");
console.log(JSON.stringify(result, null, 2));
