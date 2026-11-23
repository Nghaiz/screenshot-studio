"""REAL image engines through the sidecar protocol: SigLIP i18n embeddings, zero-shot, dominant colour.

Generated images only (D10). Local-only (needs the sidecar venv + cached weights); never skips (D11).
"""
from __future__ import annotations

import base64
import math
import struct

import pytest
from conftest import MESSAGE, assert_valid, text_image


def vector(b64: str) -> list[float]:
    raw = base64.b64decode(b64)
    return list(struct.unpack(f"<{len(raw) // 4}f", raw))


def cos(a: list[float], b: list[float]) -> float:
    return sum(x * y for x, y in zip(a, b))  # both L2-normalized


def cat_image(path):
    """A cartoon cat face: orange head, ears, eyes, nose, whiskers."""
    from PIL import Image, ImageDraw

    image = Image.new("RGB", (512, 512), (235, 245, 255))
    d = ImageDraw.Draw(image)
    d.polygon([(120, 190), (150, 60), (230, 150)], fill=(230, 140, 50))
    d.polygon([(392, 190), (362, 60), (282, 150)], fill=(230, 140, 50))
    d.ellipse((96, 120, 416, 430), fill=(240, 150, 60))
    d.ellipse((176, 300, 336, 410), fill=(255, 235, 215))
    for cx in (196, 316):
        d.ellipse((cx - 32, 220, cx + 32, 280), fill=(120, 200, 90))
        d.ellipse((cx - 8, 222, cx + 8, 278), fill="black")
    d.polygon([(240, 310), (272, 310), (256, 330)], fill=(240, 120, 140))
    for dy in (-12, 0, 12):
        d.line((236, 335 + dy, 120, 320 + dy * 2), fill="black", width=3)
        d.line((276, 335 + dy, 392, 320 + dy * 2), fill="black", width=3)
    image.save(path)
    return path


@pytest.fixture(scope="module")
def images(tmp_path_factory):
    tmp = tmp_path_factory.mktemp("embed")
    doc = text_image(tmp / "doc.png", ["Quarterly report", "Revenue grew 12% year over year.",
                                       "Báo cáo doanh thu quý ba", "Total: 1,250,000 VND"], size=36)
    return {"cat": str(cat_image(tmp / "cat.png")), "doc": str(doc)}


@pytest.fixture(scope="module")
def embedded(real_sidecar, images):
    sc, init = real_sidecar
    resp = sc.request("embed.images", {"items": [{"imageId": 1, "path": images["cat"]},
                                                  {"imageId": 2, "path": images["doc"]}]}, timeout=120)
    assert "result" in resp, resp
    assert_valid(MESSAGE, resp, "embed.images response")
    texts = sc.request("embed.text", {"texts": ["cat", "con mèo", "a text document"]}, timeout=60)
    assert "result" in texts, texts
    return init, resp["result"], texts["result"]


def test_text_cat_is_closer_to_the_cat_image_than_to_a_text_screenshot(embedded):
    _, imgs, texts = embedded
    cat_img, doc_img = (vector(r["vector"]) for r in imgs["results"])
    cat_text, cat_vi, doc_text = (vector(v) for v in texts["vectors"])
    assert cos(cat_text, cat_img) > cos(cat_text, doc_img), (cos(cat_text, cat_img), cos(cat_text, doc_img))
    assert cos(cat_vi, cat_img) > cos(cat_vi, doc_img), "Vietnamese query 'con mèo' (i18n model)"
    assert cos(doc_text, doc_img) > cos(doc_text, cat_img)


def test_vectors_are_l2_normalized_with_embed_dim(embedded):
    init, imgs, texts = embedded
    assert init["embedDim"] == imgs["dim"] == texts["dim"] == 768
    assert init["embedModel"] == imgs["model"] == texts["model"] == "ViT-B-16-SigLIP-i18n-256"
    for v in [r["vector"] for r in imgs["results"]] + texts["vectors"]:
        values = vector(v)
        assert len(values) == init["embedDim"]
        assert abs(math.sqrt(sum(x * x for x in values)) - 1) < 1e-3


def test_initialize_reports_device(real_sidecar):
    _, init = real_sidecar
    assert init["device"] in ("cuda", "cpu") and {"embed", "classify", "color"} <= set(init["capabilities"])


def test_zero_shot_ranks_the_right_label_first(real_sidecar, images):
    sc, _ = real_sidecar
    resp = sc.request("classify.zeroShot", {"items": [{"imageId": 1, "path": images["cat"]},
                                                      {"imageId": 2, "path": images["doc"]}],
                                            "labels": ["cat", "document"]}, timeout=120)
    assert "result" in resp, resp
    cat, doc = resp["result"]["results"]
    assert cat["scores"][0]["label"] == "cat" and doc["scores"][0]["label"] == "document", resp["result"]
    assert abs(sum(s["score"] for s in cat["scores"]) - 1) < 1e-4


def test_dominant_colour(real_sidecar, tmp_path):
    from PIL import Image, ImageDraw

    path = tmp_path / "theme.png"
    image = Image.new("RGB", (800, 500), "#1E1E2E")
    ImageDraw.Draw(image).rectangle((40, 40, 240, 160), fill="#CBA6F7")
    image.save(path)
    sc, _ = real_sidecar
    resp = sc.request("color.dominant", {"items": [{"imageId": 5, "path": str(path)}]}, timeout=60)
    (result,) = resp["result"]["results"]
    assert result["error"] is None and result["rgb"] == "#1E1E2E", result
    assert "#CBA6F7" in result["palette"], result
