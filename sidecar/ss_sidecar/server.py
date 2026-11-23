"""Line-delimited JSON-RPC 2.0 server: one reader (this thread), MAX_IN_FLIGHT workers, one locked writer.

stdout carries protocol lines only: `__main__` hands this class the original stdout as a private binary stream and
points fd 1 / sys.stdout at stderr before anything else is imported. Requests may be pipelined and are answered in
any order (matched by id); `$/cancel` marks an in-flight request so its handler stops at the next item with -32004.
"""
from __future__ import annotations

import json
import logging
import threading
from concurrent.futures import ThreadPoolExecutor
from typing import BinaryIO

from . import config
from .context import RequestContext, SidecarState
from .errors import INTERNAL, INVALID_REQUEST, METHOD_NOT_FOUND, PARSE_ERROR, RpcError
from .methods import METHODS, Method
from .schema import ContractSchema

log = logging.getLogger("ss_sidecar.server")
BOM = b"\xef\xbb\xbf"


def is_request_id(value: object) -> bool:
    return isinstance(value, int) and not isinstance(value, bool) and value >= 0


class Server:
    def __init__(self, out: BinaryIO, inp: BinaryIO, schema: ContractSchema | None = None):
        self._out = out
        self._in = inp
        self._schema = schema or ContractSchema()
        self._state = SidecarState()
        self._pool = ThreadPoolExecutor(max_workers=config.MAX_IN_FLIGHT, thread_name_prefix="rpc")
        self._write_lock = threading.Lock()
        self._inflight: dict[int, RequestContext] = {}
        self._inflight_lock = threading.Lock()
        self._exit = threading.Event()

    # --- writer -------------------------------------------------------------------------------------------------
    def send(self, message: dict) -> None:
        data = (json.dumps(message, ensure_ascii=False, separators=(",", ":"), allow_nan=False) + "\n").encode()
        with self._write_lock:
            view = memoryview(data)
            while view:
                view = view[self._out.write(view) :]
            self._out.flush()

    def notify(self, method: str, params: dict) -> None:
        self.send({"jsonrpc": "2.0", "method": method, "params": params})

    def reply(self, request_id: int, result: dict) -> None:
        self.send({"jsonrpc": "2.0", "id": request_id, "result": result})

    def fail(self, request_id: int | None, error: RpcError) -> None:
        self.send({"jsonrpc": "2.0", "id": request_id, "error": error.to_json()})

    # --- reader -------------------------------------------------------------------------------------------------
    def serve(self) -> int:
        """Read requests until `shutdown` or EOF (the parent went away). Returns the exit code."""
        if self._schema.error:
            log.error("%s; every request will answer -32000", self._schema.error)
        log.info("sidecar %s ready for requests", config.SIDECAR_VERSION)
        for raw in iter(self._in.readline, b""):
            line = raw.strip().removeprefix(BOM)
            if line:
                self.handle_line(line)
            if self._exit.is_set():
                break
        else:
            log.info("stdin closed; exiting")
        self._pool.shutdown(wait=False, cancel_futures=True)
        return 0

    def handle_line(self, line: bytes) -> None:
        try:
            msg = json.loads(line.decode("utf-8"))
        except (UnicodeDecodeError, ValueError) as e:
            self.fail(None, RpcError(PARSE_ERROR, str(e)))
            return
        if not isinstance(msg, dict) or msg.get("jsonrpc") != "2.0":
            rid = msg.get("id") if isinstance(msg, dict) and is_request_id(msg.get("id")) else None
            self.fail(rid, RpcError(INVALID_REQUEST, 'expected one JSON object with "jsonrpc": "2.0"'))
            return
        method, params = msg.get("method"), msg.get("params", {})
        if "id" not in msg:
            self._notification(method, params)
            return
        rid = msg["id"]
        if not is_request_id(rid):
            self.fail(None, RpcError(INVALID_REQUEST, "id must be a non-negative integer"))
            return
        if not isinstance(method, str):
            self.fail(rid, RpcError(INVALID_REQUEST, "method must be a string"))
            return
        spec = METHODS.get(method)
        if spec is None:
            self.fail(rid, RpcError(METHOD_NOT_FOUND, f"unknown method {method!r}"))
            return
        try:
            self._schema.check(spec.params_def, params)
        except RpcError as e:
            self.fail(rid, e)
            return
        self._dispatch(rid, spec, params)

    def _dispatch(self, rid: int, spec: Method, params: dict) -> None:
        ctx = RequestContext(rid, self.notify)
        if spec.exits:
            with self._inflight_lock:
                for other in self._inflight.values():
                    other.cancelled.set()
            self.reply(rid, spec.handler(ctx, params, self._state))
            self._exit.set()
            return
        with self._inflight_lock:
            if rid in self._inflight:
                self.fail(rid, RpcError(INVALID_REQUEST, f"id {rid} is already in flight"))
                return
            self._inflight[rid] = ctx
        self._pool.submit(self._run, ctx, spec, params)

    def _run(self, ctx: RequestContext, spec: Method, params: dict) -> None:
        try:
            ctx.check()
            self.reply(ctx.id, spec.handler(ctx, params, self._state))
        except RpcError as e:
            self.fail(ctx.id, e)
        except Exception as e:  # noqa: BLE001 - a handler bug answers -32000 instead of killing the worker
            log.exception("request %s failed", ctx.id)
            self.fail(ctx.id, RpcError(INTERNAL, f"{type(e).__name__}: {e}"))
        finally:
            with self._inflight_lock:
                self._inflight.pop(ctx.id, None)

    def _notification(self, method: object, params: object) -> None:
        if method != "$/cancel":
            log.warning("ignoring unknown notification %r", method)
            return
        target = params.get("id") if isinstance(params, dict) else None
        with self._inflight_lock:
            ctx = self._inflight.get(target) if is_request_id(target) else None
        if ctx is None:
            log.info("$/cancel for %r: not in flight (already answered or unknown)", target)
            return
        ctx.cancelled.set()
        log.info("request %s cancelled", target)
