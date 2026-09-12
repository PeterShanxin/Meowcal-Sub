#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const EPSILON_SECONDS = 0.25;

export function normalizeText(value) {
  const chars = [...String(value ?? '').normalize('NFKC').toLowerCase()];
  return chars.filter((char, index) => {
    if (/\p{L}|\p{N}/u.test(char)) return true;
    if (char !== '.' && char !== ',') return false;
    return /\p{N}/u.test(chars[index - 1] || '') && /\p{N}/u.test(chars[index + 1] || '');
  }).join('');
}

function readJson(pathname) {
  return JSON.parse(fs.readFileSync(pathname, 'utf8'));
}

export function readJsonLines(pathname) {
  return fs.readFileSync(pathname, 'utf8')
    .split(/\r?\n/)
    .filter(line => line.trim())
    .map((line, index) => {
      try {
        return JSON.parse(line);
      } catch (error) {
        throw new Error(`${pathname}:${index + 1}: invalid JSON: ${error.message}`);
      }
    });
}

function cueText(segment) {
  return normalizeText((segment.lines || [segment.text || '']).join(' '));
}

function segmentAt(scenario, seconds) {
  return (scenario.segments || []).find(segment => seconds >= segment.onset && seconds < segment.onset + segment.duration);
}

function isAdmitted(decision) {
  return decision?.admitted === true || decision?.admitted === 'true';
}

function percentile(values, percentileValue) {
  if (!values.length) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const index = (sorted.length - 1) * percentileValue;
  const lower = Math.floor(index);
  const upper = Math.ceil(index);
  if (lower === upper) return sorted[lower];
  return sorted[lower] + (sorted[upper] - sorted[lower]) * (index - lower);
}

function stateTimebase(state) {
  const timeOriginMs = state.timeOriginMs;
  const runStartedAtMs = state.runStartedAtMs;
  if (!Number.isFinite(timeOriginMs) || !Number.isFinite(runStartedAtMs)) return null;
  const startUtcMs = timeOriginMs + runStartedAtMs;
  return Number.isFinite(startUtcMs) ? { timeOriginMs, runStartedAtMs, startUtcMs } : null;
}

function frameUtcMs(frame) {
  return finiteNumber(frame.capture_utc_ms);
}

function frameText(frame) {
  return normalizeText((Array.isArray(frame.lines) ? frame.lines : [])
    .map(line => line?.text)
    .filter(text => typeof text === 'string')
    .join(' '));
}

function admittedText(frame) {
  if (!Array.isArray(frame.admitted_texts)) return null;
  return normalizeText(frame.admitted_texts.filter(text => typeof text === 'string').join(' '));
}

