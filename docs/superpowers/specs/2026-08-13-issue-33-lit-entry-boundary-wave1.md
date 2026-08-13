# Issue #33 Wave 1: Lit main/setup entry boundary

Status: selected for implementation
Starting main: `7bbb4432577dd10c00c6e93db0ce6824c3e1ace8`

## Audit decision

The active main/setup frontend is already Vite + TypeScript + Lit:

```text
index.html -> entries/main.ts -> TauriBridge + OcrLanguageTags + meowcal-app
wizard.html -> entries/wizard.ts -> TauriBridge + OcrLanguageTags + meowcal-setup
```

`src/scripts/main.js` is not loaded by either HTML entry, is absent from the
Vite entry graph and production bundle, and is superseded by
`AppController`, `home-state.ts`, Lit views, and `MeowcalSetup`. Its global
helpers (`BackendStatusPresentation`, `TranslationStart`, and the old settings
and wizard scripts) have no active runtime consumer. Keeping them creates a
second apparent owner, stale lint debt, and a false maintainability target.

This wave establishes the already-live Lit graph as the sole main/setup
frontend entry. It does not invent another controller or split code by line
count.

## Scope

- delete the dead legacy main/setup scripts and their test-only helper tests;
- remove the unused wizard-state entry import;
- remove deleted files from formatter, lint, coverage, and maintainability
  metadata;
- update stale source comments that describe `main.js` as the active frontend;
- remove the `main.js` legacy exception and update the measured baseline prose;
- add source/entry/build characterization proving the active Vite/Lit graph.

Deleted legacy files:

- `src/scripts/main.js`
- `src/scripts/backend-status.js`
- `src/scripts/translation-start.js`
- `src/scripts/settings.js`
- `src/scripts/wizard.js`
- `src/scripts/wizard-state.js`
- their dedicated `frontend-tests/unit` files

## Ownership after the wave

- `AppController` owns the main-window asynchronous UI snapshot, settings
  auto-persistence adapter, session controls, region polling, and main-window
  bridge calls.
- `home-state.ts` owns deterministic main-window readiness presentation.
- `MeowcalSetup` owns setup-window presentation/application state and setup
  bridge calls; Rust remains the engine lifecycle/readiness source of truth.
- `TauriBridge` remains the only frontend transport adapter.
- `OcrLanguageTags` remains the shared OCR language compatibility helper.
- Lit components/views own DOM rendering and intent handlers.
- No timer/listener ownership changes in active modules.
- Overlay and selector entries remain untouched and remain #34 territory.

## Non-goals

- no settings semantics or Save-button workflow;
- no engine/pipeline/backend state-machine changes;
- no new readiness state machine;
- no `AppController` or `MeowcalSetup` extraction in this wave;
- no overlay/selector edits;
- no UI copy, accessibility, i18n, opacity, OCR quality, translation quality,
  startup stability, or renderer-wedge changes;
- no merge, issue closure, or #34/#35 work.

## Verification contract

1. prove no active HTML/entry imports the deleted legacy scripts or globals;
2. prove the built main and wizard entries contain only the active Lit graph;
3. preserve the existing auto-persist, readiness, setup, and browser bridge
   characterization paths;
4. remove the `main.js` exception and make the maintainability check fail if a
   stale exception or deleted production file is reintroduced;
5. run the repository frontend gates and the authoritative `verify.ps1 -Stage All`;
6. defer the native setup/start/stop/settings gate to the final #33 closeout,
   unless the structural change unexpectedly alters visible behavior.
