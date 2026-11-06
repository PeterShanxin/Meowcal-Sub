"""Run one OCR engine over a corpus's frames in timestamp order; write per-frame JSONL.

No stabilization, dedup or translation. `winocr-mask` reproduces the application's
white-glyph preprocessing; every other engine is a single raw pass. Each line records
its text, left edge `x`, vertical centre `cy` and height `h` in frame pixels.
"""
import argparse
import asyncio
import json
import time
from pathlib import Path

from PIL import Image


def windows_engine(lang):
    from winrt.windows.globalization import Language
    from winrt.windows.graphics.imaging import BitmapPixelFormat, SoftwareBitmap
    from winrt.windows.media.ocr import OcrEngine
    from winrt.windows.storage.streams import DataWriter

    tag = {"zh": "zh-Hans-CN", "en": "en-US"}[lang]
    engine = OcrEngine.try_create_from_language(Language(tag))
    loop = asyncio.new_event_loop()

    async def recognize(img):
        writer = DataWriter()
        writer.write_bytes(img.tobytes())
        bitmap = SoftwareBitmap.create_copy_from_buffer(
            writer.detach_buffer(), BitmapPixelFormat.RGBA8, img.width, img.height)
        return await engine.recognize_async(bitmap)

    def run(img):
        result = loop.run_until_complete(recognize(img.convert("RGBA")))
        out = []
        for line in result.lines:
            rects = [w.bounding_rect for w in line.words]
            top = min(r.y for r in rects)
            bottom = max(r.y + r.height for r in rects)
            out.append({"text": line.text, "x": min(r.x for r in rects),
                        "cy": (top + bottom) / 2, "h": bottom - top})
        return out
    return run


def reading_order(lines):
    """Top to bottom; boxes whose centres lie within half a line height form one row, left to right."""
    rows = []
    for line in sorted(lines, key=lambda item: item["cy"]):
        if rows and abs(line["cy"] - rows[-1][0]["cy"]) < 0.5 * min(line["h"], rows[-1][0]["h"]):
            rows[-1].append(line)
        else:
            rows.append([line])
    return [line for row in rows for line in sorted(row, key=lambda item: item["x"])]


def rapid_engine(version, size):
    from rapidocr import ModelType, OCRVersion, RapidOCR

    ver = {"v4": OCRVersion.PPOCRV4, "v5": OCRVersion.PPOCRV5, "v6": OCRVersion.PPOCRV6}[version]
    mt = ModelType(size)
    ocr = RapidOCR(params={
        "Global.use_cls": False, "Global.log_level": "error",
        "Det.limit_type": "max", "Det.limit_side_len": 1440,
        "EngineConfig.onnxruntime.intra_op_num_threads": 4,
        "Det.ocr_version": ver, "Det.model_type": mt,
        "Rec.ocr_version": ver, "Rec.model_type": mt,
    })

    def run(img):
        out = ocr(img.convert("RGB"))
        if out.txts is None:
            return []
        lines = []
        for box, text in zip(out.boxes.tolist(), out.txts):
            ys = [p[1] for p in box]
            lines.append({"text": text, "x": min(p[0] for p in box),
                          "cy": (min(ys) + max(ys)) / 2, "h": max(ys) - min(ys)})
        return reading_order(lines)
    return run


def white_glyph_mask(img, margin=24):
    """App subtitle path (glyph_mask.rs): bright unsaturated pixels black on white, plus margin."""
    import numpy as np
    a = np.asarray(img.convert("RGB")).astype(np.int16)
    glyph = (a.min(axis=2) >= 200) & ((a.max(axis=2) - a.min(axis=2)) <= 40)
    page = np.full((a.shape[0] + 2 * margin, a.shape[1] + 2 * margin), 255, np.uint8)
    page[margin:-margin, margin:-margin][glyph] = 0
    return Image.fromarray(page).convert("RGB")


def windows_masked_engine(lang):
    """Mask first; fall back to the raw frame when the mask yields no text (recognition_mode.rs)."""
    raw = windows_engine(lang)

    def run(img):
        lines = raw(white_glyph_mask(img))
        if any(ch.isalnum() for line in lines for ch in line["text"]):
            return [{**line, "x": line["x"] - 24, "cy": line["cy"] - 24} for line in lines]
        return raw(img)
    return run


def make(name, lang):
    if name == "winocr":
        return windows_engine(lang)
    if name == "winocr-mask":
        return windows_masked_engine(lang)
    _, version, size = name.split("-")   # rapid-<version>-<size>, e.g. rapid-v6-small
    return rapid_engine(version, size)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("corpus", type=Path)
    p.add_argument("engine")
    p.add_argument("--lang", required=True, choices=["zh", "en"])
    p.add_argument("--limit", type=int)
    p.add_argument("--tag", default="", help="suffix for the output file name")
    args = p.parse_args()
    frames = sorted((args.corpus / "frames").glob("*.png"))[: args.limit]
    out_dir = args.corpus / "ocr"
    out_dir.mkdir(exist_ok=True)
    started = time.perf_counter()
    run = make(args.engine, args.lang)
    run(Image.open(frames[0]))           # warmup, excluded from timings
    load_s = time.perf_counter() - started
    with (out_dir / f"{args.engine}{args.tag}.jsonl").open("w", encoding="utf-8") as f:
        for i, path in enumerate(frames):
            img = Image.open(path)
            t0 = time.perf_counter()
            try:
                lines, error = run(img), None
            except Exception as exc:  # recorded per frame, never dropped
                lines, error = [], repr(exc)
            ms = (time.perf_counter() - t0) * 1000
            f.write(json.dumps({"frame": i, "lines": lines, "ms": round(ms, 2), "error": error},
                               ensure_ascii=False) + "\n")
    print(json.dumps({"engine": args.engine, "frames": len(frames), "load_and_warmup_s": round(load_s, 2)}))


if __name__ == "__main__":
    main()
