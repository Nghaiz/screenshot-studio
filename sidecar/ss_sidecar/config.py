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

# OCR: RapidOCR defaults (PP-OCRv6 small det/rec, bundled in the wheel; multilingual incl. vi + en), on CPU
# onnxruntime (Deferred decision #19). Its own logger writes to stderr; keep it to warnings.
OCR_LOG_LEVEL = "warning"

# Dominant colour: downscale long edge, palette size.
COLOR_SAMPLE_EDGE = 128
PALETTE_SIZE = 5

FAKE_EMBED_DIM = 16


def schema_path() -> Path:
    return Path(os.environ.get(ENV_SCHEMA) or DEFAULT_SCHEMA_PATH)


def fake_engines() -> bool:
    return os.environ.get(ENV_FAKE_ENGINES) == "1"
