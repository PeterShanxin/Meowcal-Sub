# PP-OCRv6 small models

Core compiles these files into `meowcal-core.exe` for the `ppOcrV6` OCR engine.
They are PaddleOCR's PP-OCRv6 small detector and multilingual recognizer, as
converted to ONNX by RapidOCR 3.10.0, and are distributed under the Apache
License 2.0. See [`licenses/paddleocr`](../../../licenses/paddleocr/).

| File                          | Source                                                                                                        | SHA-256                                                            |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| `PP-OCRv6_det_small.onnx`     | `https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.10.0/onnx/PP-OCRv6/det/PP-OCRv6_det_small.onnx` | `090f04abcd9d9a7498bc4ebf677e4cb9bdce1fe4197ddb7e529f1ef44e1ff94f` |
| `PP-OCRv6_rec_small.onnx`     | `https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.10.0/onnx/PP-OCRv6/rec/PP-OCRv6_rec_small.onnx` | `6f327246b50388f3c176ae304bd95767ea6dc0c9ae92153ef8cbe210b3c14884` |
| `PP-OCRv6_rec_small_dict.txt` | The recognizer's `character` ONNX metadata entry, written out unchanged, one character per line               | `b5f2bfe2bdd9448429e3e82b51c789775d9b42f2403d082b00662eb77e401c5d` |

A test in `core/src/ocr/ppocr/tests.rs` pins the two model digests. Replacing a
model is a recognition behaviour change: rerun the stability comparison in
`docs/plans/2026-11-05-ocr-stability-experiment.md` before changing a digest.
