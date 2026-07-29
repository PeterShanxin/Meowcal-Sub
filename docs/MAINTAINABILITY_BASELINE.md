# Maintainability Baseline

This baseline converts known repository debt into enforceable, downward-only
ratchets. Machine-readable values live in
`config/maintainability-baseline.json`; this document owns their meaning.

## Enforced baseline

Measured on 2026-07-29 after the frontend foundation:

- 40 production files under `src/` and `src-tauri/src/`;
- 400 lines maximum for a new `.rs`, `.js`, `.html`, or `.css` production file;
- 20 explicit legacy files above that ceiling;
- 17 existing ESLint warnings, with zero allowed errors;
- frontend coverage floors of 77% statements, 69% branches, 77% functions, and
  77% lines.

Frontend coverage currently includes only:

- `src/scripts/backend-status.js`;
- `scripts/serve-frontend.mjs`.

The measured result is 77.04% statements, 69.38% branches, 77.77% functions,
and 77.04% lines. This is not a repository-wide coverage claim. Issue #35 owns
risk-based expansion after the decomposition lanes land.

## Legacy hotspot ceilings

Every production file over 400 lines is listed with its measured ceiling in the
machine-readable baseline. The largest and decomposition-owned hotspots are:

| File | Ceiling | Reduction owner |
|---|---:|---|
| `src-tauri/src/commands.rs` | 2,899 | #31 |
| `src/scripts/main.js` | 2,283 | #33 |
| `src-tauri/src/llm/foundry_local.rs` | 1,790 | #32 |
| `src-tauri/src/llm/manager.rs` | 1,314 | #32 |
| `src/scripts/overlay.js` | 1,293 | #34 |
| `src/scripts/selector.js` | 820 | #34 |
| `src-tauri/src/llm/context.rs` | 732 | #32 |
| `src-tauri/src/http_server.rs` | 732 | #31 adapter boundary |
| `src-tauri/src/config.rs` | 682 | #31 |
| `src/scripts/wizard.js` | 663 | #33 |

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

The verifier fails when a file crosses its ceiling, an exception points to a
missing file, a stale exception is at or below the new-file ceiling, lint
exceeds its warning budget, or measured coverage drops below a floor.

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
