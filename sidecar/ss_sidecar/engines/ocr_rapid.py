"""OcrEngine on RapidOCR 3.x (onnxruntime CPU, PP-OCRv6 small multilingual det/rec bundled in the wheel).

Spike 2026-11-24 (plans README, Deferred decision #19): 87.4% character accuracy on 5 hand-checked Vietnamese
screenshots. Known gap: the PP-OCRv6 dictionary lacks the 44 tone-marked letters such as `ế`, `ậ`, `ờ` (they are
dropped from the output); test_ocr.py pins it.
"""
from __future__ import annotations

import threading
import unicodedata

from .. import config
from .base import OcrLine
from .images import open_rgb


class RapidOcrEngine:
    name = "rapidocr"

    def __init__(self) -> None:
        from rapidocr import RapidOCR

        self._engine = RapidOCR(params={"Global.log_level": config.OCR_LOG_LEVEL})
        self._lock = threading.Lock()
        self._warm_up()

    def _warm_up(self) -> None:
        """RapidOCR loads det/cls/rec lazily on first use; load them now so `initialize` owns the model-load time."""
        from PIL import Image, ImageDraw, ImageFont

        image = Image.new("RGB", (360, 72), "white")
        ImageDraw.Draw(image).text((12, 12), "warm up 2026", fill="black", font=ImageFont.load_default(size=36))
        self._engine(image)

    def recognize(self, path: str) -> list[OcrLine]:
        image = open_rgb(path)
        with self._lock:
            out = self._engine(image)
        if out.txts is None or out.boxes is None or out.scores is None:
            return []
        lines: list[OcrLine] = []
        for poly, text, score in zip(out.boxes, out.txts, out.scores):
            xs = [float(p[0]) for p in poly]
            ys = [float(p[1]) for p in poly]
            lines.append({
                "text": unicodedata.normalize("NFC", text),
                "box": [round(min(xs), 1), round(min(ys), 1), round(max(xs), 1), round(max(ys), 1)],
                "score": round(float(score), 4),
            })
        return lines
