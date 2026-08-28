<div align="center">

# {{PRODUCT_NAME}}

**{{TAGLINE}}**

![{{PRODUCT_NAME}}](assets/hero.png)

[![Latest release](https://img.shields.io/github/v/release/PeterShanxin/Meowcal-Sub-releases?label=latest)](https://github.com/PeterShanxin/Meowcal-Sub-releases/releases/latest)
![Windows](https://img.shields.io/badge/platform-Windows%2011-0078D6?logo=windows)
![Architectures](https://img.shields.io/badge/arch-x64%20%7C%20ARM64-blue)

[**Download latest**]({{DOWNLOAD_URL}}) · [**All releases**]({{RELEASES_URL}}) · [**Release notes**]({{RELEASE_NOTES_URL}})

</div>

---

## Why Meowcal Sub

Watching foreign-language video usually means choosing between cloud subtitle services
(that send your screen content elsewhere) and manual copy-paste. Meowcal Sub targets the
gap: **read subtitles from your screen and translate them on your PC**, with a floating
overlay you can place over the player.

## What it does

1. You select the on-screen subtitle region.
2. The app captures that band, runs Windows OCR, and translates locally.
3. Translated lines appear in a floating overlay while you watch.

No account. No upload of subtitle text. A one-time download sets up the local translation
runtime and model (~1.1 GB).

## Key features

{{FEATURES_TABLE}}

## How it works

```text
subtitle region on screen
    → screen capture
    → image preprocessing
    → Windows OCR
    → normalize / dedupe
    → local HY-MT translation
    → validate
    → floating overlay
```

High-level only — implementation details and source code are not published in this
repository.

## Engineering highlights

{{ENGINEERING_LIST}}

## Performance

{{BENCHMARKS_SECTION}}

## Architecture

![Pipeline overview](assets/architecture.svg)

| Layer | Role |
| --- | --- |
| Desktop shell | Tauri 2 — window lifecycle, tray, multi-webview UI |
| Capture & OCR | Windows screen capture + WinRT OCR |
| Translation | App-managed local HY-MT runtime with install/repair/rollback |
| Presentation | Selector, setup wizard, and always-on-top overlay webviews |
| Distribution | Dual-arch installers, signature-verified in-app updates |

## Privacy & local processing

**On your machine:** {{PRIVACY_LOCAL}}

**Uses the network for:** {{PRIVACY_NETWORK}}

**Never sent:** {{PRIVACY_NOT_SENT}}

Production logs record support codes, timings, and counts — not captured or translated
subtitle text.

## Compatibility

| | |
| --- | --- |
| OS | {{REQUIREMENTS_OS}} |
| x64 | Intel / AMD Windows PCs |
| ARM64 | Snapdragon and other ARM Windows PCs |
| Disk | {{REQUIREMENTS_DISK}} |

{{REQUIREMENTS_NOTES}}

Each release includes NSIS (`.exe`) and MSI installers for both architectures, plus
`SHA256SUMS.txt` and `latest.json` for the in-app updater.

## Project status

{{STATUS}} — actively developed in a private repository. This public repo is the
download and showcase surface; issues and feature discussion are not accepted here.

Latest shipped version: **v{{VERSION}}**.

## Releases

Installers are published as [GitHub Releases]({{RELEASES_URL}}). After v0.6.6, installed
copies can update from **Settings → Updates → Check for updates** (manual check only).

## About the engineering

This project is a from-scratch Windows desktop product: real-time capture, on-device
inference, ARM64 performance work, installer packaging, and update infrastructure — not a
thin wrapper around a cloud API. The release pipeline mirrors signed artifacts from a
private build repository into this public distribution repo after a manual quality gate.

## License & usage

{{LICENSE_SUMMARY}}

---

<p align="center"><sub>Meowcal Sub · v{{VERSION}} · {{STATUS}}</sub></p>
