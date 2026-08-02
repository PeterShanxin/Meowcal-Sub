# Maintainability Baseline

This baseline converts known repository debt into enforceable, downward-only
ratchets. Machine-readable values live in
`config/maintainability-baseline.json`; this document owns their meaning.

## Enforced baseline

Measured on 2026-08-02 after the OCR thresholding rewrite milestone:

- 99 production files under `src/` and `src-tauri/src/`;
- 400 lines maximum for a new `.rs`, `.js`, `.html`, or `.css` production file;
- 17 explicit legacy files above that ceiling;
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

The measured result is 89.11% statements, 78.76% branches, 92.59% functions,
and 88.48% lines. This is not a repository-wide coverage claim. Issue #35 owns
risk-based expansion after the decomposition lanes land.

## Legacy hotspot ceilings

Every production file over 400 lines is listed with its measured ceiling in the
machine-readable baseline. The largest and decomposition-owned hotspots are:

| File                                 | Ceiling | Reduction owner      |
| ------------------------------------ | ------: | -------------------- |
| `src-tauri/src/commands.rs`          |   2,254 | #31                  |
| `src/scripts/main.js`                |   1,987 | #33                  |
| `src-tauri/src/llm/foundry_local.rs` |   1,720 | #32                  |
| `src-tauri/src/llm/manager.rs`       |   1,234 | #32                  |
| `src/scripts/overlay.js`             |   1,131 | #34                  |
| `src/scripts/selector.js`            |     820 | #34                  |
| `src-tauri/src/llm/context.rs`       |     732 | #32                  |
| `src-tauri/src/http_server.rs`       |     727 | #31 adapter boundary |
| `src-tauri/src/config.rs`            |     625 | #31                  |
| `src-tauri/src/main.rs`              |     578 | #31                  |

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
