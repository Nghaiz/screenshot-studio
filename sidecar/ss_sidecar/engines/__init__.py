"""Engine set behind the JSON-RPC methods: load real or fake engines, degrade per engine, report capabilities.

A real engine that fails to load is logged (stderr) and its capability is left out of `initialize`; the methods
that need it then answer -32003. Heavy libraries are imported only inside the engine constructors.
"""
from __future__ import annotations

import logging
from dataclasses import dataclass, field
from typing import Callable, Sequence, TypeVar

from .. import config, pii
from ..errors import DEVICE_UNAVAILABLE, INVALID_PARAMS, RpcError
from .base import ColorExtractor, Embedder, OcrEngine, PiiDetector

log = logging.getLogger("ss_sidecar.engines")
T = TypeVar("T")


class RegexPiiDetector:
    def detect(self, lines: Sequence[dict]) -> list[dict]:
        return pii.detect(list(lines))


@dataclass
class Engines:
    device: str
    embed_model: str
    embed_dim: int
    ocr: OcrEngine | None = None
    embedder: Embedder | None = None
    color: ColorExtractor | None = None
    pii: PiiDetector = field(default_factory=RegexPiiDetector)
    load_errors: dict[str, str] = field(default_factory=dict)

    def capabilities(self) -> list[str]:
        caps = []
        if self.ocr is not None:
            caps.append("ocr")
        if self.embedder is not None:
            caps += ["embed", "classify"]
        if self.color is not None:
            caps.append("color")
        caps.append("pii")  # regex over given lines; with `lines: null` it also needs "ocr" (checked per call)
        return caps


def resolve_device(requested: str, fake: bool) -> str:
    available = False
    if not fake:
        try:
            import torch

            available = bool(torch.cuda.is_available())
        except Exception as e:  # torch missing or broken: CPU, and say why
            log.warning("CUDA probe failed (%s: %s); using CPU", type(e).__name__, e)
    if requested == "cuda" and not available:
        raise RpcError(DEVICE_UNAVAILABLE, "device 'cuda' requested but torch.cuda.is_available() is False")
    return "cuda" if requested in ("auto", "cuda") and available else "cpu"


def ocr_engine_class():
    """The real OcrEngine class for `config.ocr_pipeline()` (data: SS_SIDECAR_OCR_PIPELINE)."""
    if config.ocr_pipeline() == "rapidocr+easyocr":
        from .ocr_easy import RapidOcrEasyToneEngine

        return RapidOcrEasyToneEngine
    from .ocr_rapid import RapidOcrEngine

    return RapidOcrEngine


def _attempt(name: str, errors: dict[str, str], factory: Callable[[], T]) -> T | None:
    try:
        return factory()
    except Exception as e:
        log.exception("engine %s failed to load; capability disabled", name)
        errors[name] = f"{type(e).__name__}: {e}"
        return None


def load_engines(device: str, embed_model: str, fake: bool | None = None) -> Engines:
    if embed_model not in config.EMBED_MODELS:
        raise RpcError(INVALID_PARAMS, f"unsupported embedModel {embed_model!r}; supported: {sorted(config.EMBED_MODELS)}")
    fake = config.fake_engines() if fake is None else fake
    resolved = resolve_device(device, fake)
    errors: dict[str, str] = {}
    if fake:
        from .fakes import FakeColor, FakeEmbedder, FakeOcr

        ocr = _attempt("ocr", errors, FakeOcr)
        embedder = _attempt("embed", errors, lambda: FakeEmbedder(embed_model, resolved))
        color = _attempt("color", errors, FakeColor)
        default_dim = config.FAKE_EMBED_DIM
    else:
        from .color import PillowColorExtractor
        from .siglip import SiglipEmbedder

        ocr = _attempt("ocr", errors, lambda: ocr_engine_class()(resolved))
        embedder = _attempt("embed", errors, lambda: SiglipEmbedder(embed_model, resolved))
        color = _attempt("color", errors, PillowColorExtractor)
        default_dim = config.EMBED_MODELS[embed_model]["dim"]
    return Engines(
        device=resolved,
        embed_model=embed_model,
        embed_dim=embedder.dim if embedder is not None else default_dim,
        ocr=ocr,
        embedder=embedder,
        color=color,
        load_errors=errors,
    )
