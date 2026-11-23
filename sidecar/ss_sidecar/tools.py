"""`--selftest` and `--prefetch` (AX-01): one JSON line each on stdout, exit 0 on success, 1 otherwise."""
from __future__ import annotations

import logging
import math
import struct
import tempfile
from pathlib import Path

from . import config
from .engines import Engines, load_engines
from .errors import RpcError
from .schema import ContractSchema

log = logging.getLogger("ss_sidecar.tools")
PROBE_TEXT = "Screenshot Studio 2026"
PROBE_WORD = "Studio"


def write_probe(directory: str) -> str:
    from PIL import Image, ImageDraw, ImageFont

    path = Path(directory) / "selftest-probe.png"
    image = Image.new("RGB", (640, 120), "white")
    ImageDraw.Draw(image).text((20, 30), PROBE_TEXT, fill="black", font=ImageFont.load_default(size=48))
    image.save(path)
    return str(path)


def _check_vector(vector: bytes, dim: int, what: str) -> list[str]:
    values = struct.unpack(f"<{len(vector) // 4}f", vector)
    norm = math.sqrt(sum(v * v for v in values))
    problems = [] if len(values) == dim else [f"{what}: dim {len(values)} != {dim}"]
    return problems + ([] if abs(norm - 1) < 1e-3 else [f"{what}: L2 norm {norm:.4f} != 1"])


def _smoke(engines: Engines, probe: str) -> list[str]:
    errors = []
    if engines.ocr is not None:
        text = " ".join(line["text"] for line in engines.ocr.recognize(probe))
        if PROBE_WORD not in text:
            errors.append(f"ocr: probe text {PROBE_TEXT!r} read as {text!r}")
    if engines.embedder is not None:
        errors += _check_vector(engines.embedder.embed_texts(["cat"])[0], engines.embed_dim, "embed.text")
        image_vec = engines.embedder.embed_images([probe])[0]
        errors += _check_vector(image_vec, engines.embed_dim, "embed.images") if isinstance(image_vec, bytes) else [
            f"embed.images: {image_vec}"]
    if engines.color is not None:
        engines.color.dominant(probe)
    if not engines.pii.detect([{"text": "mail a@example.com", "box": [0, 0, 100, 10]}]):
        errors.append("pii: no finding for an email")
    return errors


def selftest(device: str, embed_model: str) -> dict:
    try:
        engines = load_engines(device, embed_model)
    except RpcError as e:
        return {"capabilities": [], "device": "cpu", "ok": False, "errors": [f"{e.code}: {e.detail}"]}
    errors = [f"{name}: {why}" for name, why in engines.load_errors.items()]
    schema = ContractSchema()
    if schema.error:
        errors.append(schema.error)
    try:
        with tempfile.TemporaryDirectory(prefix="ss-sidecar-") as tmp:
            errors += _smoke(engines, "selftest-probe.png" if config.fake_engines() else write_probe(tmp))
    except Exception as e:  # noqa: BLE001 - the selftest reports, it does not crash
        log.exception("selftest smoke failed")
        errors.append(f"smoke: {type(e).__name__}: {e}")
    return {"capabilities": engines.capabilities(), "device": engines.device, "ok": not errors, "errors": errors}


def prefetch(embed_model: str) -> dict:
    """Load every real model once so the weights land in HF_HOME (RapidOCR's own models ship inside its wheel)."""
    if embed_model not in config.EMBED_MODELS:
        return {"ok": False, "models": [], "errors": [f"unsupported embedModel {embed_model!r}"]}
    models, errors = [], []
    try:
        from .engines import ocr_engine_class

        engine = ocr_engine_class()("cpu", download=True)
        models.append(f"ocr:{engine.name}")
    except Exception as e:  # noqa: BLE001
        log.exception("OCR prefetch failed")
        errors.append(f"ocr: {type(e).__name__}: {e}")
    try:
        import open_clip

        pretrained = config.EMBED_MODELS[embed_model]["pretrained"]
        open_clip.create_model_and_transforms(embed_model, pretrained=pretrained)
        open_clip.get_tokenizer(embed_model)
        models.append(f"open_clip:{embed_model}/{pretrained}")
    except Exception as e:  # noqa: BLE001
        log.exception("embedding model prefetch failed")
        errors.append(f"embed: {type(e).__name__}: {e}")
    return {"ok": not errors, "models": models, "errors": errors}
