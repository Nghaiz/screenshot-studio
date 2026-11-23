"""JSON-RPC method handlers and the method registry (data: name -> handler + params schema def)."""
from __future__ import annotations

import base64
import logging
from dataclasses import dataclass
from typing import Callable

from . import config
from .context import RequestContext, SidecarState
from .engines import load_engines
from .errors import CAPABILITY_DISABLED, INTERNAL, ItemError, RpcError
from .schema import def_name

log = logging.getLogger("ss_sidecar.methods")


def b64(vector: bytes) -> str:
    return base64.b64encode(vector).decode("ascii")


def item_error(exc: BaseException) -> dict:
    if isinstance(exc, ItemError):
        return exc.to_json()
    log.error("engine fault on one item: %s: %s", type(exc).__name__, exc, exc_info=exc)
    return {"code": "internal", "message": f"{type(exc).__name__}: {exc}"}


def per_item(ctx: RequestContext, items: list, ok: Callable, fail: Callable) -> list:
    """Run `ok(item)` for each item; a failing item becomes `fail(item, error)` and the batch goes on."""
    results = []
    for done, item in enumerate(items, 1):
        ctx.check()
        try:
            results.append(ok(item))
        except Exception as e:  # noqa: BLE001 - every item failure is reported in-band
            results.append(fail(item, item_error(e)))
        ctx.progress(done, len(items))
    return results


def initialize(ctx: RequestContext, p: dict, state: SidecarState) -> dict:
    key = (p["device"], p["embedModel"])
    with state.lock:
        if state.engines is None or state.key != key:  # a failed reload keeps the engines already loaded
            state.engines, state.key = load_engines(p["device"], p["embedModel"]), key
        e = state.engines
    return {
        "protocolVersion": config.PROTOCOL_VERSION,
        "sidecarVersion": config.SIDECAR_VERSION,
        "capabilities": e.capabilities(),
        "device": e.device,
        "embedModel": e.embed_model,
        "embedDim": e.embed_dim,
    }


def ocr_recognize(ctx: RequestContext, p: dict, state: SidecarState) -> dict:
    ocr = state.require("ocr").ocr

    def ok(item):
        lines = ocr.recognize(item["path"])
        return {"imageId": item["imageId"], "text": "\n".join(l["text"] for l in lines), "lines": lines, "error": None}

    def fail(item, err):
        return {"imageId": item["imageId"], "text": "", "lines": [], "error": err}

    return {"results": per_item(ctx, p["items"], ok, fail)}


def embed_images(ctx: RequestContext, p: dict, state: SidecarState) -> dict:
    embedder = state.require("embed").embedder
    items, results = p["items"], []
    for start in range(0, len(items), config.EMBED_BATCH):
        ctx.check()
        chunk = items[start : start + config.EMBED_BATCH]
        try:
            vectors = embedder.embed_images([i["path"] for i in chunk])
        except Exception as e:  # noqa: BLE001 - a failed forward pass fails its items, not the batch
            vectors = [e] * len(chunk)
        for item, v in zip(chunk, vectors):
            ok = isinstance(v, bytes)
            results.append({"imageId": item["imageId"], "vector": b64(v) if ok else None,
                            "error": None if ok else item_error(v)})
        ctx.progress(len(results), len(items))
    return {"model": embedder.model, "dim": embedder.dim, "results": results}


def embed_text(ctx: RequestContext, p: dict, state: SidecarState) -> dict:
    embedder = state.require("embed").embedder
    ctx.check()
    vectors = embedder.embed_texts(p["texts"]) if p["texts"] else []
    return {"model": embedder.model, "dim": embedder.dim, "vectors": [b64(v) for v in vectors]}


def classify_zero_shot(ctx: RequestContext, p: dict, state: SidecarState) -> dict:
    embedder = state.require("classify").embedder

    def ok(item):
        scores = embedder.classify(item["path"], p["labels"])
        return {"imageId": item["imageId"], "scores": [{"label": l, "score": round(s, 6)} for l, s in scores],
                "error": None}

    return {"results": per_item(ctx, p["items"], ok,
                                lambda item, err: {"imageId": item["imageId"], "scores": [], "error": err})}


def color_dominant(ctx: RequestContext, p: dict, state: SidecarState) -> dict:
    color = state.require("color").color

    def ok(item):
        rgb, palette = color.dominant(item["path"])
        return {"imageId": item["imageId"], "rgb": rgb, "palette": palette, "error": None}

    def fail(item, err):
        return {"imageId": item["imageId"], "rgb": None, "palette": [], "error": err}

    return {"results": per_item(ctx, p["items"], ok, fail)}


def pii_detect(ctx: RequestContext, p: dict, state: SidecarState) -> dict:
    engines = state.require("pii")
    lines = p["lines"]
    if lines is None:
        if engines.ocr is None:
            raise RpcError(CAPABILITY_DISABLED, "pii.detect with lines=null runs OCR, and capability 'ocr' is disabled")
        ctx.check()
        try:
            lines = engines.ocr.recognize(p["path"])
        except ItemError as e:
            raise RpcError(INTERNAL, f"{e.code}: {e.message}") from e
    return {"findings": engines.pii.detect(lines)}


def shutdown(ctx: RequestContext, p: dict, state: SidecarState) -> dict:
    return {}


@dataclass(frozen=True)
class Method:
    handler: Callable[[RequestContext, dict, SidecarState], dict]
    params_def: str
    exits: bool = False  # the server answers, then the process exits 0


def _m(name: str, handler: Callable) -> tuple[str, Method]:
    return name, Method(handler, def_name(name, "Params"))


METHODS: dict[str, Method] = dict([
    _m("initialize", initialize),
    _m("ocr.recognize", ocr_recognize),
    _m("embed.images", embed_images),
    _m("embed.text", embed_text),
    _m("classify.zeroShot", classify_zero_shot),
    _m("color.dominant", color_dominant),
    _m("pii.detect", pii_detect),
    ("shutdown", Method(shutdown, "EmptyObject", exits=True)),
])
