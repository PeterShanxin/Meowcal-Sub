# OCR Binarization & Default Settings Design

**Date**: 2026-03-01
**Branch**: feature/ocr-noise-improvement
**Status**: Approved

---

## Background

Translumo's `ImageHelper.cs` applies a binary threshold (value 150) after grayscale conversion as its primary noise-reduction step. Our app already has grayscale and histogram equalization. This design adds binarization as a third pipeline step and audits all OCR-related defaults for consistency.

Morphological operations (Translumo's 1×1 `MorphOpen`) are intentionally excluded — a 1×1 kernel is a no-op and provides no real benefit.

Multi-engine voting (Phase 4) remains deferred. Translumo's own README recommends Windows OCR exclusively, and adding Tesseract/EasyOCR would require distributing binaries and a Python runtime.

---

## Design

### Approach

Approach A — targeted addition. Add one field to `PreprocessingConfig` and `OcrConfig`, apply it as a third pipeline step after histogram equalization, and fix the backend/frontend defaults mismatch found during the audit.

### Preprocessing Pipeline

```
BGRA input
  → Grayscale (BT.601 luminance)
  → Histogram equalization (adaptive contrast normalization)
  → Binary threshold at 128 (pure black/white)
  → BGRA output
```

**Why 128 and not 150 (Translumo's value):** Translumo applies threshold to the raw grayscale image. We apply it after histogram equalization, which redistributes pixel intensities across the full 0–255 range. At that point, 128 (the midpoint) is the correct split. Using 150 post-EQ would bias toward white-heavy output.

Each step is gated by its own boolean flag and degrades gracefully if disabled.

---

## File Changes

### `src-tauri/src/ocr/preprocessing.rs`

- Add `binarize: bool` to `PreprocessingConfig`
- Add `binarize: true` to `PreprocessingConfig::optimal()`
- Add binarize step in `preprocess_image()` after histogram EQ:
  - For each pixel: if `gray_val < 128` → 0, else → 255
- Add two unit tests:
  - `test_binarize_threshold` — asserts pixels become only 0 or 255
  - `test_full_pipeline` — runs grayscale → EQ → binarize, asserts valid BGRA output with pure black/white values

### `src-tauri/src/config.rs`

Add to `OcrConfig`:
```rust
/// Apply binary threshold after contrast enhancement.
/// Converts image to pure black and white (threshold: 128/255).
/// Recommended for most subtitle scenarios.
#[serde(default = "default_binarize")]
pub binarize: bool,
```

**Defaults audit — changes only:**

| Field | Old default | New default | Reason |
|-------|-------------|-------------|--------|
| `binarize` | *(new)* | `true` | On for all users by default |
| `validation_strictness` | `Permissive` (0.2) | `Moderate` (0.4) | Backend/frontend were mismatched; Moderate is a better noise floor |

All other fields (`preprocessing_enabled`, `grayscale`, `contrast_enhancement`, `confidence_threshold`, `enable_multi_pass`, `multi_pass_count`) remain unchanged — they were already correctly aligned.

Update `OcrConfig::default()` and `TranslationConfig::default()` accordingly.

### `src-tauri/src/llm/manager.rs`

Update `base_config()` test fixture: `validation_strictness: ValidationStrictness::Moderate` (was `Permissive` / omitted — inconsistent with new default).

### `src/index.html`

Add one toggle row to the existing OCR preprocessing section:

```html
<div class="setting-row">
  <label for="toggle-ocr-binarize">Binarize image</label>
  <span class="setting-description">Convert to pure black &amp; white after contrast enhancement</span>
  <input type="checkbox" id="toggle-ocr-binarize" />
</div>
```

### `src/scripts/main.js`

Three updates:

1. `normalizeOcrConfig()` — add `binarize: true` to `defaultConfig`; read `ocr.binarize ?? true` in normalize block
2. `applyOcrSettings()` — set `toggle-ocr-binarize` checked state from config
3. `collectOcrSettings()` — include `binarize` in returned object

---

## Defaults Summary (post-change)

| Setting | Default | Effect |
|---------|---------|--------|
| `preprocessing_enabled` | `true` | All preprocessing on |
| `grayscale` | `true` | Grayscale conversion |
| `contrast_enhancement` | `true` | Histogram equalization |
| `binarize` | `true` *(new)* | Binary threshold at 128 |
| `validation_strictness` | `Moderate` *(changed)* | Effective threshold 0.4 |
| `confidence_threshold` | `0.5` | Cap when strictness = Strict |
| `enable_multi_pass` | `false` | Off for performance |
| `multi_pass_count` | `2` | Used when multi-pass enabled |

---

## Testing Plan

| Test | File | Type |
|------|------|------|
| `test_binarize_threshold` | `preprocessing.rs` | Unit |
| `test_full_pipeline` | `preprocessing.rs` | Unit |
| Update `base_config()` fixture | `manager.rs` | Test fixture |
| Manual: verify OCR quality on subtitle screenshots | — | Manual |

---

## Non-Goals

- Morphological operations (1×1 kernel = no-op in Translumo, no benefit)
- Multi-engine voting (deferred, Translumo recommends Windows OCR only)
- Configurable threshold value (fixed at 128 post-EQ; no UI needed)
- Adaptive/Otsu thresholding (YAGNI for now)
