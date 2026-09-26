"""Generate desktop-snapshot fixtures for the selector benchmark.

Each fixture is a packed BGRA frame (what the capture backends return) plus a
PNG preview. Scenes model the moment a user opens the area selector: a video
playing full screen or in a window over flat desktop UI. Video frames are the
repository's cinematic background, upscaled and given mild Gaussian grain
(sigma 3) so they compress like decoded video rather than like a smooth upscale.

Output goes to ./fixtures (git-ignored; regenerate with this script).
"""
import json, pathlib, random
import numpy as np
from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parents[3]
OUT = pathlib.Path(__file__).resolve().parent / "fixtures"
OUT.mkdir(exist_ok=True)
rng = np.random.default_rng(20260926)

video_src = Image.open(ROOT / "src/assets/cinematic-background.png").convert("RGB")
home = Image.open(ROOT / "docs/assets/screenshot-home.png").convert("RGB")
overlay = Image.open(ROOT / "docs/assets/screenshot-overlay.png").convert("RGB")

def video_frame(w, h):
    frame = video_src.resize((w, h), Image.LANCZOS)
    arr = np.asarray(frame).astype(np.int16)
    arr += rng.normal(0, 3, arr.shape).round().astype(np.int16)
    frame = Image.fromarray(np.clip(arr, 0, 255).astype(np.uint8))
    # A burned-in subtitle strip near the bottom, as in the real scenario.
    sub = overlay.resize((int(w * 0.6), int(w * 0.6 * overlay.height / overlay.width)))
    frame.paste(sub, ((w - sub.width) // 2, int(h * 0.80)))
    return frame

def desktop(w, h, video_rect=None):
    img = Image.new("RGB", (w, h), (32, 36, 44))
    # Flat UI: taskbar and a couple of app windows.
    img.paste((20, 22, 28), (0, h - 48, w, h))
    img.paste(home, (40, 40))
    img.paste(home, (w - home.width - 60, 120))
    if video_rect:
        x, y, vw, vh = video_rect
        img.paste(video_frame(vw, vh), (x, y))
    return img

scenes = {
    "fullscreen-video-1080p": lambda: video_frame(1920, 1080),
    "windowed-video-1440p": lambda: desktop(2560, 1440, (320, 180, 1920, 1080)),
    "fullscreen-video-4k": lambda: video_frame(3840, 2160),
    "flat-desktop-1080p": lambda: desktop(1920, 1080),
}

manifest = []
for name, make in scenes.items():
    img = make()
    rgb = np.asarray(img)
    bgra = np.empty((img.height, img.width, 4), np.uint8)
    bgra[..., 0] = rgb[..., 2]; bgra[..., 1] = rgb[..., 1]; bgra[..., 2] = rgb[..., 0]; bgra[..., 3] = 255
    (OUT / f"{name}.bgra").write_bytes(bgra.tobytes())
    img.save(OUT / f"{name}.preview.png")
    manifest.append({"name": name, "width": img.width, "height": img.height, "file": f"{name}.bgra"})
    print(name, img.size)
(OUT / "fixtures.json").write_text(json.dumps(manifest, indent=2))
