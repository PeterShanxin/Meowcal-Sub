# ADR-0003: Incremental Lit frontend architecture

- Status: Accepted
- Date: 2026-08-02
- Decision owners: Meowcal Sub maintainers
- Related: #33, #34, ADR-0001

## Context

The main/setup frontend grew into large global scripts with direct DOM mutation and duplicated presentation state. The curated translation redesign needs explicit Home states, a four-step setup flow, progressive settings disclosure, and immediate overlay-preview updates. Extending the existing monolith would make those transitions harder to test and increase a tracked maintainability hotspot.

The application still has four independent Tauri webviews. The selector and live overlay contain working native-window and geometry behavior that should not be rewritten as part of the first usable redesign.

## Decision

Use an incremental frontend stack:

- Vite multi-page build output for the four existing Tauri webviews;
- TypeScript for new frontend contracts, controllers, and state transitions;
- Lit 3 custom elements for the main window and guided setup wizard;
- light-DOM rendering so shared design-token and accessibility styles remain centralized;
- one frontend bridge for Tauri/HTTP command transport;
- the existing selector and overlay scripts bundled as stable Vite entrypoints until #34 migrates their boundaries deliberately.

The Rust backend, Tauri command names, events, window labels, selector, and overlay behavior remain authoritative. Lit components own presentation and interaction dispatch; controllers own asynchronous UI state. Components do not infer engine readiness from ports, model IDs, or process names.

## Consequences

- New stateful main/setup UI is declarative and testable without a big-bang four-window rewrite.
- Vite and TypeScript become required development/build tools and part of the root verification contract.
- Normal-mode screens can remove infrastructure concepts while developer diagnostics remain explicitly separate.
- The selector and overlay temporarily coexist with Lit through separate build entries; this is an intentional migration boundary, not permission to duplicate their state machines.
- New `.ts` production files are covered by the repository maintainability ceiling.
- A later Svelte, React, or other framework migration requires a new ADR and evidence that it improves on this boundary.

## Rejected alternatives

### Continue adding direct DOM modules only

Rejected for the redesigned stateful surfaces. It would preserve the lowest dependency count but continue manual synchronization across a large `main.js` controller and multiple presentation states.

### Rewrite all four webviews in Svelte

Rejected for the MVP. It provides a clean compiled component model but would combine the product redesign with a risky rewrite of proven overlay and selector behavior.

### React or Preact migration

Rejected. Their ecosystems do not provide a material advantage for this compact multi-page desktop utility over incrementally adoptable standards-based custom elements.
