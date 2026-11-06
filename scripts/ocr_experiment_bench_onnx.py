"""Time PP-OCR det/rec ONNX inference on subtitle-band-sized inputs (no pre/post-processing)."""
import platform, statistics, sys, time
import numpy as np, onnxruntime as ort
cases = [("PP-OCRv6_det_small.onnx", (1, 3, 96, 1120)), ("PP-OCRv6_rec_small.onnx", (1, 3, 48, 640)),
         ("PP-OCRv6_det_medium.onnx", (1, 3, 96, 1120)), ("PP-OCRv6_rec_medium.onnx", (1, 3, 48, 640)),
         ("ch_PP-OCRv5_det_mobile.onnx", (1, 3, 96, 1120)), ("ch_PP-OCRv5_rec_mobile.onnx", (1, 3, 48, 640))]
M = sys.argv[1].rstrip("/\\") + "/"   # RapidOCR's downloaded models directory
threads = int(sys.argv[2]) if len(sys.argv) > 2 else 4
print(platform.python_version(), platform.machine(), "ort", ort.__version__, "threads", threads)
for name, shape in cases:
    so = ort.SessionOptions(); so.intra_op_num_threads = threads
    try:
        s = ort.InferenceSession(M + name, so, providers=["CPUExecutionProvider"])
    except Exception as e:
        print(name, "load failed", e); continue
    x = np.random.rand(*shape).astype(np.float32)
    feed = {s.get_inputs()[0].name: x}
    for _ in range(3): s.run(None, feed)
    ts = []
    for _ in range(20):
        t = time.perf_counter(); s.run(None, feed); ts.append((time.perf_counter() - t) * 1000)
    ts.sort()
    print(f"{name:32s} p50 {statistics.median(ts):7.1f} ms  p95 {ts[18]:7.1f} ms")
