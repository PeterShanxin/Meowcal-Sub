import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';
import { compareFixture, normalizeText } from './report.mjs';

const data = JSON.parse(fs.readFileSync(new URL('./scenarios.json', import.meta.url), 'utf8'));
const equalScenario = data.scenarios.find(scenario => scenario.id === 'equal-width-120s');
const negativeScenario = data.scenarios.find(scenario => scenario.id === 'negative-band-recovery-120s');
const timeOriginMs = 1_700_000_000_000;
test('the offline page and report use the same authored timeline', () => {
  const html = fs.readFileSync(new URL('./index.html', import.meta.url), 'utf8');
  const embedded = html.match(/<script type="application\/json" id="scenario-data">([\s\S]*?)<\/script>/);
  assert.ok(embedded, 'the offline page must embed its scenario data');
  assert.deepEqual(JSON.parse(embedded[1]), data);
});

const state = scenario => ({
  scenarioId: scenario.id,
  timeOriginMs,
  runStartedAtMs: 0,
  runStartedAtUtcMs: timeOriginMs,
  elapsedSeconds: scenario.durationSeconds,
  durationSeconds: scenario.durationSeconds,
  speed: 1,
  pauseCount: 0,
  paused: false,
  completed: true,
});
const frame = (seconds, text, admitted = true, extra = {}) => ({
  utc_ms: timeOriginMs + seconds * 1000,
  lines: [{ text, x: 120, y: 560, w: 720, h: 48 }],
  admitted_texts: admitted && text ? [text] : [],
  decisions: [{ cue_id: extra.cueId || 'native-band', admitted }],
});

function oneFramePerCue(scenario, omitId = null) {
  return scenario.segments
    .filter(segment => segment.expected === 'subtitle' && segment.id !== omitId)
    .map(segment => frame(segment.onset + 1, segment.lines.join(' '), true));
}

test('a cue with no correct OCR frame is reported as missed', () => {
  const result = compareFixture({
    scenario: equalScenario,
    fixtureState: state(equalScenario),
    gateFrames: [...oneFramePerCue(equalScenario, 'ew-05'), frame(17, 'wrong source text', true)],
  });
  assert.deepEqual(result.gate.missedOCRcueIDs, ['ew-05']);
  assert.ok(result.gate.missedAdmissionCueIDs.includes('ew-05'));
  assert.ok(!result.gate.gateAdmittedCueIDs.includes('ew-05'), 'wrong OCR must not count as admission');
  assert.equal(result.ok, false);
});

test('a stalled fixture clock cannot claim a passing native run', () => {
  const result = compareFixture({
    scenario: equalScenario,
    fixtureState: { ...state(equalScenario), events: [{ onset: 4, observedAtMs: 11_000 }] },
    gateFrames: oneFramePerCue(equalScenario),
  });
  assert.equal(result.partial, true);
  assert.equal(result.ok, false);
});

test('wrong OCR cannot establish a cue or distort gate delay p95', () => {
  const gateFrames = [];
  for (const segment of equalScenario.segments.filter(item => item.expected === 'subtitle')) {
    gateFrames.push(frame(segment.onset + 0.5, 'wrong source text', true));
    gateFrames.push(frame(segment.onset + 1, segment.lines.join(' '), false));
    gateFrames.push(frame(segment.onset + 2, segment.lines.join(' '), true));
  }
  const result = compareFixture({ scenario: equalScenario, fixtureState: state(equalScenario), gateFrames });
  assert.equal(result.gate.missedOCRcueIDs.length, 0);
  assert.equal(result.gate.missedAdmissionCueIDs.length, 0);
  assert.equal(result.gate.gateDelayMs.p95, 1000);
  assert.equal(result.gate.negativePostWarmupAdmittedFrameCount, 0);
  assert.equal(normalizeText('The harbor lights are fading.'), normalizeText('THE HARBOR LIGHTS ARE FADING'));
  assert.notEqual(normalizeText('Room 1.3'), normalizeText('Room 13'));
  assert.equal(normalizeText('Wait, here.'), normalizeText('WAIT HERE'));
});

