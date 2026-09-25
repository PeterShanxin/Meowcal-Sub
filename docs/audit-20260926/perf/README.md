# Performance audit, 2026-09-26

Dated evidence, not standing policy. Measured on a Linux x86_64 cloud container
(2 vCPU, 7 GiB RAM, rustc 1.95.0 release profile with LTO, Chromium 1194
headless). Absolute numbers will differ on Windows hardware; the relative change
is the result.

## Flows measured

1. **Select subtitle area** - the step every session starts with. After the
   click, `selector_window::open_legacy` captures the primary screen, encodes it
   as a PNG `data:` URL, and only then shows the selector window, which parses
   the IPC payload and paints the image. Measured: encoding (Rust) and payload
   parse plus decode plus paint (webview). Not measured: screen capture and
   WebView2 IPC transfer, which need Windows.
2. **Home window cold start** - navigation to the rendered "Start translation"
   control in the built frontend, with backend commands answered instantly.
   Measured to rule the frontend in or out as a startup bottleneck.

## Fixtures

`make_fixtures.py` builds packed BGRA frames (what the capture backends return)
from repository images: full-screen video at 1080p and 4K, a windowed 1080p
video on a 1440p desktop, and a flat UI-only 1080p desktop. Video frames get
mild Gaussian grain so they compress like decoded video rather than a smooth
upscale. Fixtures are git-ignored; regenerate them with the script.

## Reproduce

```sh
python3 make_fixtures.py
cargo build --release --manifest-path snapshot-bench/Cargo.toml
VARIANTS=baseline-rgba-balanced,rgba-fast,rgb-fast \
  snapshot-bench/target/release/snapshot-bench fixtures 15 3 results/final-encode.json
npm run build:web   # from the repository root
node bench-selector-render.mjs 9 results/final-render-a.json baseline-rgba-balanced,rgba-fast,rgb-fast
node bench-selector-render.mjs 9 results/final-render-b.json rgb-fast,rgba-fast,baseline-rgba-balanced
node bench-home-startup.mjs 10 results/home-startup-baseline.json
```

`snapshot-bench` is standalone because `src-tauri` links Windows-only crates. Its
`baseline` variant is the base commit's `encode_snapshot` verbatim; `rgb-fast`
has the same body as the branch's `encode_snapshot` and `bgra_to_rgb`. It pins
`png`, `base64` and `serde_json` to the versions in `src-tauri/Cargo.lock`.
Every run starts with a fresh browser context; the first run per case is
discarded as browser warm-up.

## Results (p50, ms)

Encode is 15 runs after 3 warm-ups. Render is the mean of two 9-run passes in
opposite variant order (`final-render-a.json`, `final-render-b.json`).

| Fixture | Baseline encode | Baseline render | Baseline total | Branch encode | Branch render | Branch total | Change |
|---|---:|---:|---:|---:|---:|---:|---:|
| Full-screen video 1080p | 977 | 261 | 1238 | 19 | 208 | 227 | -82% |
| Windowed video, 1440p desktop | 859 | 287 | 1146 | 31 | 241 | 272 | -76% |
| Full-screen video 4K | 3906 | 925 | 4831 | 86 | 718 | 805 | -83% |
| Flat desktop 1080p | 90 | 82 | 172 | 9 | 93 | 102 | -41% |

Every PNG variant decodes back to the source pixels (`lossless: true`).
The flat desktop's webview render is about 11 ms slower on the branch because
the fast preset writes a larger file (0.56 MB against 0.32 MB); encoding saves
81 ms, so the flow is still faster end to end.

Home cold start (`home-startup-baseline.json`, 10 runs): first contentful paint
116 ms, "Start translation" rendered at 127 ms, no long tasks, 84 kB
transferred. The frontend is not a startup bottleneck; nothing was changed.
