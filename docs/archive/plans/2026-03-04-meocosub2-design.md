# MeoCoSub2 Design Document

**Date:** 2026-03-04
**Status:** Superseded
**Superseded by:** ADR-0001 and the 2026-07-29 curated local translation spec
**Predecessor:** MeoCoSub1 (Tauri 2 + Rust, real-time OCR → translate → overlay)

---

## Overview

MeoCoSub2 is a Python CLI application that generates translated subtitles for videos the user is already watching — in any player or browser. Instead of translating in real-time frame by frame (MeoCoSub1's approach), it fetches pre-existing subtitle files, optionally translates them offline, and syncs display using periodic OCR + fuzzy matching.

### Key differences from MeoCoSub1

| Aspect | MeoCoSub1 | MeoCoSub2 |
|--------|-----------|-----------|
| Translation trigger | Every captured frame, continuously | Up-front, once per video |
| Subtitle source | OCR → LLM translate in real-time | OpenSubtitles fetch or local LLM batch translate |
| Sync method | Continuous capture loop | Periodic OCR fuzzy match against source subtitle file |
| Display | Tauri webview overlay | Web overlay via FastAPI + WebSocket (browser window) |
| Platform | Windows only (Tauri 2 + Rust) | Windows primary (Python, `winocr`) |
| Video source | Any window region | Any window region — browser streaming OR local player |

---

## User Flow

```
1. meocosub2 run "Inception"
       |
2. Search OpenSubtitles API
   GET /subtitles?query=Inception&languages=en,zh
       |
3. User picks result from results table
       |
4. Download source-language subtitle (en) + target-language subtitle (zh)
   [If no target-language subtitle exists → translate via Foundry Local]
       |
5. Overlay server starts on localhost:8765
   Browser auto-opens with overlay page
       |
6. User starts their video in any player/browser
   User selects capture region (subtitle area of screen)
       |
7. Sync loop begins (every 1.5s):
   Capture region → OCR → fuzzy match against source subs → display target sub via WebSocket
```

---

## Architecture

### Project Structure

```
meocosub2/
  pyproject.toml
  config.example.toml
  src/
    meocosub2/
      __init__.py
      cli.py                # Typer CLI: run, search, start, translate, config commands
      config.py             # Config loading/saving (TOML → %APPDATA%/meocosub2/)
      models.py             # Shared data classes: SubtitleLine, SubtitlePair, MatchResult

      opensubtitles/
        __init__.py
        client.py           # httpx async client: search, download, auth, rate limiting
        types.py            # API response models (dataclasses)

      subtitles.py          # pysubs2 wrapper: parse .srt/.ass/.ssa, align pairs
      translator.py         # Foundry Local batch translation via openai SDK
      capture.py            # mss screen capture + winocr OCR
      matcher.py            # rapidfuzz fuzzy matching engine (windowed search)
      sync.py               # Main async loop: capture → OCR → match → push overlay

      overlay/
        __init__.py
        server.py           # FastAPI + WebSocket broadcast server
        static/
          index.html        # Overlay HTML page
          overlay.css       # Transparent subtitle styling
          overlay.js        # WebSocket client + subtitle display logic
```

**Entry point:** `meocosub2` CLI command defined in `pyproject.toml`.

### Component Responsibilities

| Component | Library | Purpose |
|-----------|---------|---------|
| CLI | `typer` + `rich` | Commands, progress bars, result tables |
| Config | `tomllib` / `tomli-w` | TOML config at `%APPDATA%/meocosub2/config.toml` |
| OpenSubtitles client | `httpx` (async) | Search, download, auth, rate limit tracking |
| Subtitle parser | `pysubs2` | Parse .srt/.ass/.ssa, extract timed lines |
| Screen capture | `mss` | Fast screen region capture |
| OCR | `winocr` | Windows-native OCR engine |
| Fuzzy matcher | `rapidfuzz` | Match OCR text to source subtitle lines |
| Translation | `openai` SDK | Batch translate via Foundry Local endpoint |
| Overlay server | `fastapi` + `uvicorn` | Serve HTML overlay + WebSocket subtitle updates |

---

## Detailed Component Design

### `models.py` — Shared Data Types

```python
@dataclass
class SubtitleLine:
    index: int
    start_ms: int
    end_ms: int
    text: str            # Source language text
    translated: str = "" # Target language text (empty until translated)

@dataclass
class SubtitlePair:
    source_lines: list[SubtitleLine]
    target_lines: list[SubtitleLine]  # empty if translation needed

@dataclass
class MatchResult:
    line_index: int
    score: float
    source_text: str
    target_text: str
```

### `opensubtitles/client.py` — API Client

Base URL: `https://api.opensubtitles.com/api/v1`

Methods:
- `async search(query, languages, media_type=None) -> list[SearchResult]`
- `async download(file_id) -> Path` — downloads to temp dir
- `async login() -> str` — optional JWT for higher quota

Rate limiting: tracks `X-RateLimit-Remaining` / `X-RateLimit-Reset` headers, exponential backoff on 429.

Download flow:
1. POST `/download` with `file_id` → get temporary link + `remaining` quota
2. GET the link (no auth needed, link is time-limited)

### `subtitles.py` — Parsing

- `load_subtitle_file(path) -> list[SubtitleLine]` — pysubs2, tries utf-8 then latin-1
- `align_subtitles(source, target) -> SubtitlePair` — pairs by index (1:1 alignment)
- `build_lookup_index(lines) -> dict` — preprocessed text for matcher startup

### `translator.py` — Batch Translation via Foundry Local

Called only when no target-language subtitle exists on OpenSubtitles.

Strategy:
- Send 5 lines per API call (batched numbered format)
- Include 3 most-recent translated lines as context in each prompt
- Output sanitization: strip quotes, labels, explanation lines
- Progress tracked with `rich` progress bar

```
Prompt format:
  Translate these 5 subtitle lines into Traditional Chinese.
  Output ONLY the translations, numbered 1-5. No explanations.

  Context:
  [last 3 translated lines]

  1. First line to translate
  2. Second line...
  ...
```

Output parsing: extract numbered lines from response, sanitize each line, fall back to source text if output is empty/invalid.

Translation is done before the sync loop starts — user sees a progress bar, then overlay starts.

### `matcher.py` — Fuzzy Matching Engine

This is the core novel component. Matches OCR text to the most likely source subtitle line currently on screen.

**Preprocessing (once at startup):**
- Normalize all subtitle lines: lowercase, strip formatting tags (`<i>`, `{\an8}`), remove HI markers `[music]`, collapse whitespace

**Per-frame matching:**

```
1. Hash OCR text — skip if identical to last frame (duplicate detection)
2. Normalize OCR text (same rules as subtitle preprocessing)
3. Skip if normalized text length < 3 chars (noise)
4. Define search window:
   - If previous match exists: search lines [last-5 ... last+30]
   - Otherwise: search lines [0 ... 50]
5. rapidfuzz.process.extractOne with token_set_ratio scorer
   - token_set_ratio handles partial OCR, reordered words, noise
   - Score cutoff: config.fuzzy_threshold (default 65)
6. If no match in window: fall back to full scan
7. Return MatchResult with line index, score, source text, target text
```

Window size of 30 forward / 5 backward handles:
- Normal forward playback (30 lines ≈ 60-90 seconds of lookahead)
- Brief rewinds (5 lines back)
- Pauses (match stays at same position)

### `sync.py` — Main Loop

```python
async def run_sync_loop(pair: SubtitlePair, config: AppConfig, broadcast: Callable):
    matcher = SubtitleMatcher(pair.source_lines, config.fuzzy_threshold)
    last_displayed_index = -1

    while True:
        start = time.monotonic()

        image = capture_region(config.capture_region)
        ocr_text = await ocr_image(image, config.ocr_language)
        result = matcher.match(ocr_text)

        if result and result.line_index != last_displayed_index:
            await broadcast(result.target_text)
            last_displayed_index = result.line_index

        elapsed = time.monotonic() - start
        await asyncio.sleep(max(0, config.capture_interval_ms / 1000 - elapsed))
```

### `overlay/server.py` — Web Overlay

FastAPI app with:
- `GET /` → serves `index.html`
- `GET /config` → returns overlay style settings as JSON
- `WebSocket /ws` → accepts connections, receives messages to broadcast subtitles

`OverlayServer.broadcast(text)` sends `{"type": "subtitle", "text": "..."}` to all connected clients. Dead connections are pruned on send failure.

**Overlay window:** The browser is auto-launched with:
```
chrome --app=http://localhost:8765 --window-size=900,120
```
This creates a minimal browser app window. Users can position it over their video manually.

**HTML/CSS design** (from MeoCoSub1 overlay patterns):
- Transparent background body
- Fixed-position subtitle container: centered, bottom 10%
- Text with semi-transparent dark background pill, configurable font/color
- CSS variables set from `/config` endpoint at load time
- Fade-out after 8s of no update

**WebSocket client JS:**
- Auto-reconnects on disconnect (2s delay)
- Fetches config on load and applies CSS variables
- Smoothly fades text in/out via CSS transition

---

## Configuration Schema

File: `%APPDATA%/meocosub2/config.toml`

```toml
[opensubtitles]
api_key = ""
username = ""       # Optional: for higher download quota
password = ""

[languages]
source = "en"       # ISO 639-2B code (3-letter for OpenSubtitles API)
target = "zht"      # "zht" = Traditional Chinese, "zhe" = Simplified, etc.

[capture]
region = []         # [x, y, width, height] — empty = prompt user to select
interval_ms = 1500
ocr_language = "en"

[matching]
fuzzy_threshold = 65    # 0-100, higher = stricter matching
window_size = 30        # Lines ahead to search from last match

[translation]
endpoint = "http://127.0.0.1:5273/v1"
model = ""              # Empty = auto-detect from Foundry
timeout_s = 30
batch_size = 5

[overlay]
port = 8765
font_size = 28
font_family = "Segoe UI"
text_color = "#FFFFFF"
bg_color = "rgba(0,0,0,0.75)"
position = "bottom"     # "bottom" or "top"
```

---

## Error Handling

| Failure | Detection | Recovery |
|---------|-----------|---------|
| Missing API key | Config validation at startup | Print error with setup instructions |
| API rate limited | 429 status / header check | Exponential backoff, show reset time |
| No subtitles found | Empty results list | Suggest alt search, try without language filter |
| Download quota exhausted | `remaining=0` in response | Inform user, show reset time |
| Subtitle parse error | pysubs2 exception | Try alternate encoding (utf-8-sig, latin-1) |
| Foundry Local not running | openai SDK connection error | Print `foundry service start` instructions |
| Translation timeout | SDK timeout | Skip line, use source text as fallback |
| LLM returns garbage | Sanitization → empty string | Keep source text (passthrough) |
| OCR returns empty | Empty string | Skip frame, continue loop |
| Capture region invalid | mss exception | Prompt user to re-select region |
| WebSocket disconnected | Send exception | Remove dead connection; overlay auto-reconnects |
| Config corrupt | TOML parse error | Fall back to defaults with warning |

All exceptions are logged to `%APPDATA%/meocosub2/logs/` with 7-day rotation (matching MeoCoSub1 pattern).

---

## Key Libraries

| Library | Version | Purpose |
|---------|---------|---------|
| `typer` | ≥0.9 | CLI framework, auto help generation |
| `rich` | ≥13.0 | Results tables, progress bars |
| `httpx` | ≥0.25 | Async HTTP for OpenSubtitles API |
| `pysubs2` | ≥1.6 | Parse .srt/.ass/.ssa subtitle files |
| `mss` | ≥9.0 | Fast cross-platform screen capture |
| `winocr` | ≥0.2 | Windows-native OCR engine |
| `rapidfuzz` | ≥3.0 | C-backed fuzzy string matching |
| `openai` | ≥1.0 | Foundry Local translation (OpenAI-compatible) |
| `fastapi` | ≥0.100 | Overlay web server + WebSocket |
| `uvicorn[standard]` | ≥0.23 | ASGI server for FastAPI |
| `tomllib` (stdlib 3.11+) | — | TOML config reading |
| `tomli-w` | ≥1.0 | TOML config writing |

---

## Implementation Phases

| Phase | Description | Key deliverables |
|-------|-------------|-----------------|
| 1 | Skeleton | `pyproject.toml`, `config.py`, `models.py`, empty CLI commands |
| 2 | Subtitle acquisition | `opensubtitles/client.py`, `subtitles.py`, `search` + `download` CLI |
| 3 | Translation | `translator.py`, `translate` CLI command |
| 4 | Overlay | `overlay/server.py` + static files, manual WebSocket test |
| 5 | Capture + OCR | `capture.py`, `matcher.py`, test OCR independently |
| 6 | Sync loop | `sync.py`, wire up `start` and `run` commands end-to-end |
| 7 | Polish | Error handling, config validation, region selector, auto-open browser |

---

## References (from MeoCoSub1)

These files in MeoCoSub1 contain patterns to port:

- `src-tauri/src/llm/prompt_router.rs` — Prompt templates, language detection, output sanitization
- `src-tauri/src/llm/context.rs` — Rolling context buffer for translation consistency
- `src-tauri/src/llm/foundry_local.rs` — Foundry Local API integration (model auto-select, service detection)
- `src-tauri/src/config.rs` — Configuration schema design (overlay settings, defaults, validation)
- `src/overlay.html` + `src/scripts/overlay.js` — Overlay HTML/CSS/JS structure