function finiteNumber(value) {
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function newCycleStats() {
  return { frames: 0, admittedFrames: 0, nonemptyFrames: 0, firstUtcMs: null, lastUtcMs: null, firstNonemptyUtcMs: null };
}

function translationEventList(input) {
  if (!input) return [];
  if (Array.isArray(input)) return input;
  if (Array.isArray(input.events)) return input.events;
  return [input];
}

function compareTranslations(events, scenario, startUtcMs) {
  const trueTranslatedCueIDs = new Set();
  const rejectedNonempty = [];
  const unmatched = [];
  const unmatchedNonempty = [];
  const latencySamples = [];
  let timingEvidencePartial = false;
  for (const event of events) {
    const receivedRaw = event.receivedAt ?? event.received_at ?? event.utc_ms;
    const receivedAt = finiteNumber(receivedRaw);
    const payload = event.payload || event;
    const original = payload.original ?? payload.source ?? payload.sourceText;
    const translated = payload.translated ?? payload.target ?? payload.targetText;
    const nonemptyPayload = normalizeText(original) !== '' || normalizeText(translated) !== '';
    const payloadTimestamp = finiteNumber(payload.timestamp);
    const totalMs = finiteNumber(payload.totalMs ?? payload.total_ms);
    const captureAt = payloadTimestamp !== null && totalMs !== null && totalMs >= 0 ? payloadTimestamp - totalMs : null;
    if (captureAt === null && nonemptyPayload) timingEvidencePartial = true;
    if (receivedAt === null && captureAt === null) {
      unmatched.push({ reason: 'missing_receivedAt', event });
      continue;
    }
    if (captureAt === null && nonemptyPayload) {
      const fallbackSegment = receivedAt === null ? null : segmentAt(scenario, (receivedAt - startUtcMs) / 1000);
      unmatched.push({ receivedAt, reason: 'missing_capture_time', original });
      if (fallbackSegment?.expected === 'subtitle') unmatchedNonempty.push({ receivedAt, reason: 'missing_capture_time', original });
      continue;
    }
    const matchAt = captureAt ?? receivedAt;
    const segment = matchAt === null ? null : segmentAt(scenario, (matchAt - startUtcMs) / 1000);
    const displayState = payload.displayState ?? payload.display_state;
    if (!segment || segment.expected !== 'subtitle' || normalizeText(original) !== cueText(segment)) {
      unmatched.push({ receivedAt, captureAt, reason: 'original_not_matched', original });
      if (nonemptyPayload && segment?.expected === 'subtitle') {
        unmatchedNonempty.push({ receivedAt, captureAt, reason: 'original_not_matched', original });
      }
      continue;
    }
    if (displayState === 'translated' && normalizeText(translated)) {
      trueTranslatedCueIDs.add(segment.id);
      if (totalMs !== null && totalMs >= 0) latencySamples.push(totalMs);
    } else if (normalizeText(translated)) {
      rejectedNonempty.push({ cueId: segment.id, receivedAt, displayState });
    }
  }
  return {
    eventCount: events.length,
    trueTranslatedCueIDs: [...trueTranslatedCueIDs],
    rejectedNonempty,
    unmatchedCount: unmatched.length,
    unmatchedNonemptyCount: unmatchedNonempty.length,
    timingEvidencePartial,
    latencyMs: { samples: latencySamples, p50: percentile(latencySamples, 0.5), p95: percentile(latencySamples, 0.95) },
  };
}

export function compareFixture({ scenario, fixtureState, gateFrames, translationEvents, translationProvided = translationEvents !== undefined }) {
  const warnings = [];
  const fatal = [];
  const frames = gateFrames.filter(frame => frame.kind === undefined || frame.kind === 'gate');
  const ignoredFrameCount = gateFrames.length - frames.length;
  const timebase = stateTimebase(fixtureState);
  if (!timebase) fatal.push('fixture-state.json must include numeric timeOriginMs and runStartedAtMs');
  if (timebase && Array.isArray(fixtureState.events) && fixtureState.events.some(event =>
    Math.abs((event.observedAtMs - timebase.runStartedAtMs) / 1000 - event.onset) > EPSILON_SECONDS,
  )) warnings.push('fixture onset timing drift exceeds 250ms; report is partial');
  if (!frames.length) fatal.push('gate log contains no gate frames');
  const subtitleSegments = (scenario.segments || []).filter(segment => segment.expected === 'subtitle');
  const negativeSegments = (scenario.segments || []).filter(segment => segment.expected === 'negative');
  const cueMetrics = new Map(subtitleSegments
    .map(segment => [segment.id, {
      cueId: segment.id,
      firstCorrectOcrUtcMs: null,
      firstCorrectAdmissionUtcMs: null,
      correctOcrFrames: 0,
      correctAdmissionFrames: 0,
    }]));
  const observedAnyAdmissionCueIDs = new Set();
  const gateAdmittedCueIDs = new Set();
  const correctlyAdmittedCueIDs = new Set();
  const missedOCRcueIDs = new Set(cueMetrics.keys());
  const negativeStats = new Map(negativeSegments
    .map(segment => [segment.id, {
      segmentId: segment.id,
      durationSeconds: segment.duration,
      warmupSeconds: segment.warmupSeconds || 0,
      firstOcrUtcMs: null,
      postWarmupFrames: 0,
      postWarmupAdmittedFrames: 0,
      nonemptyPostWarmupFrames: 0,
      cycles: {},
    }]));
  let admissionEvidenceMissing = false;
  let blankAdmittedFrameCount = 0;
  const relativeFrames = [];

  if (timebase) {
    for (const frame of frames) {
      const utcMs = frameUtcMs(frame);
      if (utcMs === null) {
        continue;
      }
      const seconds = (utcMs - timebase.startUtcMs) / 1000;
      if (seconds < 0 || seconds >= scenario.durationSeconds) continue;
      const cycle = Math.max(0, Math.floor(seconds / scenario.durationSeconds));
      const scenarioSeconds = seconds - cycle * scenario.durationSeconds;
      const segment = segmentAt(scenario, scenarioSeconds);
      const admitted = (Array.isArray(frame.decisions) ? frame.decisions : []).some(isAdmitted);
      const text = frameText(frame);
      const forwardedText = admittedText(frame);
      relativeFrames.push({ utcMs, seconds, scenarioSeconds, cycle, segmentId: segment?.id || null, admitted, text, forwardedText });
      if (!segment) continue;
      if (forwardedText === null) {
        admissionEvidenceMissing = true;
      }
      if (segment.expected === 'subtitle') {
        const metric = cueMetrics.get(segment.id);
        const correctOCR = text === cueText(segment);
        if (admitted) observedAnyAdmissionCueIDs.add(segment.id);
        if (correctOCR) {
          missedOCRcueIDs.delete(segment.id);
          metric.correctOcrFrames += 1;
          if (metric.firstCorrectOcrUtcMs === null) metric.firstCorrectOcrUtcMs = utcMs;
          if (admitted && forwardedText === cueText(segment)) {
            gateAdmittedCueIDs.add(segment.id);
            correctlyAdmittedCueIDs.add(segment.id);
            metric.correctAdmissionFrames += 1;
            if (metric.firstCorrectAdmissionUtcMs === null) metric.firstCorrectAdmissionUtcMs = utcMs;
          }
        }
      } else if (segment.expected === 'blank') {
        if (forwardedText) blankAdmittedFrameCount += 1;
      } else if (segment.expected === 'negative') {
        const stat = negativeStats.get(segment.id);
        if (!stat) continue;
        const cycleStats = stat.cycles[cycle] || newCycleStats();
        if (text && stat.firstOcrUtcMs === null) stat.firstOcrUtcMs = utcMs;
        if (text && cycleStats.firstNonemptyUtcMs === null) cycleStats.firstNonemptyUtcMs = utcMs;
        const firstObserved = cycleStats.firstNonemptyUtcMs === null
          ? utcMs
          : cycleStats.firstNonemptyUtcMs;
        const warmupEnd = (firstObserved - timebase.startUtcMs) / 1000 + (segment.warmupSeconds || 0);
        if (scenarioSeconds >= warmupEnd) {
          stat.postWarmupFrames += 1;
          cycleStats.frames += 1;
          if (text) {
            stat.nonemptyPostWarmupFrames += 1;
            cycleStats.nonemptyFrames += 1;
            cycleStats.firstNonemptyUtcMs ??= utcMs;
            cycleStats.firstUtcMs ??= utcMs;
            cycleStats.lastUtcMs = utcMs;
            if (admitted && forwardedText) {
              stat.postWarmupAdmittedFrames += 1;
              cycleStats.admittedFrames += 1;
            }
          }
        }
        stat.cycles[cycle] = cycleStats;
      }
    }
  }

  const gateDelayMs = [...cueMetrics.values()]
    .filter(metric => metric.firstCorrectOcrUtcMs !== null && metric.firstCorrectAdmissionUtcMs !== null)
    .map(metric => metric.firstCorrectAdmissionUtcMs - metric.firstCorrectOcrUtcMs);
  const negativePostWarmupAdmittedFrameCount = [...negativeStats.values()]
    .reduce((total, stat) => total + stat.postWarmupAdmittedFrames, 0);
  const observedNegativeCycles = new Set([0]);
  for (const stat of negativeStats.values()) {
    for (const cycle of Object.keys(stat.cycles)) observedNegativeCycles.add(Number(cycle));
  }
  const negativeCoverage = [];
  for (const stat of negativeStats.values()) {
    for (const cycle of [...observedNegativeCycles].sort((a, b) => a - b)) {
      const cycleStats = stat.cycles[cycle] || newCycleStats();
      const expectedSeconds = Math.max(0, stat.durationSeconds - stat.warmupSeconds);
      const spanSeconds = cycleStats.firstUtcMs === null ? 0 : Math.max(0, (cycleStats.lastUtcMs - cycleStats.firstUtcMs) / 1000);
      const coverageFraction = expectedSeconds === 0 ? 1 : spanSeconds / expectedSeconds;
      negativeCoverage.push({
        segmentId: stat.segmentId,
        cycle,
        expectedPostWarmupSeconds: expectedSeconds,
        observedPostWarmupFrames: cycleStats.nonemptyFrames,
        observedSpanSeconds: spanSeconds,
        coverageFraction,
        missing: cycleStats.nonemptyFrames === 0,
        sufficient: cycleStats.nonemptyFrames >= 2 && coverageFraction >= 0.5,
      });
    }
  }
  const negativeCoverageMissing = negativeCoverage.filter(entry => entry.missing);
  const negativeCoverageInsufficient = negativeCoverage.filter(entry => !entry.missing && !entry.sufficient);
  const negativeCoverageOk = negativeCoverageMissing.length === 0 && negativeCoverageInsufficient.length === 0;
  if (negativePostWarmupAdmittedFrameCount > 0) {
    warnings.push('negative content was admitted after its warmup window');
  }
  if (negativeCoverageMissing.length > 0) warnings.push('negative segment has no post-warmup gate coverage');
  if (negativeCoverageInsufficient.length > 0) warnings.push('negative segment gate coverage is too short to prove sustained exclusion');
  if (fixtureState.pauseCount > 0 || fixtureState.paused === true) warnings.push('fixture run was paused; report is partial');
  if (fixtureState.speed !== undefined && Number(fixtureState.speed) !== 1) warnings.push('fixture state was not recorded at 1x live speed');
  if (fixtureState.completed !== true) warnings.push('fixture-state.json does not prove a completed run with Repeat disabled');
  if (admissionEvidenceMissing) warnings.push('gate log is missing admitted_texts evidence for an authored segment');
  if (Number(fixtureState.elapsedSeconds) + EPSILON_SECONDS < scenario.durationSeconds) warnings.push('fixture elapsedSeconds is shorter than the authored scenario');
  if (!frames.every(frame => frameUtcMs(frame) !== null)) warnings.push('some gate frames have no UTC timestamp');
  if (relativeFrames.length && !relativeFrames.some(frame => frame.segmentId)) warnings.push('gate frames do not overlap an authored segment');
  if (frames.length && frames.every(frame => frameText(frame) === '')) warnings.push('gate log has no OCR text; cue identity cannot be proven');
  if (ignoredFrameCount > 0) warnings.push(`ignored ${ignoredFrameCount} non-gate log entries`);

  const missedAdmissionCueIDs = [...cueMetrics.values()]
    .filter(metric => metric.firstCorrectAdmissionUtcMs === null)
    .map(metric => metric.cueId);
  const translation = translationProvided && timebase
    ? compareTranslations(translationEvents || [], scenario, timebase.startUtcMs)
    : null;
  if (translation) {
    const authoredIDs = [...cueMetrics.keys()];
    translation.missedTranslationCueIDs = authoredIDs.filter(id => !translation.trueTranslatedCueIDs.includes(id));
    translation.ok = translation.missedTranslationCueIDs.length === 0 && translation.rejectedNonempty.length === 0
      && translation.unmatchedNonemptyCount === 0 && !translation.timingEvidencePartial;
    if (translation.timingEvidencePartial) warnings.push('translation timing evidence is partial; capture origin was not proven');
  }
  if (blankAdmittedFrameCount > 0) warnings.push('text was forwarded during an authored blank');
  const partial = warnings.some(warning => /paused|completed|shorter|1x live|UTC timestamp|timing drift|admitted_texts|translation timing/.test(warning));
  const gateOk = fatal.length === 0
    && !partial
    && missedOCRcueIDs.size === 0
    && missedAdmissionCueIDs.length === 0
    && negativePostWarmupAdmittedFrameCount === 0
    && blankAdmittedFrameCount === 0
    && negativeCoverageOk;
  const endToEndOk = translation ? gateOk && translation.ok : null;
  const ok = gateOk && (endToEndOk === null || endToEndOk);
  return {
    schemaVersion: 1,
    scenarioId: scenario.id,
    ok,
    gateOk,
    endToEndOk,
    partial,
    fatal,
    warnings,
    authored: {
      durationSeconds: scenario.durationSeconds,
      subtitleCueCount: cueMetrics.size,
      negativeSegmentCount: negativeStats.size,
      startUtcMs: timebase?.startUtcMs ?? null,
    },
    gate: {
      inputFrameCount: gateFrames.length,
      frameCount: frames.length,
      ignoredFrameCount,
      relativeFrameCount: relativeFrames.length,
      missedOCRcueIDs: [...missedOCRcueIDs],
      observedAnyAdmissionCueIDs: [...observedAnyAdmissionCueIDs],
      gateAdmittedCueIDs: [...gateAdmittedCueIDs],
      correctlyAdmittedCueIDs: [...correctlyAdmittedCueIDs],
      missedAdmissionCueIDs,
      gateDelayMs: {
        samples: gateDelayMs,
        p50: percentile(gateDelayMs, 0.5),
        p95: percentile(gateDelayMs, 0.95),
      },
      negativePostWarmup: [...negativeStats.values()],
      negativePostWarmupAdmittedFrameCount,
      blankAdmittedFrameCount,
      negativeCoverage,
      negativeCoverageMissing,
      negativeCoverageInsufficient,
      negativeCoverageOk,
    },
    translation,
  };
}

function usage() {
  console.error('Usage: node evals/band-gate/report.mjs gate-log.jsonl fixture-state.json [translation-events.json]');
}

async function main() {
  const [, , gatePath, statePath, translationPath] = process.argv;
  if (!gatePath || !statePath) {
    usage();
    process.exitCode = 2;
    return;
  }
  const scenarioData = readJson(path.join(HERE, 'scenarios.json'));
  const fixtureState = readJson(statePath);
  const scenario = scenarioData.scenarios.find(item => item.id === fixtureState.scenarioId);
  if (!scenario) throw new Error(`unknown scenario: ${fixtureState.scenarioId}`);
  const translationEvents = translationPath
    ? (translationPath.endsWith('.jsonl') ? readJsonLines(translationPath) : translationEventList(readJson(translationPath)))
    : undefined;
  const result = compareFixture({ scenario, fixtureState, gateFrames: readJsonLines(gatePath), translationEvents });
  console.log(JSON.stringify(result, null, 2));
  process.exitCode = result.ok ? 0 : 1;
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  main().catch(error => {
    console.error(error.message);
    process.exitCode = 2;
  });
}
