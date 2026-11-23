"""Deterministic stdlib-only fakes of every engine (SS_SIDECAR_FAKE_ENGINES=1): the contract test and CI run them.

They never open a file: an unsupported extension or a name containing "broken" stands in for a bad image. Like real
libraries they are noisy: every load and every OCR call writes to Python's stdout AND to file descriptor 1, so the
contract test proves the protocol channel survives both kinds of pollution.
"""
from __future__ import annotations

import hashlib
import math
import os
import struct
import time
from pathlib import Path
from typing import Sequence

from .. import config
from ..errors import ItemError

FAKE_LINES = (
    {"text": "Xin chào thế giới", "box": [12.0, 8.5, 240.0, 32.0], "score": 0.97},
    {"text": "mail me at someone@example.com", "box": [12.0, 40.0, 312.0, 64.0], "score": 0.99},
    {"text": "Screenshot Studio 2026", "box": [12.0, 72.0, 260.0, 96.0], "score": 0.98},  # tools.PROBE_TEXT
)
FAKE_PALETTE = ["#1E1E2E", "#CBA6F7", "#F38BA8"]


def noise(tag: str) -> None:
    print(f"[fake {tag}] chatter on sys.stdout")
    os.write(1, f"[fake {tag}] chatter on fd 1\n".encode())


def disabled(engine: str) -> bool:
    return engine in (os.environ.get(config.ENV_FAKE_DISABLE) or "").split(",")


def check(path: str) -> None:
    delay_ms = int(os.environ.get(config.ENV_FAKE_DELAY_MS) or 0)
    if delay_ms:
        time.sleep(delay_ms / 1000)
    p = Path(path)
    if p.suffix.lower() not in config.SUPPORTED_EXTENSIONS:
        raise ItemError("unsupported", "format not supported")
    if "broken" in p.name.lower():
        raise ItemError("unreadable", "cannot decode image")


def unit_vector(seed: str, dim: int) -> list[float]:
    raw = b""
    counter = 0
    while len(raw) < dim * 4:
        raw += hashlib.sha256(f"{seed}#{counter}".encode()).digest()
        counter += 1
    values = [(b - 127.5) for b in raw[:dim]]
    norm = math.sqrt(sum(v * v for v in values)) or 1.0
    return [v / norm for v in values]


def pack(values: list[float]) -> bytes:
    return struct.pack(f"<{len(values)}f", *values)


class FakeOcr:
    name = "fake"

    def __init__(self) -> None:
        if disabled("ocr"):
            raise RuntimeError("fake OCR disabled by SS_SIDECAR_FAKE_DISABLE")
        noise("ocr load")

    def recognize(self, path: str) -> list[dict]:
        check(path)
        noise("ocr")
        return [dict(line, box=list(line["box"])) for line in FAKE_LINES]


class FakeEmbedder:
    def __init__(self, model_name: str, device: str) -> None:
        if disabled("embed"):
            raise RuntimeError("fake embedder disabled by SS_SIDECAR_FAKE_DISABLE")
        noise("embed load")
        self.model = model_name
        self.device = device
        self.dim = config.FAKE_EMBED_DIM

    def embed_images(self, paths: Sequence[str]) -> list[bytes | Exception]:
        out: list[bytes | Exception] = []
        for path in paths:
            try:
                check(path)
                out.append(pack(unit_vector(f"image:{path}", self.dim)))
            except ItemError as e:
                out.append(e)
        return out

    def embed_texts(self, texts: Sequence[str]) -> list[bytes]:
        return [pack(unit_vector(f"text:{t}", self.dim)) for t in texts]

    def classify(self, path: str, labels: Sequence[str]) -> list[tuple[str, float]]:
        check(path)
        digest = (hashlib.sha256(f"{path}|{label}".encode()).digest() for label in labels)
        weights = [math.exp(4 * int.from_bytes(d[:4], "little") / 2**32) for d in digest]
        total = sum(weights) or 1.0
        return sorted(((l, w / total) for l, w in zip(labels, weights)), key=lambda s: s[1], reverse=True)


class FakeColor:
    def __init__(self) -> None:
        if disabled("color"):
            raise RuntimeError("fake colour disabled by SS_SIDECAR_FAKE_DISABLE")

    def dominant(self, path: str) -> tuple[str, list[str]]:
        check(path)
        return FAKE_PALETTE[0], list(FAKE_PALETTE)
