# 🐱 Meowcal Sub

A local LLM-powered subtitle translation app for Windows. Captures any region of your screen, performs OCR, and displays translated subtitles in a floating overlay.

![Platform](https://img.shields.io/badge/Platform-Windows-blue)
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
| **OS** | Windows 10/11 |
| **RAM** | 8GB minimum, 16GB recommended |
| **LLM Backend** | [Foundry Local](https://github.com/microsoft/Foundry-Local) or compatible OpenAI API endpoint |

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

### Troubleshooting

Build issues:
- `clang` not found / `cc-rs` errors: install VS Build Tools and run from a VS Developer shell, or use `dev-tauri.cmd`.
- `failed to remove file ... meowcal-sub.exe`: close the running app (tray icon) and retry.

Common backend status/warning codes:
- `not_supported`: API/runtime not available.
- `not_ready`: model needs first-time download.
- `not_available`: backend binary not found.
- `timeout`: backend hung or took too long; fallback used.
- `backend_not_registered`: misconfigured backend id.

### Known Issues

**Overlay Window Chrome:**
- Clicking the capture frame then switching to the main Meowcal-Sub window may cause a persistent window bar to appear at the top of the screen. Hover over the capture frame area to dismiss.
- When clicking "Start Translation", there may be a brief white flash and faint acrylic edges visible at screen edges. This is the overlay window initializing.

**Button State Mismatch (intermittent):**
- Under certain conditions, the Start/Stop button state may become out of sync with the actual translation state. If clicking "Start Translation" shows "Translation is already running" error, the app may need to be restarted.

If translation falls back to passthrough (OCR text), check:
- Foundry Local: ensure it's running and accessible at the configured endpoint.
- Offline MT: set `translation.offlineMt.binaryPath` or add `translateLocally` to PATH.

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

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## 📄 License

MIT License - see [LICENSE](LICENSE) for details.

## 🙏 Acknowledgments

- Built with [Tauri](https://tauri.app/)
- Uses [Windows.Media.Ocr](https://docs.microsoft.com/en-us/uwp/api/windows.media.ocr) for text recognition
- Translation powered by local LLMs via [Foundry Local](https://github.com/microsoft/Foundry-Local)

---

Made with 🐱 for privacy-conscious translators
