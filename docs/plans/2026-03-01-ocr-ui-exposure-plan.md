# OCR Settings UI Exposure Plan

**Date**: 2026-03-01  
**Status**: Draft

## Summary

This plan outlines which OCR settings should be exposed in the app UI for user configuration.

## OCR Settings Available

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `confidenceThreshold` | float (0.0-1.0) | 0.5 | Min OCR confidence to accept |
| `preprocessingEnabled` | bool | true | Enable image preprocessing |
| `grayscale` | bool | true | Convert to grayscale |
| `contrastEnhancement` | bool | true | Apply histogram equalization |
| `enableMultiPass` | bool | false | Run multiple OCR passes |
| `multiPassCount` | int (1-5) | 2 | Number of OCR passes |
| `validationStrictness` | enum | "moderate" | Filter strictness |

## Recommended UI Exposure

### Tier 1: Essential (Most Users)
These should be visible in the main translation settings panel:

1. **Confidence Threshold** - Slider (0.0 - 1.0)
   - Default: 0.5
   - Label: "OCR Quality Filter"
   - Help text: "Higher values filter more noisy OCR results"

2. **Image Preprocessing** - Toggle
   - Default: ON
   - Label: "Enhance Image Quality"
   - Help text: "Improves OCR accuracy on subtitles"

### Tier 2: Advanced (Expandable Section)
These should be in an "Advanced OCR" collapsible section:

3. **Grayscale** - Toggle (default: ON)
4. **Contrast Enhancement** - Toggle (default: ON)  
5. **Multi-Pass OCR** - Toggle (default: OFF)
   - Help text: "Slower but more accurate"
6. **Pass Count** - Number input (1-5, default: 2)
   - Only shown when Multi-Pass is ON

### Tier 3: Expert (Rarely Changed)
These should be in an "Expert" section:

7. **Validation Strictness** - Dropdown
   - Options: "Permissive", "Moderate" (default), "Strict"
   - Only for power users who understand the tradeoffs

## Implementation Tasks

### 1. Update HTML (src/index.html)
Add OCR settings section in the translation panel

### 2. Update JavaScript (src/scripts/main.js)
- Add `applyOcrSettings()` function
- Add `normalizeOcrConfig()` function
- Update `saveSettings()` to include OCR config

### 3. Update CSS (src/styles/settings.css)
Add styling for new OCR settings controls

## Files to Modify:
- `src/index.html` - Add OCR settings UI elements
- `src/scripts/main.js` - Add OCR settings handlers
- `src/styles/settings.css` - Add styling

## UI Mockup

```html
<!-- OCR Settings Section -->
<div class="settings-section">
  <h3>OCR Settings</h3>
  
  <!-- Tier 1 -->
  <div class="setting-row">
    <label>OCR Quality Filter</label>
    <input type="range" id="ocr-confidence" min="0" max="100" value="50">
    <span class="value-display">0.5</span>
  </div>
  
  <div class="setting-row">
    <label>Enhance Image Quality</label>
    <input type="checkbox" id="ocr-preprocessing" checked>
  </div>
  
  <!-- Tier 2 (Advanced) -->
  <details class="advanced-section">
    <summary>Advanced OCR</summary>
    <div class="setting-row">
      <label>Grayscale</label>
      <input type="checkbox" id="ocr-grayscale" checked>
    </div>
    <div class="setting-row">
      <label>Contrast Enhancement</label>
      <input type="checkbox" id="ocr-contrast" checked>
    </div>
    <div class="setting-row">
      <label>Multi-Pass OCR</label>
      <input type="checkbox" id="ocr-multi-pass">
    </div>
  </details>
</div>
```
