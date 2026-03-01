# OCR Binarization & Default Settings Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add binary thresholding (128/255) as a third preprocessing step after histogram equalization, and fix the Permissive → Moderate validation strictness default mismatch between backend and frontend.

**Architecture:** Additive change. `PreprocessingConfig` gains a `binarize: bool` field; the pipeline runs grayscale → histogram EQ → binarize sequentially, each gated by its flag. `OcrConfig` gains the corresponding persisted setting. The `ValidationStrictness` default changes from `Permissive` to `Moderate` to align the Rust backend with the JS frontend's existing default.

**Tech Stack:** Rust (`image` crate already present), vanilla JS, Tauri 2.0. Run Rust tests with `cd src-tauri && cargo test --lib`. Build env: set `$env:CARGO_TARGET_DIR = "D:\cargo-build"` to avoid OneDrive locking.

**Design doc:** `docs/plans/2026-03-01-ocr-binarization-design.md`

---

### Task 1: Write failing tests for binarize

**Files:**
- Modify: `src-tauri/src/ocr/preprocessing.rs` (tests block, line ~216)

**Step 1: Add two failing tests to the `tests` module**

Add inside the `#[cfg(test)] mod tests` block (after the existing `test_contrast_stretch` test, before the closing `}`):

```rust
#[test]
fn test_binarize_threshold() {
    // Pixels below 128 → 0 (black), at/above 128 → 255 (white)
    let mut image = GrayImage::new(4, 1);
    image.put_pixel(0, 0, Luma([0]));
    image.put_pixel(1, 0, Luma([127]));
    image.put_pixel(2, 0, Luma([128]));
    image.put_pixel(3, 0, Luma([255]));

    let result = apply_binarize(&image);
    assert_eq!(result.get_pixel(0, 0)[0], 0,   "0 → black");
    assert_eq!(result.get_pixel(1, 0)[0], 0,   "127 → black");
    assert_eq!(result.get_pixel(2, 0)[0], 255, "128 → white");
    assert_eq!(result.get_pixel(3, 0)[0], 255, "255 → white");
}

#[test]
fn test_full_pipeline_binarized_output() {
    // Run full grayscale → EQ → binarize pipeline; output must be only 0 or 255
    let width = 10u32;
    let height = 10u32;
    let mut image_data = Vec::with_capacity((width * height * 4) as usize);
    for i in 0..(width * height) {
        let val = ((i * 255) / (width * height)) as u8;
        image_data.extend_from_slice(&[val, val, val, 255u8]); // BGRA
    }

    let config = PreprocessingConfig {
        grayscale: true,
        contrast_enhancement: true,
        binarize: true,
    };

    let result = preprocess_image(&image_data, width, height, config);

    assert_eq!(result.len(), (width * height * 4) as usize, "output size");
    for chunk in result.chunks(4) {
        let b = chunk[0];
        assert!(b == 0 || b == 255, "expected 0 or 255, got {}", b);
        assert_eq!(chunk[0], chunk[1], "B == G");
        assert_eq!(chunk[1], chunk[2], "G == R");
        assert_eq!(chunk[3], 255,      "alpha == 255");
    }
}
```

**Step 2: Run tests to confirm they fail**

```powershell
$env:CARGO_TARGET_DIR = "D:\cargo-build"
cd src-tauri
cargo test --lib ocr::preprocessing 2>&1
```

Expected: compile error — `apply_binarize` not found, `binarize` field unknown.

---

### Task 2: Implement binarize in preprocessing.rs

**Files:**
- Modify: `src-tauri/src/ocr/preprocessing.rs`

**Step 1: Add `binarize: bool` to `PreprocessingConfig`**

In the `PreprocessingConfig` struct (after `contrast_enhancement: bool`):

```rust
/// Apply binary threshold after contrast enhancement.
/// Converts image to pure black and white at the midpoint (128/255).
pub binarize: bool,
```

**Step 2: Update `optimal()` to include `binarize: true`**

```rust
pub fn optimal() -> Self {
    Self {
        grayscale: true,
        contrast_enhancement: true,
        binarize: true,
    }
}
```

**Step 3: Update `is_enabled()` to include `binarize`**

```rust
pub fn is_enabled(&self) -> bool {
    self.grayscale || self.contrast_enhancement || self.binarize
}
```

**Step 4: Add the binarize step in `preprocess_image()`**

The current code ends Step 2 with:
```rust
    // Step 2: Apply contrast enhancement if enabled
    let final_gray: GrayImage = if config.contrast_enhancement {
        ...
        apply_histogram_equalization(&gray_image)
    } else {
        ...
        gray_image
    };
```

Rename that binding to `after_eq` and add Step 3 after it:

```rust
    // Step 2: Apply contrast enhancement if enabled
    let after_eq: GrayImage = if config.contrast_enhancement {
        debug!("Applying contrast enhancement...");
        apply_histogram_equalization(&gray_image)
    } else {
        debug!("Skipping contrast enhancement");
        gray_image
    };

    // Step 3: Apply binary threshold if enabled
    // Pixels below 128 become 0 (black); 128 and above become 255 (white).
    // Applied after EQ so the threshold is always at the normalized midpoint.
    let final_gray: GrayImage = if config.binarize {
        debug!("Applying binary threshold (128)...");
        apply_binarize(&after_eq)
    } else {
        debug!("Skipping binarization");
        after_eq
    };
```

