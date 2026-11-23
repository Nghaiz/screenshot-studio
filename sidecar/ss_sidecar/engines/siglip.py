"""Embedder on open_clip SigLIP (default `ViT-B-16-SigLIP-i18n-256`, multilingual incl. Vietnamese).

Weights download into HF_HOME on first load (`python -m ss_sidecar --prefetch`). On CUDA the forward pass runs under
fp16 autocast; vectors are cast back to float32 and L2-normalized before they leave the engine.
"""
from __future__ import annotations

import contextlib
import threading
from typing import Sequence

from .. import config
from ..errors import ItemError
from .images import open_rgb


class SiglipEmbedder:
    def __init__(self, model_name: str, device: str) -> None:
        import numpy
        import open_clip
        import torch

        self._np = numpy
        spec = config.EMBED_MODELS[model_name]
        model, _, preprocess = open_clip.create_model_and_transforms(
            model_name, pretrained=spec["pretrained"], device=device
        )
        model.eval()
        self._torch = torch
        self._model = model
        self._preprocess = preprocess
        self._tokenizer = open_clip.get_tokenizer(model_name)
        self._logit_scale = float(model.logit_scale.exp().item())
        self._label_cache: dict[tuple[str, ...], object] = {}
        self._lock = threading.Lock()
        self.model = model_name
        self.device = device
        self.dim = int(self._encode_texts(["warm up"]).shape[1])
        if self.dim != spec["dim"]:
            raise RuntimeError(f"{model_name} produced dim {self.dim}, config.EMBED_MODELS says {spec['dim']}")

    def _autocast(self):
        if self.device == "cuda":
            return self._torch.autocast("cuda", dtype=self._torch.float16)
        return contextlib.nullcontext()

    def _finish(self, feats):
        feats = self._torch.nn.functional.normalize(feats.float(), dim=-1)
        return feats.cpu().numpy().astype("<f4")

    def _encode_images(self, images: list):
        batch = self._torch.stack([self._preprocess(im) for im in images]).to(self.device)
        with self._lock, self._torch.inference_mode(), self._autocast():
            return self._finish(self._model.encode_image(batch))

    def _encode_texts(self, texts: Sequence[str]):
        tokens = self._tokenizer(list(texts)).to(self.device)
        with self._lock, self._torch.inference_mode(), self._autocast():
            return self._finish(self._model.encode_text(tokens))

    def embed_images(self, paths: Sequence[str]) -> list[bytes | Exception]:
        out: list[bytes | Exception | None] = [None] * len(paths)
        images, slots = [], []
        for i, path in enumerate(paths):
            try:
                images.append(open_rgb(path))
                slots.append(i)
            except ItemError as e:
                out[i] = e
        if images:
            for i, vec in zip(slots, self._encode_images(images)):
                out[i] = vec.tobytes()
        return out  # type: ignore[return-value]

    def embed_texts(self, texts: Sequence[str]) -> list[bytes]:
        return [vec.tobytes() for vec in self._encode_texts(texts)]

    def classify(self, path: str, labels: Sequence[str]) -> list[tuple[str, float]]:
        image = self._encode_images([open_rgb(path)])[0]
        if not labels:
            return []
        key = tuple(labels)
        if key not in self._label_cache:
            self._label_cache[key] = self._encode_texts([config.CLASSIFY_TEMPLATE.format(l) for l in labels])
        logits = self._logit_scale * (self._label_cache[key] @ image)  # softmax over labels (SigLIP bias cancels)
        weights = self._np.exp(logits - logits.max())
        scores = [(label, float(w)) for label, w in zip(labels, weights / weights.sum())]
        return sorted(scores, key=lambda s: s[1], reverse=True)
