"""Whole-request JSON-RPC errors and per-item errors (contract § Errors, RpcItemError)."""
from __future__ import annotations

PARSE_ERROR = -32700
INVALID_REQUEST = -32600
METHOD_NOT_FOUND = -32601
INVALID_PARAMS = -32602
INTERNAL = -32000
MODEL_NOT_LOADED = -32001
DEVICE_UNAVAILABLE = -32002
CAPABILITY_DISABLED = -32003
CANCELLED = -32004

MESSAGES = {
    PARSE_ERROR: "parse error",
    INVALID_REQUEST: "invalid request",
    METHOD_NOT_FOUND: "method not found",
    INVALID_PARAMS: "invalid params",
    INTERNAL: "internal error",
    MODEL_NOT_LOADED: "model not loaded",
    DEVICE_UNAVAILABLE: "device unavailable",
    CAPABILITY_DISABLED: "capability disabled",
    CANCELLED: "cancelled",
}

ITEM_CODES = ("unreadable", "unsupported", "timeout", "internal")


class RpcError(Exception):
    """Fails the whole request; serialized as {code, message, data: {detail}}."""

    def __init__(self, code: int, detail: str | None = None):
        super().__init__(f"{code} {MESSAGES[code]}: {detail}")
        self.code = code
        self.detail = detail

    def to_json(self) -> dict:
        return {"code": self.code, "message": MESSAGES[self.code], "data": {"detail": self.detail}}


class Cancelled(RpcError):
    def __init__(self, request_id: int):
        super().__init__(CANCELLED, f"request {request_id} was cancelled")


class ItemError(Exception):
    """One item of a batch failed; the batch still returns every item with this in its `error` field."""

    def __init__(self, code: str, message: str):
        if code not in ITEM_CODES:
            raise ValueError(f"unknown item error code {code!r}")
        super().__init__(f"{code}: {message}")
        self.code = code
        self.message = message

    def to_json(self) -> dict:
        return {"code": self.code, "message": self.message}
