"""Sidecar constants. One place for every tunable; nothing else inlines these literals."""
from __future__ import annotations

import os
from pathlib import Path

PROTOCOL_VERSION = 1
# Must equal [project].version in sidecar/pyproject.toml (test_contract.py guards the drift).
SIDECAR_VERSION = "0.1.0"

SIDECAR_DIR = Path(__file__).resolve().parents[1]

# Contract schema (SSOT for params validation). Default: the repo layout <repo>/contracts/...; a bundled app sets
# SS_CONTRACT_SCHEMA to wherever it ships the file.
ENV_SCHEMA = "SS_CONTRACT_SCHEMA"
DEFAULT_SCHEMA_PATH = SIDECAR_DIR.parent / "contracts" / "sidecar-jsonrpc.schema.json"

# Test seam: "1" swaps every model engine for a deterministic, stdlib-only fake (tests/test_contract.py, CI).
ENV_FAKE_ENGINES = "SS_SIDECAR_FAKE_ENGINES"
# Fake-only knobs: per-item delay (ms) so cancellation can be exercised; comma list of fake engines that fail to load.
ENV_FAKE_DELAY_MS = "SS_SIDECAR_FAKE_DELAY_MS"
ENV_FAKE_DISABLE = "SS_SIDECAR_FAKE_DISABLE"

# Worker threads == the most requests Rust keeps in flight (contract § Transport).
MAX_IN_FLIGHT = 2

# Extensions the sidecar decodes; mirrors ss-core `scan::EXTENSIONS` (the files the app indexes). Others: "unsupported".
SUPPORTED_EXTENSIONS = frozenset({".png", ".jpg", ".jpeg", ".webp", ".gif", ".bmp"})

# Embedding models: open_clip name -> pretrained tag + output dim. Default per Deferred decision #19.
DEFAULT_EMBED_MODEL = "ViT-B-16-SigLIP-i18n-256"
EMBED_MODELS = {
    "ViT-B-16-SigLIP-i18n-256": {"pretrained": "webli", "dim": 768},
}
EMBED_BATCH = 8  # images per forward pass inside one embed.images request (progress + cancel granularity)
CLASSIFY_TEMPLATE = "a screenshot of {}"  # zero-shot prompt per label; scores are a softmax over the labels

# OCR pipeline (Deferred decision #19), data, overridable with SS_SIDECAR_OCR_PIPELINE:
#   "rapidocr+easyocr" - RapidOCR (PP-OCRv6 det + rec, CPU onnxruntime) reads each line; EasyOCR latin_g2 (vi + en,
#                        torch on the initialize device) re-reads the same boxes to restore the tone-marked Vietnamese
#                        letters PP-OCRv6 cannot emit. Default since phase 3.1b.
#   "rapidocr"         - RapidOCR alone; drops those 44 letters (x2 cases).
ENV_OCR_PIPELINE = "SS_SIDECAR_OCR_PIPELINE"
DEFAULT_OCR_PIPELINE = "rapidocr+easyocr"
OCR_PIPELINES = ("rapidocr+easyocr", "rapidocr")
EASYOCR_LANGS = ["vi", "en"]
EASYOCR_MODEL_SUBDIR = "easyocr"  # under HF_HOME, next to the SigLIP weights
OCR_REC_BATCH = 8  # line crops per EasyOCR forward pass (GPU); 2-5x faster than 1 on the spike screenshots
# Below this difflib ratio the two readings of a line are not the same text: no segment merge (ocr_easy.py).
# Measured: same line with tone letters dropped ~0.9; a derailed RapidOCR reading ~0.2.
OCR_MERGE_MIN_RATIO = 0.6
OCR_LOG_LEVEL = "warning"  # RapidOCR's own logger (stderr)

# Dominant colour: downscale long edge, palette size.
COLOR_SAMPLE_EDGE = 128
PALETTE_SIZE = 5

FAKE_EMBED_DIM = 16


def schema_path() -> Path:
    return Path(os.environ.get(ENV_SCHEMA) or DEFAULT_SCHEMA_PATH)


def fake_engines() -> bool:
    return os.environ.get(ENV_FAKE_ENGINES) == "1"


def ocr_pipeline() -> str:
    name = os.environ.get(ENV_OCR_PIPELINE) or DEFAULT_OCR_PIPELINE
    if name not in OCR_PIPELINES:
        raise ValueError(f"{ENV_OCR_PIPELINE}={name!r}; expected one of {OCR_PIPELINES}")
    return name


def model_dir(subdir: str) -> Path:
    """Model cache under HF_HOME (the app sets it; huggingface_hub's own default otherwise)."""
    home = os.environ.get("HF_HOME") or str(Path.home() / ".cache" / "huggingface")
    return Path(home) / subdir