**Step 5: Add the `apply_binarize` private function** (after `apply_contrast_stretch`):

```rust
/// Apply binary threshold to a grayscale image.
///
/// Pixels with intensity < 128 become 0 (black).
/// Pixels with intensity >= 128 become 255 (white).
/// Call this after histogram equalization for best results.
fn apply_binarize(image: &GrayImage) -> GrayImage {
    let (width, height) = image.dimensions();
    let mut output = GrayImage::new(width, height);
    for (x, y, pixel) in image.enumerate_pixels() {
        let new_val: u8 = if pixel[0] < 128 { 0 } else { 255 };
        output.put_pixel(x, y, Luma([new_val]));
    }
    output
}
```

**Step 6: Run tests — all should pass**

```powershell
$env:CARGO_TARGET_DIR = "D:\cargo-build"
cd src-tauri
cargo test --lib ocr::preprocessing 2>&1
```

Expected: all 5 preprocessing tests pass (3 existing + 2 new).

**Step 7: Also fix the test fixture that constructs `PreprocessingConfig` directly**

The existing test `test_preprocessing_config_defaults` uses `PreprocessingConfig::default()`. The `Default` derive sets all bools to `false`, which is correct — the default struct is disabled. The `optimal()` tests use `optimal()` which now includes `binarize: true`. No changes needed here.

**Step 8: Commit**

```bash
git add src-tauri/src/ocr/preprocessing.rs
git commit -m "feat(ocr): add binary threshold step to preprocessing pipeline

Adds binarize: bool to PreprocessingConfig. When enabled, applies a
hard threshold at 128 after histogram equalization, converting the
image to pure black/white. Enabled in optimal() preset.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

### Task 3: Update OcrConfig — add binarize field, fix strictness default

**Files:**
- Modify: `src-tauri/src/config.rs`

**Step 1: Add `binarize` field to `OcrConfig` struct**

In `OcrConfig`, after the `contrast_enhancement` field (around line 197):

```rust
/// Apply binary threshold after contrast enhancement.
/// Converts the image to pure black and white at the midpoint (128/255).
/// Recommended: true for most subtitle scenarios.
#[serde(default = "default_binarize")]
pub binarize: bool,
```

**Step 2: Add the default function**

After `fn default_contrast_enhancement() -> bool { true }`:

```rust
fn default_binarize() -> bool {
    true
}
```

**Step 3: Fix ValidationStrictness default — Permissive → Moderate**

In the `ValidationStrictness` enum, move the `#[default]` attribute:

```rust
pub enum ValidationStrictness {
    /// Permissive: only rejects obvious garbage
    Permissive,
    /// Moderate: balances false positives and false negatives
    #[default]
    Moderate,
    /// Strict: aggressively filters potential garbage
    Strict,
}
```

**Step 4: Update `OcrConfig::default()` to include `binarize`**

In the `impl Default for OcrConfig` block, add:

```rust
binarize: default_binarize(),
```

The `validation_strictness: ValidationStrictness::default()` line already exists and will now resolve to `Moderate` automatically.

**Step 5: Verify it compiles**

```powershell
$env:CARGO_TARGET_DIR = "D:\cargo-build"
cd src-tauri
cargo check 2>&1
```

Expected: no errors.

**Step 6: Run all lib tests**

```powershell
$env:CARGO_TARGET_DIR = "D:\cargo-build"
cd src-tauri
cargo test --lib 2>&1
```

Expected: all pass.

**Step 7: Commit**

