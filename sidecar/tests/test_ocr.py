"""REAL OCR (RapidOCR + EasyOCR latin_g2 for tone-marked letters) through the sidecar protocol, on generated images
(D10: no owner files).

Local-only (needs the sidecar venv); never skips (D11). Text is compared after NFC normalization.
"""
from __future__ import annotations

import unicodedata

from conftest import INIT_PARAMS, MESSAGE, SIDECAR_INIT_TIMEOUT_S, assert_valid, text_image


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
    image = text_image(tmp_path / "mixed.png", ["Screenshot Studio invoice 2026", "Thông báo: đã lưu hóa đơn",
                                                         "Docs: https://www.example.com/v1.2 report.pdf"])
    (result,) = ocr(sc, [{"imageId": 1, "path": str(image)}])
    assert result["error"] is None
    text = nfc(result["text"])
    for expected in ("Screenshot Studio", "invoice 2026", "Thông báo", "đã lưu", "hóa đơn",
                     "https://www.example.com/v1.2 report.pdf"):
        assert nfc(expected) in text, f"{expected!r} not in OCR text {text!r}"
    width, height = 1280, 60 + 3 * int(44 * 1.8)
    for line in result["lines"]:
        x1, y1, x2, y2 = line["box"]
        assert 0 <= x1 < x2 <= width and 0 <= y1 < y2 <= height, line
        assert 0 < line["score"] <= 1, line


TONE_LINES = ["Xin chào thế giới", "Điều kiện cần và đủ", "Mật khẩu người dùng đã được cập nhật",
              "Ảnh chụp màn hình lưu ở thư mục Tải xuống"]


def test_vietnamese_tone_letters_are_recognised(real_sidecar, tmp_path):
    """Inverted from the phase 3.1 KnownGap pin: PP-OCRv6 alone dropped the 44 tone-marked letters (`ế ậ ờ`...).

    The default pipeline (RapidOCR + EasyOCR latin_g2 restoring those letters, Deferred decision #19) reads them. A failure
    here is a regression of Vietnamese OCR search: fix the pipeline, never re-pin the old gap.
    """
    sc, _ = real_sidecar
    image = text_image(tmp_path / "vi.png", TONE_LINES)
    (result,) = ocr(sc, [{"imageId": 2, "path": str(image)}])
    text = nfc(result["text"])
    for line in TONE_LINES:
        assert nfc(line) in text, f"{line!r} not read verbatim from {text!r}"


def test_rapidocr_only_pipeline_stays_selectable(spawn, tmp_path):
    """The OCR pipeline is data (SS_SIDECAR_OCR_PIPELINE): the RapidOCR-only pipeline must keep working."""
    sc = spawn(fake=False, env={"SS_SIDECAR_OCR_PIPELINE": "rapidocr"})
    init = sc.request("initialize", INIT_PARAMS, timeout=SIDECAR_INIT_TIMEOUT_S)
    assert "ocr" in init["result"]["capabilities"], init
    image = text_image(tmp_path / "en.png", ["Screenshot Studio invoice 2026"])
    (result,) = ocr(sc, [{"imageId": 4, "path": str(image)}])
    assert "Screenshot Studio invoice 2026" in nfc(result["text"])


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
