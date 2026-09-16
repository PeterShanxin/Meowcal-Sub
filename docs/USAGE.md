# Using Meowcal Sub

[Back to the project](../README.md) · [简体中文简介](../README.zh-CN.md)

Meowcal Sub translates **visible text in a selected screen region**, not audio.
Start with a video that already displays subtitles in a language supported by
your Windows OCR installation.

## Installation

Use the installers on the [latest application release](https://github.com/PeterShanxin/Meowcal-Sub/releases/latest).
Choose **x64** for Intel/AMD Windows PCs or **ARM64** for Windows on ARM.
The `.exe` is the regular setup installer; `.msi` packages are also available.
GitHub's **Source code** archives are for developers, not app installation.
You do not need a separate Rust, Node.js, or Core installation.

The app is a Windows 11 public beta. The engine checks for at least **8 GiB of
total system RAM reported by Windows** and, when installation needs disk space,
**3 GiB free on the engine installation drive**. These thresholds come from the
[engine manifest](../core/config/engine-manifest.v1.json), not a performance
benchmark. Extra memory headroom is needed for your video player and other apps.

First-time setup downloads approximately **1.1 GB** of local translation assets.
That is a download estimate, **not a total installed-disk requirement**: allow
additional space for extraction, caches, and retained runtime versions. A missing
Windows OCR language is a separate setup requirement.

### Check a download

Download the installer and `SHA256SUMS.txt` from the **same application release**.
In PowerShell, compute the hash of the file you downloaded, for example:

```powershell
Get-FileHash -Algorithm SHA256 -LiteralPath ".\Meowcal.Sub_0.8.0_x64-setup.exe"
```

Use your actual filename for another version or architecture. Compare the full
hash with the entry for that exact filename in `SHA256SUMS.txt`, ignoring letter
case. Stop if they differ. Matching hashes check file integrity against the
published checksum; they are not an independent guarantee that software is safe.

Installers are not Authenticode-signed, so Windows SmartScreen may show an
unknown-publisher warning. Do not disable SmartScreen or other security
protection globally. Review the source and release origin before deciding to
run the installer. In-app updater signatures are a different mechanism and do
not constitute Windows publisher signing.

## First translation

Complete the setup wizard, or choose **Set up translation** on Home. Pick the
original subtitle language and your target language. Let setup download and
test the local engine. Follow **Install recognition language** if Windows lacks
the OCR language for the original subtitles.

Play a video with readable subtitles on your **primary monitor**, choose
**Select subtitle area**, and draw a box around the original lines. Include space
for all subtitle lines, while avoiding player controls, unrelated text, and the
translated overlay itself. The box is a screen region, not an object tracker:
reselect it when the video moves or changes size on that monitor. The normal
selector/capture workflow does not support secondary-display capture; move the
video back to the primary monitor before selecting its subtitle area.

Choose **Start translation**. The first start takes longer while the model warms
up. **Subtitle style** controls text size and a Dark or Light plate; use
**Stop translation** to end the session.

## When something does not look right

| Symptom | Check first |
| --- | --- |
| Home says the recognition language is missing | Install the Windows OCR language matching the original subtitles, not just the target translation language. |
| Home offers **Repair engine** | Use that action to check and restore the managed engine. Keep the displayed support code if repair fails. |
| Setup reports `ENGINE_INCOMPATIBLE` or `ENGINE_DISK_SPACE` | Check the Windows 11, 8 GiB system-RAM, and 3 GiB free-installation-space requirements above. A missing architecture/runtime or unsupported installation path can also produce an incompatibility error; include the exact support message in a report. |
| Text is missing or misread | Check that the selected region is still aligned and visible, contains every subtitle line, and does not include player controls or the translation overlay. Try clearer, larger source subtitles. |
| Readable non-subtitle text is ignored | Normal mode filters for subtitle-like text. Enable **Translate any text** in Settings only when that is your intended use. It does not improve the OCR model itself. |
| Translations come out garbled, or the PC stalls while the engine runs | Turn on **Settings → Engine and updates → Run the engine on CPU only**. The engine restarts on CPU. Include the support code and your GPU and driver version in a report. |
| Translation feels slow | Allow for the initial model warm-up. Check local CPU and memory pressure and the size of the selected region. Different hardware, source text, and model load produce different latency. |
| Video capture is blank or incomplete | Protected content or a player's display mode may not be capturable. Test with a non-protected local clip in a normal window; this app is not a capture-protection bypass. |
| The app says it is up to date | Check the installed version against the application release, not a `core-v...` runtime tag. A merged code change is not necessarily a released update. |

For update checks, open **Settings → Engine and updates → Check for updates**.
Automatic checks occur at startup at most once per day. Checking does not
itself download an application update.

## Frequently asked questions

### Can it create subtitles for a video that has none?

No. This app reads screen text with Windows OCR; it does not transcribe audio.
For subtitle-file search and playback-aligned sessions, see
[Meowcal Sub 2](https://github.com/PeterShanxin/Meowcal-Sub-2), a separate project
whose public release is coming soon. Its repository may show a 404 until public.

### Does it work offline?

OCR and translation can run offline after the local engine and recognition
language are installed. Setup, repair downloads, missing Windows language
components, and update checks/downloads use the network. Your video service has
its own connectivity requirements.

### How does GPU support differ by architecture?

**ARM64:** Adreno acceleration is gated to validated hardware/driver combinations.
Other configurations take the CPU policy. A GPU readiness timeout can trigger
one CPU retry within the existing startup deadline.

**x64:** the shipped runtime uses Vulkan. The application does not currently
apply the ARM64 hardware-validation gate or its CPU-retry behavior to x64. Do
not assume an incompatible or failing Vulkan configuration will automatically
switch to CPU.

**On either architecture**, **Settings → Engine and updates → Run the engine on
CPU only** starts the engine without GPU offload. Changing it restarts the
engine. Translation can be slower on CPU.

Both paths require the system RAM listed above. See
[Core performance evidence](CORE_PERFORMANCE.md) for scoped measurements, not
a universal latency promise or a prediction for every machine.

### Can it translate web pages or slides?

**Translate any text** in Settings relaxes the subtitle-specific filter for
readable text in the selected region. It is off by default. Recognition still
depends on text clarity, language support, and screen-capture availability.

### Do I need another Meow app?

No. Meowcal Sub uses its packaged, pinned [Meowcal Core](../core/README.md)
runtime. MeowWatch is for watching together; Sub 2 has a separate subtitle
workflow. Neither is required to run Sub 1.

## Get help safely

Use the [issue chooser](https://github.com/PeterShanxin/Meowcal-Sub/issues/new/choose).
Include the app version, Windows build, x64/ARM64 architecture, source and target
languages, the displayed support code, and a minimal non-private reproduction.
Do not post captured subtitle text, private screenshots, API keys, or raw logs
containing them. Production logs are designed to omit subtitle text; inspect
anything you share anyway. Report vulnerabilities privately using
[SECURITY.md](../SECURITY.md).