test('partial forwarded lines cannot admit a multi-line cue', () => {
  const cue = equalScenario.segments.find(segment => segment.id === 'ew-05');
  const gateFrames = [
    ...oneFramePerCue(equalScenario, cue.id),
    {
      ...frame(cue.onset + 2, cue.lines.join(' '), true),
      admitted_texts: [cue.lines[0]],
    },
  ];
  const result = compareFixture({ scenario: equalScenario, fixtureState: state(equalScenario), gateFrames });
  assert.ok(result.gate.missedAdmissionCueIDs.includes(cue.id));
  assert.equal(result.gate.gateDelayMs.samples.length, equalScenario.segments.length - 1);
  assert.equal(result.ok, false);
});

test('legacy gate logs without forwarded text evidence are explicitly partial', () => {
  const result = compareFixture({
    scenario: equalScenario,
    fixtureState: state(equalScenario),
    gateFrames: oneFramePerCue(equalScenario).map(({ admitted_texts, ...legacy }) => legacy),
  });
  assert.equal(result.partial, true);
  assert.equal(result.ok, false);
  assert.ok(result.warnings.some(warning => warning.includes('admitted_texts')));
});

test('a partial run reports post-warmup negative admission and cannot pass', () => {
  const fixtureState = { ...state(negativeScenario), completed: false, elapsedSeconds: 52, paused: true, pauseCount: 1 };
  const result = compareFixture({
    scenario: negativeScenario,
    fixtureState,
    gateFrames: [
      frame(0, 'MEOWCAL LAB / DEMO', false),
      frame(7, 'MEOWCAL LAB / DEMO', false),
      frame(40, '00:40', false),
      frame(46, '00:46', true),
    ],
  });
  assert.equal(result.partial, true);
  assert.equal(result.ok, false);
  assert.equal(result.gate.negativePostWarmupAdmittedFrameCount, 1);
  assert.ok(result.gate.negativeCoverageMissing.some(entry => entry.segmentId === 'nb-credits-band'));
  assert.ok(result.gate.negativeCoverageInsufficient.some(entry => entry.segmentId === 'nb-timer-band'));
  assert.equal(result.gate.negativeCoverageOk, false);
  assert.ok(result.warnings.some(warning => warning.includes('paused')));
});

test('an empty OCR frame after the first negative read cannot prove coverage', () => {
  const result = compareFixture({
    scenario: negativeScenario,
    fixtureState: { ...state(negativeScenario), completed: false, elapsedSeconds: 8 },
    gateFrames: [
      frame(0, 'MEOWCAL LAB / DEMO', false),
      frame(7, '', false),
      frame(39, '', false),
    ],
  });
  const watermark = result.gate.negativeCoverage.find(entry => entry.segmentId === 'nb-watermark-band' && entry.cycle === 0);
  assert.equal(watermark.observedPostWarmupFrames, 0);
  assert.ok(result.gate.negativeCoverageMissing.some(entry => entry.segmentId === 'nb-watermark-band'));
  assert.equal(result.gate.negativeCoverageOk, false);
});

test('post-completion frames cannot fabricate another negative cycle', () => {
  const gateFrames = oneFramePerCue(negativeScenario);
  for (const segment of negativeScenario.segments.filter(item => item.expected === 'negative')) {
    for (const offset of [0, segment.warmupSeconds + 1, segment.duration - 1]) {
      gateFrames.push(frame(segment.onset + offset, segment.text || 'negative text', false));
    }
  }
  gateFrames.push(frame(negativeScenario.durationSeconds + 0.1, 'last dialogue', true));
  const result = compareFixture({ scenario: negativeScenario, fixtureState: state(negativeScenario), gateFrames });
  assert.equal(result.gate.negativeCoverage.length, 3);
  assert.equal(result.gate.negativeCoverageOk, true);
  assert.equal(result.ok, true);
});


