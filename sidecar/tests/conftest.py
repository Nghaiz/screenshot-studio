"""Shared test helpers: drive `python -m ss_sidecar` over real pipes and validate against the contract schema.

Imports only stdlib + jsonschema + pytest at module level: CI runs test_contract.py (fake engines) without the
model stack. Pillow is imported lazily by the image helpers the real-model tests use.
"""
from __future__ import annotations

import json
import os
import queue
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

import pytest
from jsonschema import Draft202012Validator

SIDECAR_DIR = Path(__file__).resolve().parents[1]
REPO = SIDECAR_DIR.parent
SCHEMA_PATH = REPO / "contracts" / "sidecar-jsonrpc.schema.json"
FIXTURES = REPO / "contracts" / "fixtures" / "sidecar"
FONT_DIR = Path(os.environ.get("WINDIR", r"C:\Windows")) / "Fonts"
SIDECAR_INIT_TIMEOUT_S = 120  # contract § Lifecycle: initialize must answer within this (model load)

sys.path.insert(0, str(SIDECAR_DIR))  # in-process imports of the pure modules (pii, schema, config)

SCHEMA = json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))
MESSAGE = Draft202012Validator(SCHEMA)


def validator(def_ref: str) -> Draft202012Validator:
    return Draft202012Validator({"$ref": f"#/$defs/{def_ref}", "$defs": SCHEMA["$defs"]})


def assert_valid(v: Draft202012Validator, instance: object, what: str) -> None:
    problems = [f"{'/'.join(map(str, e.absolute_path))}: {e.message}" for e in v.iter_errors(instance)]
    assert not problems, f"{what} violates the contract schema: {problems}\n{json.dumps(instance)[:2000]}"


class Sidecar:
    """One sidecar process. Every stdout line is kept (`raw_lines`) for the protocol-purity assertion."""

    def __init__(self, fake: bool, env: dict[str, str] | None = None):
        full_env = os.environ.copy()
        full_env.update(PYTHONUTF8="1", PYTHONUNBUFFERED="1")
        full_env.pop("SS_SIDECAR_FAKE_ENGINES", None)
        full_env.pop("SS_SIDECAR_OCR_PIPELINE", None)  # tests run the default OCR unless they pass `env`
        if fake:
            full_env["SS_SIDECAR_FAKE_ENGINES"] = "1"
        full_env.update(env or {})
        self._stderr = tempfile.TemporaryFile()
        self.proc = subprocess.Popen(
            [sys.executable, "-m", "ss_sidecar"], cwd=SIDECAR_DIR, env=full_env,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self._stderr,
        )
        self.raw_lines: list[bytes] = []
        self.notifications: list[dict] = []
        self._pending: dict[object, dict] = {}
        self._queue: queue.Queue = queue.Queue()
        self._next_id = 100
        self._reader = threading.Thread(target=self._read, daemon=True)
        self._reader.start()

    def wait_exit(self, timeout: float = 10) -> int:
        """Exit code, after the reader has seen stdout EOF (so `raw_lines` is complete)."""
        code = self.proc.wait(timeout)
        self._reader.join(timeout)
        assert not self._reader.is_alive(), "stdout still open after exit"
        return code

    def _read(self) -> None:
        for raw in self.proc.stdout:
            self.raw_lines.append(raw)
            try:
                self._queue.put(json.loads(raw))
            except ValueError:
                self._queue.put({"unparsable": raw.decode("utf-8", "replace")})
        self._queue.put(None)

    def send(self, message: object) -> None:
        self.send_raw(json.dumps(message, ensure_ascii=False).encode("utf-8") + b"\n")

    def send_raw(self, data: bytes) -> None:
        self.proc.stdin.write(data)
        self.proc.stdin.flush()

    def next_id(self) -> int:
        self._next_id += 1
        return self._next_id

    def _pump(self, deadline: float, waiting_for: str) -> dict:
        """Take the next stdout message; file responses by id and notifications in order."""
        try:
            msg = self._queue.get(timeout=max(0.01, deadline - time.monotonic()))
        except queue.Empty:
            raise AssertionError(f"timed out waiting for {waiting_for}; stderr tail:\n{self.stderr()[-3000:]}")
        if msg is None:
            raise AssertionError(f"sidecar stdout closed waiting for {waiting_for}; stderr:\n{self.stderr()[-3000:]}")
        if "id" in msg:
            self._pending[msg["id"]] = msg
        else:
            self.notifications.append(msg)
        return msg

    def answered(self, rid: object) -> bool:
        return rid in self._pending

    def response(self, rid: object, timeout: float = 30) -> dict:
        """The response with this id; notifications and other responses seen meanwhile are kept."""
        deadline = time.monotonic() + timeout
        while rid not in self._pending:
            self._pump(deadline, f"response id {rid}")
        return self._pending.pop(rid)

    def notification(self, predicate, timeout: float = 30) -> dict:
        """The first notification (already seen or new) matching the predicate."""
        deadline = time.monotonic() + timeout
        seen = next((n for n in self.notifications if predicate(n)), None)
        while seen is None:
            msg = self._pump(deadline, "a notification")
            seen = msg if "id" not in msg and predicate(msg) else None
        return seen

    def request(self, method: str, params: object, timeout: float = 30) -> dict:
        rid = self.next_id()
        self.send({"jsonrpc": "2.0", "id": rid, "method": method, "params": params})
        return self.response(rid, timeout)

    def stderr(self) -> str:
        self._stderr.seek(0)
        return self._stderr.read().decode("utf-8", "replace")

    def close(self) -> None:
        if self.proc.poll() is None:
            try:
                self.send({"jsonrpc": "2.0", "id": self.next_id(), "method": "shutdown", "params": {}})
                self.proc.wait(10)
            except (OSError, subprocess.TimeoutExpired):
                self.proc.kill()
                self.proc.wait(10)
        self._stderr.close()