```bash
git add src-tauri/src/config.rs
git commit -m "feat(config): add binarize setting, fix validation_strictness default

Adds OcrConfig.binarize (default true). Changes ValidationStrictness
default from Permissive (0.2) to Moderate (0.4) to align backend with
the JS frontend which already defaulted to 'moderate'.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

### Task 4: Wire binarize through commands.rs

**Files:**
- Modify: `src-tauri/src/commands.rs` (around lines 1648–1678 and 1929–1932)

**Step 1: Extract the binarize setting from config (line ~1653)**

After `let ocr_contrast_enhancement = translation_config.ocr.contrast_enhancement;`, add:

```rust
let ocr_binarize = translation_config.ocr.binarize;
```

**Step 2: Add binarize to the debug log (line ~1668)**

Update the `debug!` macro to include `binarize={}`:

```rust
debug!(
    "OCR settings: confidence_threshold={:.2}, preprocessing={}, grayscale={}, contrast={}, binarize={}, multi_pass={}, pass_count={}, strictness={:?}, effective_threshold={:.2}",
    ocr_confidence_threshold,
    ocr_preprocessing_enabled,
    ocr_grayscale,
    ocr_contrast_enhancement,
    ocr_binarize,
    ocr_enable_multi_pass,
    ocr_multi_pass_count,
    ocr_validation_strictness,
    effective_confidence_threshold
);
```

**Step 3: Pass binarize into PreprocessingConfig (line ~1929)**

Update the `PreprocessingConfig` construction:

```rust
let preprocessing_config = PreprocessingConfig {
    grayscale: ocr_grayscale,
    contrast_enhancement: ocr_contrast_enhancement,
    binarize: ocr_binarize,
};
```

**Step 4: Verify it compiles and tests pass**

```powershell
$env:CARGO_TARGET_DIR = "D:\cargo-build"
cd src-tauri
cargo test --lib 2>&1
```

Expected: all pass.

**Step 5: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "feat(commands): wire binarize setting into OCR preprocessing pipeline

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

### Task 5: Fix manager.rs test fixture

**Files:**
- Modify: `src-tauri/src/llm/manager.rs` (line ~1194)

**Step 1: Verify the current state**

The `base_config()` fixture in manager.rs already has `ocr: crate::config::OcrConfig::default()` (added in the compile-fix earlier in this branch). Because `OcrConfig::default()` now returns `Moderate` and `binarize: true`, no explicit field change is needed.

However, confirm by running the tests:

```powershell
$env:CARGO_TARGET_DIR = "D:\cargo-build"
cd src-tauri
cargo test --lib llm::manager 2>&1
```

Expected: all pass. If any test fails due to the strictness change, update the specific assertion to `Moderate`.

**Step 2: Commit (only if changes were needed)**

```bash
git add src-tauri/src/llm/manager.rs
git commit -m "test(llm): align manager test fixture with Moderate strictness default

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

If no changes were needed, skip this commit.

---

### Task 6: Add binarize toggle to index.html

**Files:**
- Modify: `src/index.html` (inside the Advanced OCR `<details>` block, around line 485)

**Step 1: Add the toggle after the Contrast Enhancement block**

Insert after the closing `</div>` of the Contrast Enhancement block (after line 485) and before the `<!-- Multi-Pass OCR -->` comment:

```html
                        <!-- Binarize -->
                        <div class="toggle-wrapper">
                            <span class="toggle-label">Binarize</span>
                            <label class="toggle">
                                <input type="checkbox" id="toggle-ocr-binarize" checked>
                                <span class="toggle-slider"></span>
                            </label>
                        </div>
```

**Step 2: Update the hint text (line ~463)**

Change the hint below the main preprocessing toggle from:

```html
<p class="setting-hint">Improves OCR accuracy on subtitles (grayscale + contrast)</p>
```

To:

```html
<p class="setting-hint">Improves OCR accuracy on subtitles (grayscale + contrast + binarize)</p>
```

**Step 3: Commit**

```bash
git add src/index.html
git commit -m "feat(ui): add binarize toggle to OCR advanced settings

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

### Task 7: Update main.js — normalize, apply, collect

**Files:**
- Modify: `src/scripts/main.js` (functions `normalizeOcrConfig`, `applyOcrSettings`, `collectOcrSettings`)

**Step 1: Add `binarize` to `normalizeOcrConfig` (line ~464)**

In `defaultConfig`, after `contrastEnhancement: true`:

```javascript
binarize: true,
```

In the returned object (the `normalizeOcrConfig` return block), after the `contrastEnhancement` line:

```javascript
binarize: ocr.binarize ?? defaultConfig.binarize,
```

**Step 2: Add `binarize` to `applyOcrSettings` (line ~421)**

After the contrast toggle block (after `if (contrastToggle) contrastToggle.checked = config.contrastEnhancement;`):

```javascript
const binarizeToggle = document.getElementById('toggle-ocr-binarize');
if (binarizeToggle) binarizeToggle.checked = config.binarize;
```

**Step 3: Add `binarize` to `collectOcrSettings` (line ~494)**

In the returned object, after `contrastEnhancement`:

```javascript
binarize: document.getElementById('toggle-ocr-binarize')?.checked ?? true,
```

**Step 4: Commit**

```bash
git add src/scripts/main.js
git commit -m "feat(ui): wire binarize toggle in OCR settings JS

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

### Task 8: Final verification

**Step 1: Run all Rust tests**

```powershell
$env:CARGO_TARGET_DIR = "D:\cargo-build"
cd src-tauri
cargo test --lib 2>&1
cargo test --test integration_ipc 2>&1
```

Expected: all pass.

**Step 2: Run clippy**

```powershell
$env:CARGO_TARGET_DIR = "D:\cargo-build"
cd src-tauri
cargo clippy -- -D warnings 2>&1
```

Expected: no warnings.

**Step 3: Run fmt check**

```powershell
$env:CARGO_TARGET_DIR = "D:\cargo-build"
cd src-tauri
cargo fmt --check 2>&1
```

Expected: no changes needed. If there are changes, run `cargo fmt` and commit.

**Step 4: Commit fmt fixes if needed**

```bash
git add src-tauri/src/
git commit -m "style(ocr): cargo fmt

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```
