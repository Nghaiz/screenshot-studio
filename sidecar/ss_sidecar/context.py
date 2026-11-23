"""Per-request context (cancellation + progress) and the shared engine state the method handlers read."""
from __future__ import annotations

import threading
from typing import Callable

from .engines import Engines
from .errors import CAPABILITY_DISABLED, MODEL_NOT_LOADED, Cancelled, RpcError

# Which engine's load error explains a missing capability.
CAPABILITY_ENGINE = {"ocr": "ocr", "embed": "embed", "classify": "embed", "color": "color", "pii": "pii"}


class RequestContext:
    def __init__(self, request_id: int, notify: Callable[[str, dict], None]):
        self.id = request_id
        self.cancelled = threading.Event()
        self._notify = notify

    def check(self) -> None:
        """Raise -32004 if Rust sent `$/cancel` for this request; handlers call it between items."""
        if self.cancelled.is_set():
            raise Cancelled(self.id)

    def progress(self, done: int, total: int) -> None:
        self._notify("progress", {"requestId": self.id, "done": done, "total": total})


class SidecarState:
    def __init__(self) -> None:
        self.engines: Engines | None = None
        self.key: tuple[str, str] | None = None
        self.lock = threading.Lock()

    def require(self, capability: str) -> Engines:
        engines = self.engines
        if engines is None:
            raise RpcError(MODEL_NOT_LOADED, "models are not loaded: send initialize first (or it is still running)")
        if capability not in engines.capabilities():
            reason = engines.load_errors.get(CAPABILITY_ENGINE[capability], "not available")
            raise RpcError(CAPABILITY_DISABLED, f"capability {capability!r} is disabled: {reason}")
        return engines
