# Using Meowcal Sub

[Back to the project](../README.md) · [简体中文简介](../README.zh-CN.md)

Meowcal Sub translates **visible text in a selected screen region**, not audio.
Start with a video that already displays subtitles in Chinese, Japanese,
Korean, English, Spanish, French, or German.

This guide describes the latest published application release. To see what a
specific release changed, read its notes under [`releases/`](releases/).

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
additional space for extraction, caches, and retained runtime versions.

### Check a download

Download the installer and `SHA256SUMS.txt` from the **same application release**.
In PowerShell, compute the hash of the file you downloaded, for example:

```powershell
Get-FileHash -Algorithm SHA256 -LiteralPath ".\Meowcal.Sub_0.9.1_x64-setup.exe"
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

On `main` (not yet released), first-time setup asks for agreement to the model
terms before preparing the engine. Choose languages on Home after setup. Setup
shows the current operation and download progress reported by Core. A previously
configured installation shows a completion screen; Core still verifies assets
when starting the engine. After the model terms are accepted, Core can reuse eligible
local assets. Cancelling initial setup exits the app.

1. Complete the setup wizard, or choose **Set up translation** on Home. Let
   setup download and test the local engine.
2. On Home, choose **Original subtitles** and **Translate into**.
3. Move the Home window onto the screen that plays the video.
4. Choose **Start translation**. The subtitle-area selector opens.
5. Drag across the original subtitles, or press <kbd>Enter</kbd> to place a
   suggested box in the lower part of the screen. When the video shows two-line
   subtitles, include both lines. Avoid player controls, unrelated text, and
   the place where the translation will appear.
6. Choose **Confirm and start**, or press <kbd>Enter</kbd>. Arrow keys move the
   box; <kbd>Shift</kbd> + arrow keys resize it. **Redraw** starts the box
   again, and **Cancel** (<kbd>Esc</kbd>) returns to Home without starting.

The first start takes longer while the model warms up. Translations appear in a
floating overlay near the selected area. The overlay has two controls; when
they have faded, point at the overlay or switch to it with
<kbd>Alt</kbd>+<kbd>Tab</kbd>. **Subtitle style** sets text size and a Dark or
Light plate, and **Exit translation** stops and returns to Home. **Stop translation** on Home
also ends the session.

In the unreleased development build, Home shows readiness through the primary
button: preparation disables it and shows a spinner; **Select subtitle area**
opens the selector when ready. The selector offers **Confirm and start**,
**Redraw**, **Save and exit**, and **Exit**. The mouse can move or resize a
selection; arrow-key movement and resizing are not available. Home shows
**Let’s watch** when ready. If a valid area is saved for the current display,
**Start with saved area** starts without reopening the selector. Repair instructions
and other actionable messages remain visible when needed.

In the latest release, to choose or adjust the area without starting, use the
subtitle-area row on Home (**Select** or **Change**) and choose **Use this area**.
In the unreleased development build, choose **Select subtitle area**, then
**Save and exit**. To reuse it directly, choose **Start with saved area**.

The area is a fixed screen region, not an object tracker. When the video moves,
changes size, or switches between window and full screen, select the area again.

### Multiple displays

With multiple displays, Home shows a short notice. The selector, recognition,
and overlay use the screen that contains the largest part of the Home window
when the selector opens. A running session stays on that screen even if you
move Home. To switch screens, stop translation, move Home, and select the area
again.

If the capture screen disconnects, or its position, resolution, or scaling
changes, translation stops and the overlay hides. Reconnecting does not resume
capture; select the subtitle area again. After a restart, the app restores a
saved area only when the same screen, bounds, and scaling are still present and
Home is on that screen.

## Settings

On `main` (not yet released), **Settings → About** shows the app version,
provider, project link, and model license and usage terms. The first-time setup
still presents the provider disclosure and requires agreement to the model terms.
Settings changes save automatically, with a four-second success notice.

| Setting | What it does | When to change it |
| --- | --- | --- |
| **Translate any text** | Translates readable text such as pages, menus, and games, not only subtitle-shaped lines. Off by default. | Turn it on for slides or apps. Keep it off for video: it also admits watermarks, timers, and scene text. |
| **Keep names consistent** | Sends recent lines as context so names and terms stay consistent. Off by default. | Leave it off unless names change between lines; context can make the model repeat an earlier line. |
| **Profile → Automatic** | Reads with PP-OCRv6 and lets translation use the graphics card when the engine can. | The default for most PCs. |
| **Profile → Compatibility** | Reads with Windows OCR and translates on the CPU. | Use it when translations are garbled, the graphics driver misbehaves, or the PC stalls. |
| **Profile → Custom** | Shown when a choice under **Advanced** differs from both profiles. | Not selectable; choose a profile to return to it. |
| **Advanced → Text recognition** | **PP-OCRv6** (built in, no Windows language needed) or **Windows OCR**. Korean subtitles always use Windows OCR. | Try **Windows OCR** for a subtitle style PP-OCRv6 misreads. |
| **Advanced → Run the engine on CPU only** | Starts translation without GPU offload. Changing it restarts the engine. | Use it for garbled output or stalls while keeping PP-OCRv6. Translation can be slower. |
| **Advanced → Recognition quality** | Shown for Windows OCR only: **Fast**, **Balanced**, or **Accurate**. | Use **Accurate** for hard-to-read text if your PC has spare capacity. |
| **Engine and updates → Translation engine** | Shows the engine state. **Test** runs a private sample translation; **Repair** checks and restores the engine. | Use **Repair** when Home asks for it or a test fails. |
| **Automatically check for updates** | Checks at most once a day, after startup. | Turn it off to check only by hand. |
| **Developer options → Developer mode** | Shows **Show subtitle diagnostics**, the engine phase, and the support code. | For troubleshooting only. |

Windows OCR needs the Windows recognition language for the original subtitles.
When it is missing, Home shows **Install recognition language**.

While translation runs on the CPU, PP-OCRv6 uses two CPU threads to leave room
for translation; with GPU translation it uses up to four.

## What the app is doing

### Home

| Status | Meaning | What to do |
| --- | --- | --- |
| **Checking** | The app checks the engine, recognition, and saved area. | Wait. |
| **Setup needed** | The engine is not installed. | Choose **Set up translation**. |
| **Preparing** | The engine is loading or checking its model. | Wait; you can start when it is ready. |
| **Busy** | The engine is finishing another operation, such as an install or repair. | Wait, then try again. |
| **Needs repair** | The engine check failed. Your settings are kept. | Choose **Repair engine**. Keep the support code if repair fails. |
| **Almost ready** | Windows OCR needs the recognition language, or no subtitle area exists yet. | Choose **Install recognition language** or **Start translation**. |
| **Select area** | The selector is open. | Confirm or cancel in the selector. |
| **Starting** | The engine warms up for the session. | Wait. |
| **Running** | Subtitles are live. | Choose **Stop translation** to end. |

If a session ends on its own, Home shows **Translation stopped unexpectedly.
Start translation again.** If recognition fails three times in a row, Home shows
**Text recognition is not responding. Return to Home and restart translation.**

### Overlay

| Overlay shows | Meaning | What to do |
| --- | --- | --- |
| A translated line | The latest subtitle that finished translating. | Nothing. |
| **Preparing translation…** | The engine is still warming up or recovering. | Wait. |
| **Waiting for subtitles…** | Recognition works, but the area has shown no text for 15 seconds. | Check that subtitles are visible inside the area. The notice clears when text returns. |
| **Translation engine stopped.** | Engine recovery failed. The app tries again after 30, 60, 120, and then every 300 seconds while subtitles need translation. | Wait, or choose **Exit to Home** and use **Repair engine**. |
| **Translation is interrupted.** | Capture or recognition stopped working, or the session ended. | Choose **Exit to Home**, then start again. |

When one translation takes too long or the engine briefly fails, the current
line stays on screen and the app retries that subtitle while it is still
visible. Short gaps and successful retries do not show a notice.

## How subtitles are handled

- **Order.** Each subtitle is translated once and shown in the order it
  appeared. A new subtitle needs two matching readings before it is accepted,
  so one misread frame does not create a line.
- **Fast dialogue.** New subtitles start translating while the previous one is
  still displayed. Each translation stays visible long enough to read, but no
  longer than its original subtitle was on screen (at most four seconds), so
  the overlay does not fall further and further behind. If translation still
  falls 15 seconds or 32 lines behind, the app skips the waiting lines and
  continues from the current subtitle.
- **Pause and resume.** While the video is paused, the same subtitle is not
  translated again and its translation stays on screen. After you resume, the
  next subtitle is translated normally.
- **Repeated subtitles.** If the same text disappears and then returns, it is
  translated and shown again. Two identical lines with no blank between them
  count as one subtitle. Changes in case, spacing, or final punctuation do not
  create a new subtitle; a changed word, number, or character does.
- **Screen changes.** Moving or resizing the video does not move the area.
  Select the area again. Changing the selected area during a session restarts
  the session and discards translations still in progress.
- **Credits and signs.** Normal mode filters out watermarks, timers, and moving
  scene text. Fast lists of names, such as credits, may still be translated.

## When something does not look right

| Symptom | Check first |
| --- | --- |
| Home says the recognition language is missing | Windows OCR is reading the subtitles (Korean, **Compatibility**, or **Text recognition → Windows OCR**). Install the Windows OCR language for the original subtitles, not the target language. |
| Home offers **Repair engine** | Use that action to check and restore the managed engine. Keep the displayed support code if repair fails. |
| Setup reports `ENGINE_INCOMPATIBLE` or `ENGINE_DISK_SPACE` | Check the Windows 11, 8 GiB system-RAM, and 3 GiB free-installation-space requirements above. A missing architecture/runtime or unsupported installation path can also produce an incompatibility error; include the exact support message in a report. |
| Text is missing or misread | Check that the area is aligned, visible, contains every subtitle line, and excludes player controls and the overlay. Try clearer, larger source subtitles, or the other **Text recognition** engine. |
| Readable non-subtitle text is ignored | Normal mode filters for subtitle-like text. Turn on **Translate any text** only when that is your intended use. It does not improve recognition itself. |
| The same line is translated twice | The subtitle disappeared briefly, or recognition returned an empty frame, between two readings. Keep the area tight around the subtitles. |
| Translations come out garbled, or the PC stalls while the engine runs | Choose **Profile → Compatibility**, or turn on **Advanced → Run the engine on CPU only**. The engine restarts on CPU. Include the support code and your GPU and driver version in a report. |
| Translation feels slow or lags behind | Allow for the initial warm-up. Close CPU- or memory-heavy apps; on a busy PC CPU translation can slow down sharply. Keep the area small. |
| Video capture is blank or incomplete | Protected content or a player's display mode may not be capturable. Home can show **Compatibility capture · Protected video may not be read**. Test with a non-protected local clip in a normal window; this app is not a capture-protection bypass. |
| The app says it is up to date | Check the installed version against the application release, not a `core-v...` runtime tag. A merged code change is not necessarily a released update. |

For update checks, open **Settings → Engine and updates → Check for updates**.
Automatic checks occur at startup at most once per day. Checking does not
itself download an application update; **Download and install** does.

**Not yet released:** Available updates show up to three formatted highlights
under **What's new**, without a separate scroll box. **Full release notes ↗**
opens that version's complete GitHub release in your browser. The link remains
available when the update has no summary.

## Logs and diagnostics

The latest release writes one session log per launch and removes `.log` files
older than seven days at startup. The changes below are on `main` and have not
shipped in a release yet.

**Settings > Local logs** offers three levels:

- **Off** stops new diagnostic events and capture evidence after settings load.
- **Basic** (default) records operational diagnostics, without capture images or
  subtitle text.
- **Verbose** also saves the original captured region, actual OCR input PNGs,
  raw OCR text and geometry, and source/translated text locally. Selecting it is
  explicit consent to retain screen content that can be private. No upload occurs.

Session files use `meowcal-sub_<dev|prod>_<date>_<time>_<pid>.log` and mark each
entry `[dev]` or `[prod]`. The installed direct app uses
`%APPDATA%\com.meowcal.sub\logs`; paste that path into File Explorer.
The existing seven-day startup cleanup still applies to ordinary `.log` files.
Verbose evidence lives in `logs\verbose`, separately capped at **256 MiB**.
Before each new evidence record, the app removes its oldest complete or partial
records as needed. It does not delete unrelated files. A record larger than the
cap, a writer lock held for more than two seconds, an unexpected file, or a write failure skips evidence without
stopping translation. Verbose has no separate age expiry. Turning it off stops
new evidence but does not erase existing records; remove the `verbose` folder
when you no longer need its private contents. Review files before sharing them.
Saving PNGs can slow capture; use Verbose for short troubleshooting sessions.

Default logs record timings, character counts, queue depth, engine state,
support codes, and error codes. They are designed to omit subtitle text and
translations. Still read a log before you share it.

Two diagnostics can expose screen content, so use them only for local
investigation:

- **Settings → Developer options → Developer mode → Show subtitle diagnostics**
  adds recognition details beside the live subtitles. Screenshots taken while
  it is on show that detail.
- The environment variable `MEOWCAL_BAND_LOG` writes a local JSONL file of
  subtitle-area decisions without text. Adding `MEOWCAL_BAND_LOG_TEXT=1`
  records the **verbatim recognized text** as well. Never publish those files.

## Report a problem safely

1. Note the app version (**Settings → Engine and updates**), Windows build
   (`winver`), and whether the PC is x64 or ARM64.
2. Note the source and target languages, the **Profile**, and the
   **Text recognition** engine.
3. Copy the support code if Home or Settings shows one.
4. Write the steps that reproduce the problem with a video you can describe
   without sharing it.
5. If a log helps, copy only the relevant lines and remove any text you do not
   want to share.

Use the [issue chooser](https://github.com/PeterShanxin/Meowcal-Sub/issues/new/choose).
Do not post captured subtitle text, screenshots of what you were watching, API
keys, or raw logs containing them. A screenshot of the overlay's own controls is
fine. Report vulnerabilities privately using [SECURITY.md](../SECURITY.md).

To recover from a stuck state, try these in order: **Stop translation** or
**Exit translation**; **Repair engine**; restart the app; choose
**Profile → Compatibility**.

## Frequently asked questions

### Can it create subtitles for a video that has none?

No. This app reads screen text with OCR; it does not transcribe audio.
For subtitle-file search and playback-aligned sessions, see
[Meowcal Sub 2](https://github.com/PeterShanxin/Meowcal-Sub-2), a separate project.

### Does it work offline?

OCR and translation can run offline after the local engine is installed, and
after the Windows recognition language is installed when Windows OCR is used.
Setup, repair downloads, missing Windows language components, and update
checks/downloads use the network. Your video service has its own connectivity
requirements.

### How does GPU support differ by architecture?

**ARM64:** Adreno acceleration is gated to validated hardware/driver combinations.
Other configurations take the CPU policy. A GPU readiness timeout can trigger
one CPU retry within the existing startup deadline.

**x64:** the shipped runtime uses Vulkan on a discrete GPU. A PC with only an
integrated GPU, or none, runs the engine on CPU: the one integrated GPU that was
measured was slower than the CPU. A GPU readiness timeout or a failed sample
translation triggers one CPU retry within the existing startup deadline. A
discrete GPU without a working Vulkan driver can still run on CPU while the
app shows GPU.

**On either architecture**, **Settings → Advanced → Run the engine on CPU only**
(part of the **Compatibility** profile) starts the engine without GPU offload.
Changing it restarts the engine. Translation can be slower on CPU.

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
