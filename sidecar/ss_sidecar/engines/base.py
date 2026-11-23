"""Small engine interfaces the JSON-RPC methods depend on (phase 3.1 step 4). Real and fake engines implement them.

Every per-image call raises `errors.ItemError` for a bad item (unreadable/unsupported); any other exception is an
engine fault that the method layer reports in-band as an `internal` item error.
"""
from __future__ import annotations

from typing import Protocol, Sequence, TypedDict


class OcrLine(TypedDict):
    text: str
    box: list[float]  # [x1, y1, x2, y2], image pixels
    score: float


class OcrEngine(Protocol):
    name: str

    def recognize(self, path: str) -> list[OcrLine]: ...


class Embedder(Protocol):
    model: str
    dim: int
    device: str  # "cuda" | "cpu"

    def embed_images(self, paths: Sequence[str]) -> list[bytes | Exception]:
        """One little-endian float32 L2-normalized vector (bytes) or the item's exception, per path, in order."""
        ...

    def embed_texts(self, texts: Sequence[str]) -> list[bytes]: ...

    def classify(self, path: str, labels: Sequence[str]) -> list[tuple[str, float]]:
        """(label, score) for every label, descending by score; scores sum to 1."""
        ...


class ColorExtractor(Protocol):
    def dominant(self, path: str) -> tuple[str, list[str]]:
        """('#RRGGBB' most frequent colour, palette of '#RRGGBB' by frequency)."""
        ...


class PiiDetector(Protocol):
    def detect(self, lines: Sequence[dict]) -> list[dict]: ...
