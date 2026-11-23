"""REAL OCR (RapidOCR, PP-OCRv6 small, CPU) through the sidecar protocol, on generated images (D10: no owner files).

Local-only (needs the sidecar venv); never skips (D11). Text is compared after NFC normalization.
"""
from __future__ import annotations

import unicodedata

from conftest import MESSAGE, assert_valid, text_image


def nfc(s: str) -> str:
    return " ".join(unicodedata.normalize("NFC", s).split())


def ocr(sc, items: list[dict]) -> list[dict]:
    resp = sc.request("ocr.recognize", {"items": items}, timeout=120)
    assert "result" in resp, resp
    assert_valid(MESSAGE, resp, "ocr.recognize response")
    return resp["result"]["results"]


def test_initialize_reports_ocr(real_sidecar):
    _, init = real_sidecar
    assert "ocr" in init["capabilities"] and "pii" in init["capabilities"]


def test_recognises_english_and_vietnamese(real_sidecar, tmp_path):
    sc, _ = real_sidecar
    image = text_image(tmp_path / "mixed.png", ["Screenshot Studio invoice 2026", "Thông báo: đã lưu hóa đơn"])
    (result,) = ocr(sc, [{"imageId": 1, "path": str(image)}])
    assert result["error"] is None
    text = nfc(result["text"])
    for expected in ("Screenshot Studio", "invoice 2026", "Thông báo", "đã lưu", "hóa đơn"):
        assert nfc(expected) in text, f"{expected!r} not in OCR text {text!r}"
    width, height = 1280, 60 + 2 * int(44 * 1.8)
    for line in result["lines"]:
        x1, y1, x2, y2 = line["box"]
        assert 0 <= x1 < x2 <= width and 0 <= y1 < y2 <= height, line
        assert 0 < line["score"] <= 1, line


def test_vietnamese_tone_letters_known_gap(real_sidecar, tmp_path):
    """Absence pin (KnownGap): the PP-OCRv6 dictionary has no `ế`, `ớ` (44 tone letters), so they are dropped.

    If this goes RED the gap closed (a model with full Vietnamese coverage is in use): invert it into a positive
    assertion that "Xin chào thế giới" is read verbatim and update Deferred decision #19. Never re-pin or delete it.
    """
    sc, _ = real_sidecar
    image = text_image(tmp_path / "vi.png", ["Xin chào thế giới"])
    (result,) = ocr(sc, [{"imageId": 2, "path": str(image)}])
    text = nfc(result["text"])
    assert "Xin chào" in text, text
    assert "thế giới" not in text, "gap closed: see this test's docstring"


def test_bad_items_are_reported_in_band(real_sidecar, tmp_path):
    sc, _ = real_sidecar
    good = text_image(tmp_path / "good.png", ["Hello world"])
    corrupt = tmp_path / "corrupt.png"
    corrupt.write_bytes(b"this is not a PNG")
    tiff = tmp_path / "scan.tiff"
    tiff.write_bytes(b"II*\x00")
    items = [{"imageId": 10, "path": str(good)}, {"imageId": 11, "path": str(corrupt)},
             {"imageId": 12, "path": str(tmp_path / "missing.png")}, {"imageId": 13, "path": str(tiff)}]
    results = ocr(sc, items)
    assert [r["imageId"] for r in results] == [10, 11, 12, 13]
    assert [(r["error"] or {}).get("code") for r in results] == [None, "unreadable", "unreadable", "unsupported"]
    assert "Hello world" in nfc(results[0]["text"])


def test_pii_detect_runs_ocr_itself(real_sidecar, tmp_path):
    sc, _ = real_sidecar
    image = text_image(tmp_path / "pii.png", ["Contact: someone@example.com", "Phone 0912 345 678"])
    resp = sc.request("pii.detect", {"imageId": 3, "path": str(image), "lines": None}, timeout=120)
    assert "result" in resp, resp
    findings = {f["kind"]: f for f in resp["result"]["findings"]}
    assert findings.keys() == {"email", "phone"}, resp["result"]
    assert findings["email"]["text"] == "someone@example.com"
    assert findings["phone"]["text"].replace(" ", "") == "0912345678"
    x1, _, x2, _ = findings["email"]["box"]
    assert 40 < x1 < x2 <= 1280, findings["email"]
