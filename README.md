# Windows ARM Native Translation App

This application provides real-time translation of a selected screen area using the native Neural Processing Unit (NPU) on Windows on Arm devices (e.g., Surface Laptop 7, Surface Pro 11).

## Features
- **Native Performance:** Uses `onnxruntime-genai-directml` to run local LLMs (Phi-3) on the NPU/GPU.
- **Privacy First:** All OCR and translation happens locally on your device. No data is sent to the cloud.
- **Floating Subtitles:** Translated text appears in a transparent overlay below the original text.
- **Efficient:** Optimized to minimize battery usage by leveraging hardware acceleration.

## Requirements

- **Device:** Windows on Arm PC (Copilot+ PC recommended for NPU support).
- **OS:** Windows 11.
- **Python:** Python 3.10 or newer (ARM64 native version recommended).

## Installation

1.  **Install Python for Windows ARM64**
    *   Download from [Python.org](https://www.python.org/downloads/windows/). Look for "Windows embeddable package (64-bit ARM)" or the installer.

2.  **Clone or Download this Repository**

3.  **Install Dependencies**
    Open PowerShell or Command Prompt in the project folder and run:
    ```powershell
    pip install -r requirements.txt
    ```
    *Note: `winsdk` and `onnxruntime-genai-directml` are Windows-specific.*

## Running the App

1.  **Run the script:**
    ```powershell
    python main.py
    ```

2.  **First Run:**
    *   The application will automatically download the **Phi-3 Mini ONNX (DirectML)** model from Hugging Face. This is approx 2-3 GB. Please wait for the download to complete in the console.

3.  **Usage:**
    *   Click the **System Tray Icon** (or wait for the app to launch).
    *   Select **"Select Area"** from the menu (or it may launch automatically).
    *   Draw a box around the text you want to translate (e.g., a game dialog, a video subtitle, a document).
    *   The translation will appear floating below the box.

## Troubleshooting

*   **"Model not found":** Ensure you have internet access on the first run. Check the `models/` folder.
*   **Performance:** Ensure your device is plugged in or set to "Best Performance" if translation is slow. The NPU usually handles this efficiently.
*   **OCR Issues:** The app uses the system language for OCR. Ensure you have the necessary language packs installed in Windows Settings > Time & Language > Language & Region.

## Configuration

Edit `config.py` to change:
*   `TARGET_LANGUAGE`: The language to translate into (default: Spanish).
*   `OCR_LANGUAGE`: The language code for text recognition (default: en-US).
