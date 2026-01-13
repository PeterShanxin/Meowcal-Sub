# 🐱 Meowcal Sub

A local, NPU-accelerated subtitle translation app for **Copilot+ PCs** (Windows on ARM).

![Platform](https://img.shields.io/badge/Platform-Windows%20ARM64-blue)
![Rust](https://img.shields.io/badge/Built%20with-Rust%20%2B%20Tauri-orange)
![License](https://img.shields.io/badge/License-MIT-green)

## ✨ Features

- **🔒 Privacy First** - All OCR and translation happens locally on your device
- **⚡ NPU Accelerated** - Uses Windows Copilot Runtime for efficient AI processing
- **🖼️ Floating Subtitles** - Translated text appears in a sleek overlay
- **🔋 Battery Efficient** - Optimized to minimize CPU/GPU usage
- **🌐 Multi-language** - Supports many language pairs

## 📋 Requirements

| Requirement | Details |
|-------------|---------|
| **Device** | Copilot+ PC (Qualcomm Snapdragon X, Intel Core Ultra, AMD Ryzen AI) |
| **OS** | Windows 11 24H2 (Build 26100+) |
| **RAM** | 8GB minimum, 16GB recommended |

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

### Build & Run

```powershell
# Clone the repo
git clone https://github.com/PeterShanxin/Meowcal-Sub.git
cd Meowcal-Sub

# Run in development mode
npx tauri dev
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

Edit settings in the app or modify `config.json`:

| Setting | Description | Default |
|---------|-------------|---------|
| `sourceLanguage` | OCR language (e.g., "en-US", "ja-JP") | `en-US` |
| `targetLanguage` | Translation target (e.g., "zh-CN") | `zh-CN` |
| `captureIntervalMs` | How often to capture (lower = smoother) | `500` |

### Translation Backends (Auto Fallback)

Default order:
1) Windows AI (Phi Silica / LanguageModel)
2) Offline MT (translateLocally / ORT model)
3) Edge Translator (experimental)
4) Passthrough (OCR text)

You can configure backend preferences and feature flags under `translation` in config.

#### Offline MT (translateLocally) Setup

1) Install `translateLocally` on your machine (no auto-download in the app).
2) Either:
   - Add the binary to your `PATH`, or
   - Set `translation.offlineMt.binaryPath` to the full path of the binary.

Example config snippet:
```json
{
  "translation": {
    "preferredBackend": "offline_mt",
    "offlineMt": {
      "binaryPath": "C:\\\\tools\\\\translateLocally\\\\translateLocally.exe",
      "timeoutMs": 3000,
      "maxChunkChars": 500
    }
  }
}
```

### Troubleshooting

Common backend status/warning codes:
- `not_supported`: API/runtime not available (Windows AI or Edge Translator missing).
- `not_ready`: model needs first-time download or bindings not wired yet.
- `not_available`: offline MT binary not found.
- `timeout`: backend hung or took too long; fallback used.
- `backend_not_registered`: misconfigured backend id.

If translation falls back to passthrough (OCR text), check:
- Windows AI: ensure Copilot+ PC + Windows 11 24H2, WinAppSDK bindings, and required capabilities.
- Offline MT: set `translation.offlineMt.binaryPath` or add `translateLocally` to PATH.
- Edge Translator: WebView2 runtime must support `navigator.translation`; it’s experimental and opt-in.

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

## 🛣️ Roadmap

- [x] Basic Tauri app structure
- [x] Screen capture (Win32 GDI)
- [x] Windows OCR integration
- [x] Area selection UI
- [x] Translation loop (capture → OCR → translate → emit events)
- [x] Windows.Graphics.Capture for video support (GDI can't capture HW-accelerated content)
- [x] Overlay window for displaying translations
- [ ] Phi Silica translation (when Windows AI APIs are stable)
- [ ] Settings persistence
- [ ] Auto-start with Windows
- [ ] Multiple monitor support

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## 📄 License

MIT License - see [LICENSE](LICENSE) for details.

## 🙏 Acknowledgments

- Built with [Tauri](https://tauri.app/) 
- Uses [Windows.Media.Ocr](https://docs.microsoft.com/en-us/uwp/api/windows.media.ocr) for text recognition
- Powered by [Windows Copilot Runtime](https://learn.microsoft.com/en-us/windows/ai/)

---

Made with 🐱 for Copilot+ PCs
