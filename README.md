# 🐱 Meowcal Sub

> ⚠️ **BETA SOFTWARE** - This is an early beta release. Expect bugs and breaking changes. Please report issues on GitHub!

A local LLM-powered subtitle translation app for Windows. Captures any region of your screen, performs OCR, and displays translated subtitles in a floating overlay.

![Platform](https://img.shields.io/badge/Platform-Windows-blue)
![Version](https://img.shields.io/badge/Version-0.1.0--beta-orange)
![Rust](https://img.shields.io/badge/Built%20with-Rust%20%2B%20Tauri-orange)
![License](https://img.shields.io/badge/License-MIT-green)

## ✨ Features

- **🔒 Privacy First** - All OCR and translation happens locally on your device
- **🤖 Local LLM Translation** - Uses Foundry Local or other local LLM backends
- **🖼️ Floating Subtitles** - Translated text appears in a sleek overlay
- **📺 Video Support** - Hardware-accelerated capture works with videos and games
- **🌐 Multi-language** - Supports many language pairs via Windows OCR

## 📋 Requirements

| Requirement | Details |
|-------------|---------|
| **OS** | Windows 10/11 (Windows 11 recommended for best compatibility) |
| **RAM** | 8GB minimum, 16GB recommended |
| **LLM Backend** | [Foundry Local](https://github.com/microsoft/Foundry-Local) (robustly tested) or compatible OpenAI API endpoint |

## 🚀 Quick Start

### Prerequisites

1. **Install Rust** (if not already installed):
   ```powershell
   winget install Rustlang.Rustup
   ```

2. **Install Node.js** (v18+):
   ```powershell
   winget install OpenJS.NodeJS
   ```

3. **Install Visual Studio Build Tools** (Windows C/C++ toolchain):
   ```powershell
   winget install Microsoft.VisualStudio.2022.BuildTools
   ```

### Build & Run

```powershell
# Clone the repo
git clone https://github.com/PeterShanxin/Meowcal-Sub.git
cd Meowcal-Sub

# Run in development mode
npx tauri dev
```

On Windows ARM64 (or if you hit toolchain errors), use the helper script:
```powershell
.\dev-tauri.cmd
```

### Build for Release

```powershell
npx tauri build
```

The built app will be in `src-tauri/target/release/`.

## 🎮 Usage

1. **Launch the app** - A cat icon appears in your system tray
2. **Click "Select Area"** - Draw a box around the text you want to translate
3. **Click "Start Translation"** - Watch the magic happen!
4. **Translated subtitles** appear in a floating overlay below your selection

## ⚙️ Configuration

Edit settings in the app or modify `%APPDATA%\\com.meowcal.sub\\config.json`:

| Setting | Description | Default |
|---------|-------------|---------|
| `sourceLanguage` | OCR language (e.g., "en-US", "ja-JP") | `en-US` |
| `targetLanguage` | Translation target (e.g., "zh-CN") | `zh-CN` |
| `captureIntervalMs` | How often to capture (lower = smoother) | `500` |

### Translation Backends (Auto Fallback)

Default order:
1) Foundry Local
2) Offline MT (translateLocally / ORT model)
3) Windows AI (Phi Silica / LanguageModel)
4) Passthrough (OCR text)

You can configure backend feature flags under `translation` in config.

#### Context-Aware Controls (Foundry Local only)

| Setting | Description | Default |
|---------|-------------|---------|
| `translation.enableContextAware` | Master on/off switch | `true` |
| `translation.contextLevel` | `off` / `memoryOnly` / `memoryAndRecent` | `memoryAndRecent` |
| `translation.contextRecentCount` | How many recent lines to include when `memoryAndRecent` | `3` |
| `translation.contextBudgetPercent` | Context token budget as % of model window | `15` |
| `translation.contextSummaryCooldownMs` | Minimum time between summary runs | `5000` |
| `translation.promptMaxSourceChars` | Max OCR chars sent to LLM prompt builder | `300` |
| `translation.promptMaxContextChars` | Max context chars included in prompts | `600` |
| `translation.contextBufferSize` | Rolling subtitle context buffer (lines) | `12` |
| `translation.contextResetGapMs` | Clear context after idle gap | `6000` |

#### Offline MT (translateLocally) Setup

1) Install `translateLocally` on your machine (no auto-download in the app).
2) Either:
   - Add the binary to your `PATH`, or
   - Set `translation.offlineMt.binaryPath` to the full path of the binary.

Example config snippet:
```json
{
  "translation": {
    "offlineMt": {
      "binaryPath": "C:\\\\tools\\\\translateLocally\\\\translateLocally.exe",
      "timeoutMs": 3000,
      "maxChunkChars": 500
    }
  }
}
```

