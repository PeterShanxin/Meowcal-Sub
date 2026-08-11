# Maintainability Baseline

This baseline converts known repository debt into enforceable, downward-only
ratchets. Machine-readable values live in
`config/maintainability-baseline.json`; this document owns their meaning.

## Enforced baseline

Measured on 2026-08-10 after the #32 engine-status orchestration extraction
(following the #31 command-boundary and lifecycle waves):

- production files under `src/` and `src-tauri/src/`;
- 400 lines maximum for a new `.rs`, `.js`, `.html`, or `.css` production file;
- 15 explicit legacy files above that ceiling;
- 10 existing ESLint warnings, with zero allowed errors;
- frontend coverage floors of 89% statements, 78% branches, 92% functions, and
  88% lines.

Frontend coverage currently includes only:

- `src/scripts/backend-status.js`;
- `src/scripts/ocr-language-tags.js`;
- `src/scripts/pipeline-update.js`;
- `src/scripts/translation-display.js`;
- `src/scripts/wizard-state.js`;
- `scripts/serve-frontend.mjs`.

The measured result is 90.85% statements, 82.68% branches, 93.33% functions,
and 90.41% lines, all above the recorded floors. This is not a repository-wide
coverage claim. Issue #35 owns risk-based expansion after the decomposition
lanes land, and owns raising these floors to the measured values: doing it from
an unrelated lane would tighten the gate under every other in-flight branch.

## Legacy hotspot ceilings

Every production file over 400 lines is listed with its measured ceiling in the
machine-readable baseline, which is authoritative. The largest and
decomposition-owned hotspots are:

| File                                 | Ceiling | Reduction owner      |
| ------------------------------------ | ------: | -------------------- |
| `src/scripts/main.js`                |   1,987 | #33                  |
| `src-tauri/src/llm/foundry_local.rs` |   1,700 | #32                  |
| `src-tauri/src/commands.rs`          |   1,209 | #31 / #32 surface    |
| `src/scripts/overlay.js`             |   1,129 | #34                  |
| `src-tauri/src/llm/manager.rs`       |   1,021 | #32                  |
| `src/scripts/selector.js`            |     820 | #34                  |
| `src-tauri/src/llm/context.rs`       |     732 | #32                  |
| `src-tauri/src/config.rs`            |     595 | #31                  |

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
- Coverage minimums: never decrease. Improved measured coverage should raise
  the relevant floor without claiming coverage outside the configured include
  list.

The verifier compares the edited baseline with the committed baseline (or the
checked-out commit with its parent in CI). It rejects raised file/warning
ceilings, lowered coverage floors, and new legacy exceptions. It also requires
each legacy ceiling to equal the measured file length, so shrinkage cannot
leave hidden regrowth headroom.

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