test('interleaved non-gate log entries are ignored and incomplete translation fails end to end', () => {
  const frames = [
    { kind: 'ocr', ms: 1, lines: [{ text: 'diagnostic-only' }] },
    ...oneFramePerCue(equalScenario),
    { kind: 'decision', ms: 2, lines: [{ text: 'diagnostic-only' }] },
  ];
  const first = equalScenario.segments[0];
  const result = compareFixture({
    scenario: equalScenario,
    fixtureState: state(equalScenario),
    gateFrames: frames,
    translationEvents: [{
      receivedAt: timeOriginMs + 1_000,
      payload: { original: first.lines[0], translated: 'We should leave before sunrise.', displayState: 'translated', timestamp: timeOriginMs + 1_000, totalMs: 1_000 },
    }],
  });
  assert.equal(result.gate.inputFrameCount, equalScenario.segments.length + 2);
  assert.equal(result.gate.frameCount, equalScenario.segments.length);
  assert.equal(result.gate.ignoredFrameCount, 2);
  assert.equal(result.gateOk, true);
  assert.equal(result.endToEndOk, false);
  assert.equal(result.ok, false);
  assert.equal(result.translation.trueTranslatedCueIDs.length, 1);
  assert.ok(result.translation.missedTranslationCueIDs.includes('ew-30'));
});

test('late translation completion is assigned using capture timestamp', () => {
  const first = equalScenario.segments[0];
  const next = equalScenario.segments[1];
  const receivedAt = timeOriginMs + (next.onset + 0.2) * 1000;
  const result = compareFixture({
    scenario: equalScenario,
    fixtureState: state(equalScenario),
    gateFrames: oneFramePerCue(equalScenario),
    translationEvents: [{
      receivedAt,
      payload: {
        original: first.lines[0],
        translated: 'translation',
        displayState: 'translated',
        timestamp: receivedAt,
        totalMs: 4_000,
      },
    }],
  });
  assert.ok(result.translation.trueTranslatedCueIDs.includes(first.id));
  assert.ok(!result.translation.trueTranslatedCueIDs.includes(next.id));
  assert.equal(result.translation.unmatchedNonemptyCount, 0);
  assert.deepEqual(result.translation.latencyMs.samples, [4_000]);
});

test('malformed translation timing is reported as partial instead of reassigned', () => {
  const first = equalScenario.segments[0];
  const result = compareFixture({
    scenario: equalScenario,
    fixtureState: state(equalScenario),
    gateFrames: oneFramePerCue(equalScenario),
    translationEvents: [{
      receivedAt: timeOriginMs + 4_200,
      payload: {
        original: first.lines[0],
        translated: 'translation',
        displayState: 'translated',
        timestamp: 'invalid',
        totalMs: 4_500,
      },
    }],
  });
  assert.equal(result.translation.timingEvidencePartial, true);
  assert.equal(result.translation.unmatchedNonemptyCount, 1);
  assert.equal(result.endToEndOk, false);
});

test('boolean capture timestamps are rejected instead of coerced', () => {
  const first = equalScenario.segments[0];
  const result = compareFixture({
    scenario: equalScenario,
    fixtureState: state(equalScenario),
    gateFrames: oneFramePerCue(equalScenario),
    translationEvents: [{
      receivedAt: timeOriginMs + 4_200,
      payload: {
        original: first.lines[0],
        translated: 'translation',
        displayState: 'translated',
        timestamp: true,
        totalMs: 4_500,
      },
    }],
  });
  assert.equal(result.translation.timingEvidencePartial, true);
  assert.equal(result.translation.unmatchedNonemptyCount, 1);
  assert.equal(result.translation.latencyMs.samples.length, 0);
  assert.equal(result.endToEndOk, false);
});

test('end-to-end status cannot pass when the OCR gate fails', () => {
  const translations = negativeScenario.segments
    .filter(segment => segment.expected === 'subtitle')
    .map(segment => ({
      receivedAt: timeOriginMs + (segment.onset + 1) * 1000,
      payload: { original: segment.lines[0], translated: 'translated', displayState: 'translated', timestamp: timeOriginMs + (segment.onset + 1) * 1000, totalMs: 0 },
    }));
  const result = compareFixture({
    scenario: negativeScenario,
    fixtureState: { ...state(negativeScenario), completed: false, elapsedSeconds: 52 },
    gateFrames: [frame(46, '00:46', false)],
    translationEvents: translations,
  });
  assert.equal(result.translation.ok, true);
  assert.equal(result.gateOk, false);
  assert.equal(result.endToEndOk, false);
  assert.equal(result.ok, false);
});
