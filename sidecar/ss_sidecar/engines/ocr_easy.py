"""Default OcrEngine "rapidocr+easyocr" (phase 3.1b): RapidOCR reads every line, EasyOCR restores the letters
RapidOCR's recognizer cannot emit.

RapidOCR's PP-OCRv6 dictionary lacks 44 tone-marked Vietnamese letters (`ế ậ ờ`... and capitals), so they vanish
(`điều` -> `điu`). EasyOCR's latin_g2 model has all 134 Vietnamese letters but drops dots and slashes in emails,
URLs and file names. So RapidOCR's text is the base, EasyOCR reads the SAME line boxes, and only the differing
segments where EasyOCR carries one of those missing letters are taken from EasyOCR. Measured on the 5 hand-checked
Vietnamese screenshots (plans README, Deferred decision #19): 96.8% character accuracy vs 87.4%, tone letters
205/219 vs 0/219, English and punctuation unchanged. EasyOCR weights: `<HF_HOME>/easyocr`, downloaded by `--prefetch`.
"""
from __future__ import annotations

import difflib
import threading
import unicodedata

from .. import config
from .base import OcrLine
from .images import open_rgb
from .ocr_rapid import make_line, new_rapidocr, warm_up_image

_VOWELS = "aăâeêioôơuưy"
_MARKS = ("", "̀", "́", "̃", "̉", "̣")  # none, grave, acute, tilde, hook above, dot below
_LOWER = {unicodedata.normalize("NFC", v + m) for v in _VOWELS for m in _MARKS} - set("aeiouy") | {"đ"}
VIETNAMESE_LETTERS = frozenset(_LOWER | {c.upper() for c in _LOWER})  # the 134 non-ASCII Vietnamese letters


def merge_recovered(base: str, other: str | None, recovered: frozenset[str]) -> str:
    """Merge two readings of one line. Similar readings: `base`, with each differing segment taken from `other` when
    that segment has a `recovered` letter. Dissimilar readings (a line dense in recovered letters can derail the
    base recognizer entirely): `other` if it has any recovered letter, else `base`."""
    if not other:
        return base
    base, other = unicodedata.normalize("NFC", base), unicodedata.normalize("NFC", other)
    matcher = difflib.SequenceMatcher(None, base, other, autojunk=False)
    if matcher.ratio() < config.OCR_MERGE_MIN_RATIO:
        return other if any(c in recovered for c in other) else base
    out = []
    for tag, i1, i2, j1, j2 in matcher.get_opcodes():
        segment = other[j1:j2]
        out.append(segment if tag != "equal" and any(c in recovered for c in segment) else base[i1:i2])
    return "".join(out)


class RapidOcrEasyToneEngine:
    name = "rapidocr+easyocr"

    def __init__(self, device: str, download: bool = False) -> None:
        import easyocr
        import numpy

        self._np = numpy
        self._rapid = new_rapidocr()
        self._rapid(warm_up_image())  # loads det/cls/rec, so the rec dictionary below exists
        rapid_letters = set(self._rapid.text_rec.postprocess_op.character)
        self.recovered = frozenset(VIETNAMESE_LETTERS - rapid_letters)
        store = str(config.model_dir(config.EASYOCR_MODEL_SUBDIR))
        self._reader = easyocr.Reader(
            config.EASYOCR_LANGS, gpu=device == "cuda", detector=False, model_storage_directory=store,
            user_network_directory=store, download_enabled=download, verbose=False,
        )
        self._lock = threading.Lock()
        self._read(warm_up_image())

    def _easy_texts(self, image, polys) -> dict[int, str]:
        """EasyOCR's reading of each RapidOCR box, by box index (EasyOCR echoes the clipped box it was given)."""
        width, height = image.size
        boxes, index = [], {}
        for i, poly in enumerate(polys):
            xs, ys = [int(p[0]) for p in poly], [int(p[1]) for p in poly]
            box = [max(0, min(xs)), min(max(xs), width), max(0, min(ys)), min(max(ys), height)]
            boxes.append(box)
            index[(box[0], box[2], box[1], box[3])] = i
        grey = self._np.asarray(image.convert("L"))
        results = self._reader.recognize(grey, horizontal_list=boxes, free_list=[], detail=1, paragraph=False,
                                         batch_size=config.OCR_REC_BATCH)
        texts = {}
        for points, text, _score in results:
            key = (int(points[0][0]), int(points[0][1]), int(points[2][0]), int(points[2][1]))
            if key in index:
                texts[index[key]] = text
        return texts

    def _read(self, image) -> list[OcrLine]:
        with self._lock:
            out = self._rapid(image)
            if out.txts is None or out.boxes is None or out.scores is None:
                return []
            easy = self._easy_texts(image, out.boxes) if self.recovered else {}
        return [make_line(poly, merge_recovered(text, easy.get(i), self.recovered), score)
                for i, (poly, text, score) in enumerate(zip(out.boxes, out.txts, out.scores))]

    def recognize(self, path: str) -> list[OcrLine]:
        return self._read(open_rgb(path))