INIT_PARAMS = {"protocolVersion": 1, "device": "auto", "languages": ["vi", "en"],
               "embedModel": "ViT-B-16-SigLIP-i18n-256", "ocrEngine": "rapidocr"}


@pytest.fixture
def spawn():
    """Factory: spawn(fake=True, env=None) -> Sidecar; every sidecar is shut down after the test."""
    started: list[Sidecar] = []

    def _spawn(fake: bool = True, env: dict[str, str] | None = None) -> Sidecar:
        started.append(Sidecar(fake, env))
        return started[-1]

    yield _spawn
    for sc in started:
        sc.close()


@pytest.fixture(scope="module")
def real_sidecar():
    """A REAL-model sidecar, initialized (device auto). Yields (sidecar, initialize result)."""
    sc = Sidecar(fake=False)
    try:
        resp = sc.request("initialize", INIT_PARAMS, timeout=SIDECAR_INIT_TIMEOUT_S)
        assert "result" in resp, f"initialize failed: {resp}\n{sc.stderr()[-3000:]}"
        yield sc, resp["result"]
    finally:
        sc.close()


def font(size: int, name: str = "segoeui.ttf"):
    """A Windows font with Vietnamese glyphs. Missing font = test error (tests never skip, D11)."""
    from PIL import ImageFont

    path = FONT_DIR / name
    if not path.is_file():
        raise FileNotFoundError(f"font with Vietnamese glyphs not found: {path}")
    return ImageFont.truetype(str(path), size)


def text_image(path: Path, lines: list[str], size: int = 44, width: int = 1280) -> Path:
    """Black text on white, one line per entry, generous margins (a clean screenshot)."""
    from PIL import Image, ImageDraw

    f = font(size)
    step = int(size * 1.8)
    image = Image.new("RGB", (width, 60 + step * len(lines)), "white")
    draw = ImageDraw.Draw(image)
    for i, line in enumerate(lines):
        draw.text((40, 30 + i * step), line, fill="black", font=f)
    image.save(path)
    return path
