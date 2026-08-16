# Maintainability Baseline

This baseline converts known repository debt into enforceable, downward-only
ratchets. Machine-readable values live in
`config/maintainability-baseline.json`; this document owns their meaning.

## Enforced baseline

Measured on 2026-08-16, after #35 closed the decomposition programme by widening
the measured coverage scope (#31 lifecycle, #32 engine/pipeline, #33 main/setup,
#34 selector/overlay):

- production files under `src/` and `src-tauri/src/`;
- 400 lines maximum for a new `.rs`, `.js`, `.ts`, `.html`, or `.css` production file;
- 14 explicit legacy files above that ceiling;
- 8 existing ESLint warnings, with zero allowed errors;
- frontend coverage floors of 80% statements, 74% branches, 81% functions, and
  83% lines, over a named 30-module scope.

## Frontend coverage scope

`frontendCoverageScope` in the machine-readable baseline lists, file by file,
what the coverage percentage is a percentage _of_. It is the denominator, so it
lives next to the floors rather than in the test runner's config, and the ratchet
enforces two rules over it:

- **the scope may grow but never shrink** — dropping a module raises the
  percentage without a line of new test code, which is the cheapest way there is
  to make a coverage claim mean less than it says;
- **a floor may only fall in a change that widens the scope** — widening pulls in
  code the old floors never described, so the number can legitimately drop while
  the claim gets stronger. Any other decrease is still rejected.

The 30 modules are the risk areas the decomposition lanes produced: the overlay
and selector geometry, appearance, timer, and payload rules from #33/#34; the
main and setup state and controllers from #33; and the repository's own gates,
because a gate that is wrong fails open.

Deliberately outside the scope, and why:

| Not measured                                                              | Reason                                                  |
| ------------------------------------------------------------------------- | ------------------------------------------------------- |
| `src/scripts/overlay.js`, `selector.js`, `tauri-bridge.js`                | DOM and Tauri adapters; the unit environment is `node`  |
| `src/ui/*-view.ts`, `meowcal-app.ts`, `meowcal-setup.ts`, `src/entries/*` | Lit components and entries; need a DOM                  |
| `src/ui/contracts.ts`                                                     | types only — nothing survives compilation to measure    |
| all Rust                                                                  | no coverage instrumentation on the toolchain; see below |

Those adapters are covered by the browser smoke and the manual Windows gate
instead. Counting them here would need a jsdom shim that proves less than either.

`frontend-tests/unit/coverage-scope.test.mjs` keeps the list honest from the
other side: a module that a unit test exercises but the scope omits fails the
suite, so coverage cannot be kept high by leaving risky code out of the
measurement. Its exclusions are an explicit list with a reason per entry.

The measured result over that scope is **80.58% statements, 74.65% branches,
81.09% functions, and 83.47% lines** (826/1025, 592/793, 163/201, 783/938),
and the floors are those numbers rounded down to whole percent.

**This is not a repository-wide coverage claim.** It is a claim about 30 named
modules, and the number is not comparable to the 90.17% recorded before #35: the
previous figure measured nine files, this one measures thirty. The percentage
fell because the scope grew, which is the trade #35 chose deliberately - a lower
number that describes the risky code beats a higher one that leaves it out.

## Rust coverage

No line-coverage instrumentation is installed on the toolchain, and #35
deliberately did not add one: it would become a documented prerequisite for every
clean checkout, in exchange for a number nothing gates on. What the gate does
enforce is that every Rust test target actually runs — `--lib`, and each file
under `src-tauri/tests/`. `scripts/tests/verify.Tests.ps1` fails when an
integration target exists that `verify.ps1` never invokes, so a new test cannot be
invisible to the gate. That is the risk the "instrumentation" question is really
about here.

## Legacy hotspot ceilings

Every production file over 400 lines is listed with its measured ceiling in the
machine-readable baseline, which is authoritative. The largest and
decomposition-owned hotspots are:

| File                                 | Ceiling | Reduction owner   |
| ------------------------------------ | ------: | ----------------- |
| `src-tauri/src/llm/foundry_local.rs` |   1,561 | #32               |
| `src-tauri/src/commands.rs`          |   1,090 | #31 / #32 surface |
| `src/scripts/overlay.js`             |   1,062 | #34               |
| `src-tauri/src/llm/context.rs`       |     732 | #32               |
| `src-tauri/src/llm/manager.rs`       |     726 | #32               |
| `src/scripts/selector.js`            |     721 | #34               |
| `src-tauri/src/config.rs`            |     595 | #31               |

`main.rs` left the legacy list in the #31 lifecycle wave. `http_server.rs`
left it in the #32 engine-status wave (now under the 400-line new-file
ceiling after status orchestration moved to `engine_status`).

The remaining explicit exceptions are styles, HTML, and focused platform
modules recorded in the JSON manifest. They have the same no-growth rule even
when no dedicated extraction issue is assigned.

## Ratchet semantics

- New production file ceiling: may decrease; an increase requires explicit
  architectural justification and maintainer review.
- Legacy file ceiling: never increases. When a file shrinks, lower its ceiling
  in the same pull request. Remove its exception once it reaches 400 lines.
- ESLint maximum: never increases. Fixing a legacy warning lowers
  `eslintMaxWarnings` in the same pull request.
- Coverage scope: may grow, never shrink. A module leaves the scope only when
  the file itself is gone.
- Coverage minimums: never decrease, except in a change that widens the scope -
  the one case where the number describes something larger than it did. Improved
  measured coverage should raise the relevant floor.

The verifier compares the edited baseline with the committed baseline (or the
checked-out commit with its parent in CI). It rejects raised file/warning
ceilings, lowered coverage floors outside a scope widening, modules dropped from
the coverage scope, and new legacy exceptions. It also requires each legacy
ceiling to equal the measured file length, so shrinkage cannot leave hidden
regrowth headroom.

## Updating the baseline

Any baseline change is part of the production pull request that caused it. The
pull request records:

1. before and after measurements;
2. why ownership or scope changed;
3. the exact manifest values changed;
4. a negative test proving the affected gate can fail;
5. any manual Windows regression required by visible behavior.

Do not use a follow-up cleanup pull request to restore a ratchet after merging
growth. Temporary local edits used for negative proof are removed before
commit.

## Verification

Run:

```powershell
.\scripts\verify.ps1
```

For focused work, `npm run maintainability`, `npm run lint`, and
`npm run test:frontend` exercise line ceilings, the warning budget, and the
configured frontend coverage floors respectively. The root command remains the
handoff gate because it also proves stage ordering, Rust checks, browser bridge
behavior, and dependency audit.
