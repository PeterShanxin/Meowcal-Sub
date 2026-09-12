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
  return scenario.segments.find(segment =>
    seconds >= segment.onset && seconds < segment.onset + segment.duration,
  );
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
  const raw = frame.utc_ms ?? frame.utcMs;
  if (raw === null || raw === undefined || raw === '') return null;
  const value = Number(raw);
  return Number.isFinite(value) ? value : null;
}

function frameText(frame) {
  return normalizeText((Array.isArray(frame.lines) ? frame.lines : [])
    .map(line => line?.text)
    .filter(text => typeof text === 'string')
    .join(' '));
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
  for (const event of events) {
    const receivedRaw = event.receivedAt ?? event.received_at ?? event.utc_ms;
    const receivedAt = receivedRaw === null || receivedRaw === undefined || receivedRaw === '' ? NaN : Number(receivedRaw);
    const payload = event.payload || event;
    if (!Number.isFinite(receivedAt)) {
      unmatched.push({ reason: 'missing_receivedAt', event });
      continue;
    }
    const segment = segmentAt(scenario, (receivedAt - startUtcMs) / 1000);
    const original = payload.original ?? payload.source ?? payload.sourceText;
    const translated = payload.translated ?? payload.target ?? payload.targetText;
    const displayState = payload.displayState ?? payload.display_state;
    if (!segment || segment.expected !== 'subtitle' || normalizeText(original) !== cueText(segment)) {
      unmatched.push({ receivedAt, reason: 'original_not_matched', original });
      continue;
    }
    if (displayState === 'translated' && normalizeText(translated)) {
      trueTranslatedCueIDs.add(segment.id);
    } else if (normalizeText(translated)) {
      rejectedNonempty.push({ cueId: segment.id, receivedAt, displayState });
    }
  }
  return {
    eventCount: events.length,
    trueTranslatedCueIDs: [...trueTranslatedCueIDs],
    rejectedNonempty,
    unmatchedCount: unmatched.length,
  };
}

export function compareFixture({ scenario, fixtureState, gateFrames, translationEvents, translationProvided = translationEvents !== undefined }) {
  const warnings = [];
  const fatal = [];
  const frames = gateFrames.filter(frame => frame.kind === undefined || frame.kind === 'gate');
  const ignoredFrameCount = gateFrames.length - frames.length;
  const timebase = stateTimebase(fixtureState);
  if (!timebase) fatal.push('fixture-state.json must include numeric timeOriginMs and runStartedAtMs');
  if (!frames.length) fatal.push('gate log contains no gate frames');
  const cueMetrics = new Map(scenario.segments
    .filter(segment => segment.expected === 'subtitle')
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
  const negativeStats = new Map(scenario.segments
    .filter(segment => segment.expected === 'negative')
    .map(segment => [segment.id, {
      segmentId: segment.id,
      durationSeconds: segment.duration,
      warmupSeconds: segment.warmupSeconds || 0,
      postWarmupFrames: 0,
      postWarmupAdmittedFrames: 0,
      cycles: {},
    }]));
  const relativeFrames = [];

  if (timebase) {
    for (const frame of frames) {
      const utcMs = frameUtcMs(frame);
      if (utcMs === null) {
        warnings.push('gate frame missing utc_ms; frame ignored');
        continue;
      }
      const seconds = (utcMs - timebase.startUtcMs) / 1000;
      if (seconds < -EPSILON_SECONDS || seconds > scenario.durationSeconds + EPSILON_SECONDS) continue;
      const cycle = Math.max(0, Math.floor(seconds / scenario.durationSeconds));
      const scenarioSeconds = seconds - cycle * scenario.durationSeconds;
      const segment = segmentAt(scenario, scenarioSeconds);
      const admitted = (Array.isArray(frame.decisions) ? frame.decisions : []).some(isAdmitted);
      const text = frameText(frame);
      relativeFrames.push({ utcMs, seconds, scenarioSeconds, cycle, segmentId: segment?.id || null, admitted, text });
      if (!segment) continue;
      if (segment.expected === 'subtitle') {
        const metric = cueMetrics.get(segment.id);
        const correctOCR = text === cueText(segment);
        if (admitted) observedAnyAdmissionCueIDs.add(segment.id);
        if (correctOCR) {
          missedOCRcueIDs.delete(segment.id);
          metric.correctOcrFrames += 1;
          if (metric.firstCorrectOcrUtcMs === null) metric.firstCorrectOcrUtcMs = utcMs;
          if (admitted) {
            gateAdmittedCueIDs.add(segment.id);
            correctlyAdmittedCueIDs.add(segment.id);
            metric.correctAdmissionFrames += 1;
            if (metric.firstCorrectAdmissionUtcMs === null) metric.firstCorrectAdmissionUtcMs = utcMs;
          }
        }
      } else if (segment.expected === 'negative') {
        const stat = negativeStats.get(segment.id);
        const warmupEnd = segment.onset + (segment.warmupSeconds || 0);
        if (scenarioSeconds >= warmupEnd) {
          stat.postWarmupFrames += 1;
          if (admitted) stat.postWarmupAdmittedFrames += 1;
          const cycleStats = stat.cycles[cycle] || { frames: 0, admittedFrames: 0, firstUtcMs: null, lastUtcMs: null };
          cycleStats.frames += 1;
          cycleStats.firstUtcMs ??= utcMs;
          cycleStats.lastUtcMs = utcMs;
          if (admitted) cycleStats.admittedFrames += 1;
          stat.cycles[cycle] = cycleStats;
        }
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
      const cycleStats = stat.cycles[cycle] || { frames: 0, admittedFrames: 0, firstUtcMs: null, lastUtcMs: null };
      const expectedSeconds = Math.max(0, stat.durationSeconds - stat.warmupSeconds);
      const spanSeconds = cycleStats.firstUtcMs === null ? 0 : Math.max(0, (cycleStats.lastUtcMs - cycleStats.firstUtcMs) / 1000);
      const coverageFraction = expectedSeconds === 0 ? 1 : spanSeconds / expectedSeconds;
      negativeCoverage.push({
        segmentId: stat.segmentId,
        cycle,
        expectedPostWarmupSeconds: expectedSeconds,
        observedPostWarmupFrames: cycleStats.frames,
        observedSpanSeconds: spanSeconds,
        coverageFraction,
        missing: cycleStats.frames === 0,
        sufficient: cycleStats.frames >= 2 && coverageFraction >= 0.5,
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
    translation.ok = translation.missedTranslationCueIDs.length === 0 && translation.rejectedNonempty.length === 0;
  }
  const partial = warnings.some(warning => /paused|completed|shorter|1x live|UTC timestamp/.test(warning));
  const gateOk = fatal.length === 0
    && !partial
    && missedOCRcueIDs.size === 0
    && missedAdmissionCueIDs.length === 0
    && negativePostWarmupAdmittedFrameCount === 0
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