## ⚠️ Beta Limitations & Known Issues

### Current Limitations

- **Translation Backends:**
  - **Foundry Local** - Robustly tested and recommended ✅
  - **Offline MT (translateLocally)** - Included but not actively tested/used
  - **Windows AI (Phi Silica)** - Experimental Microsoft feature, not functional on most systems
  - *Future plan: Will gradually remove untested/unmaintained backends*

- **Overlay System:**
  - Currently using web-based overlay (legacy compatible method)
  - Debug overlay information is visible by default (see overlay.html line 65)
  - Multiple overlay implementations exist in codebase (WinUI3 version is deprecated)
  - *Future plan: Will remove unused overlay implementations*

- **Known Issues:**
  - First-run model downloads can cause delays
  - Multi-monitor setups with different DPI scaling may have positioning issues
  - No auto-update mechanism (manual updates required)
  - Limited language pair testing (contributions welcome!)

### Troubleshooting

**Build issues:**
- `clang` not found / `cc-rs` errors: install VS Build Tools and run from a VS Developer shell, or use `dev-tauri.cmd`.
- `failed to remove file ... meowcal-sub.exe`: close the running app (tray icon) and retry.

**Common backend status/warning codes:**
- `not_supported`: API/runtime not available.
- `not_ready`: model needs first-time download.
- `not_available`: backend binary not found.
- `timeout`: backend hung or took too long; fallback used.
- `backend_not_registered`: misconfigured backend id.

**Debug & Diagnostics:**

The app automatically generates verbose logs for every session at:
```
%APPDATA%\com.meowcal.sub\logs\meowcal-sub-YYYY-MM-DD_HH-MM-SS.log
```

**Important:** Logs are automatically rotated (kept for 7 days). When reporting issues:
1. Reproduce the issue
2. Find the corresponding session log (check timestamp)
3. Share the log file when asking for help

**Privacy Note:** While logs are verbose and include OCR/translated text, all processing happens **completely locally** on your machine. Logs contain:
- OCR results (source text)
- Translation results
- Backend status and errors
- System diagnostics

No data is sent to external servers during normal operation.


## 🏗️ Project Structure

```
meowcal-sub/
├── src-tauri/              # Rust backend
│   ├── src/
│   │   ├── main.rs         # App entry point
│   │   ├── commands.rs     # Tauri IPC commands
│   │   ├── capture/        # Screen capture
│   │   ├── ocr/            # Windows OCR
│   │   ├── llm/            # Phi Silica AI
│   │   └── overlay/        # Overlay management
│   └── Cargo.toml          # Rust dependencies
├── src/                    # Frontend (HTML/CSS/JS)
│   ├── index.html          # Main UI
│   ├── styles/             # CSS
│   └── scripts/            # JavaScript
└── README.md
```

## 🔧 Development

### Run Tests

```powershell
cd src-tauri
cargo test
```

### Check Linting

```powershell
cargo clippy
```

### Format Code

```powershell
cargo fmt
```

## 🗺️ Future Roadmap

### Planned Changes (TODO)

- [ ] **Backend Cleanup:**
  - Remove or clearly mark as experimental: Offline MT (translateLocally) backend
  - Remove Windows AI (Phi Silica) backend (non-functional experimental MS feature)
  - Focus development on Foundry Local backend

- [ ] **Overlay Architecture Cleanup:**
  - Remove deprecated WinUI3-based overlay implementation (`src-winui3/OverlayHost/`)
  - Keep and improve current web-based overlay (legacy compatible method)
  - Remove unused IPC infrastructure once WinUI overlay is removed

- [ ] **Production Polish:**
  - Remove or make optional the debug overlay (overlay.html line 65)
  - Add auto-update mechanism
  - Improve multi-monitor DPI handling
  - Add more language pair testing and optimizations

- [ ] **Testing & Documentation:**
  - Expand automated test coverage (see `docs/plans/2026-01-26-comprehensive-test-automation-design.md`)
  - Add video tutorials and usage examples
  - Create troubleshooting guide for common scenarios

Contributions welcome! See the Contributing section below.

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## 📄 License

MIT License - see [LICENSE](LICENSE) for details.

## 🙏 Acknowledgments

- Built with [Tauri](https://tauri.app/)
- Uses [Windows.Media.Ocr](https://docs.microsoft.com/en-us/uwp/api/windows.media.ocr) for text recognition
- Translation powered by local LLMs via [Foundry Local](https://github.com/microsoft/Foundry-Local)
- Overlay uses web-based rendering for maximum compatibility across Windows versions

---

Made with 🐱 for privacy-conscious translators
