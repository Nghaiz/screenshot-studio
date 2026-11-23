"""OcrEngine on RapidOCR 3.x alone (onnxruntime CPU, PP-OCRv6 small multilingual det + rec bundled in the wheel).

Selected by `SS_SIDECAR_OCR_PIPELINE=rapidocr`; NOT the default. Phase 3.1 spike: 87.4% character accuracy on 5
hand-checked Vietnamese screenshots, but the PP-OCRv6 dictionary lacks the 44 tone-marked Vietnamese letters
(`ế ậ ờ`...), which vanish from the text. The default pipeline (`ocr_easy.py`) builds on this one.
"""
from __future__ import annotations

import threading
import unicodedata

from .. import config
from .base import OcrLine
from .images import open_rgb


def new_rapidocr():
    from rapidocr import RapidOCR

    return RapidOCR(params={"Global.log_level": config.OCR_LOG_LEVEL})


def warm_up_image():
    """A small text image: engines run it once so `initialize` owns the lazy model-load time."""
    from PIL import Image, ImageDraw, ImageFont

    image = Image.new("RGB", (360, 72), "white")
    ImageDraw.Draw(image).text((12, 12), "warm up 2026", fill="black", font=ImageFont.load_default(size=36))
    return image


def make_line(points, text: str, score: float) -> OcrLine:
    """OcrLine from a polygon (any number of [x, y] points): axis-aligned box, NFC text."""
    xs = [float(p[0]) for p in points]
    ys = [float(p[1]) for p in points]
    return {
        "text": unicodedata.normalize("NFC", text),
        "box": [round(min(xs), 1), round(min(ys), 1), round(max(xs), 1), round(max(ys), 1)],
        "score": round(float(score), 4),
    }


class RapidOcrEngine:
    name = "rapidocr"

    def __init__(self, device: str = "cpu", download: bool = False) -> None:
        del device, download  # CPU onnxruntime; models ship inside the rapidocr wheel
        self._engine = new_rapidocr()
        self._lock = threading.Lock()
        self._engine(warm_up_image())

    def recognize(self, path: str) -> list[OcrLine]:
        image = open_rgb(path)
        with self._lock:
            out = self._engine(image)
        if out.txts is None or out.boxes is None or out.scores is None:
            return []
        return [make_line(poly, text, score) for poly, text, score in zip(out.boxes, out.txts, out.scores)]
