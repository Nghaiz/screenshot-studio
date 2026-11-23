"""Contract tests with FAKE engines (stdlib-only; CI runs this file): protocol shape, errors, cancel, stdout purity.

Every test drives a real `python -m ss_sidecar` subprocess over pipes, exactly like the Rust supervisor will.
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
import time
import tomllib

import pytest
from conftest import FIXTURES, INIT_PARAMS, MESSAGE, SIDECAR_DIR, assert_valid, validator

from ss_sidecar import config, pii
from ss_sidecar.schema import def_name

ITEM = {"imageId": 1, "path": "C:\\shots\\a.png"}


def fixture_requests() -> list[dict]:
    reqs = [json.loads(p.read_text(encoding="utf-8")) for p in sorted(FIXTURES.glob("*.request.json"))]
    return sorted(reqs, key=lambda r: (r["method"] != "initialize", r["method"] == "shutdown"))


def started(spawn, env=None):
    sc = spawn(fake=True, env=env)
    resp = sc.request("initialize", INIT_PARAMS)
    assert "result" in resp, resp
    return sc, resp["result"]


def error_code(resp: dict) -> int:
    assert "error" in resp, f"expected an error response, got {resp}"
    assert_valid(MESSAGE, resp, "error response")
    return resp["error"]["code"]


def test_every_fixture_request_gets_a_schema_valid_response(spawn):
    sc = spawn()
    requests = fixture_requests()
    assert {r["method"] for r in requests} >= {"initialize", "ocr.recognize", "embed.images", "embed.text",
                                               "classify.zeroShot", "color.dominant", "pii.detect", "shutdown"}
    for req in requests:
        sc.send(req)
        resp = sc.response(req["id"])
        assert "result" in resp, f"{req['method']}: {resp}"
        assert_valid(MESSAGE, resp, f"{req['method']} response")
        assert_valid(validator(def_name(req["method"], "Result")), resp["result"], f"{req['method']} result")
        example = json.loads((FIXTURES / f"{req['method']}.response.json").read_text(encoding="utf-8"))["result"]
        assert set(resp["result"]) == set(example), req["method"]
        if "items" in req["params"]:
            got = [r["imageId"] for r in resp["result"]["results"]]
            assert got == [i["imageId"] for i in req["params"]["items"]], "every item answered, in request order"
    assert sc.wait_exit() == 0, "shutdown exits 0"
    assert sc.notifications, "batch methods send progress notifications"
    for note in sc.notifications:
        assert_valid(MESSAGE, note, "notification")
        assert note["method"] == "progress"


def test_bad_items_are_reported_in_band(spawn):
    sc, _ = started(spawn)
    items = [{"imageId": 1, "path": "C:\\s\\ok.png"}, {"imageId": 2, "path": "C:\\s\\broken.png"},
             {"imageId": 3, "path": "C:\\s\\scan.tiff"}]
    for method in ("ocr.recognize", "embed.images", "color.dominant"):
        resp = sc.request(method, {"items": items})
        codes = [(r["error"] or {}).get("code") for r in resp["result"]["results"]]
        assert codes == [None, "unreadable", "unsupported"], method


def test_unknown_method_is_32601(spawn):
    sc, _ = started(spawn)
    assert error_code(sc.request("ocr.translate", {"items": []})) == -32601


@pytest.mark.parametrize("method,params", [
    ("ocr.recognize", {}),
    ("ocr.recognize", {"items": [ITEM] * 17}),
    ("embed.images", {"items": [ITEM] * 33}),
    ("embed.text", {"texts": "cat"}),
    ("classify.zeroShot", {"items": [ITEM]}),
    ("initialize", dict(INIT_PARAMS, protocolVersion=2)),
    ("initialize", dict(INIT_PARAMS, device="tpu")),
    ("pii.detect", {"imageId": 1, "path": "a.png", "lines": [{"text": "x", "box": [1, 2, 3]}]}),
    ("color.dominant", [ITEM]),
    ("shutdown", {"now": True}),
])
def test_bad_params_are_32602(spawn, method, params):
    sc = spawn()
    assert error_code(sc.request(method, params)) == -32602


def test_malformed_lines_get_32700_and_32600(spawn):
    sc = spawn()
    sc.send_raw(b"{not json\n")
    assert error_code(sc.response(None)) == -32700
    sc.send({"jsonrpc": "1.0", "id": 5, "method": "shutdown", "params": {}})
    assert error_code(sc.response(5)) == -32600
    sc.send({"jsonrpc": "2.0", "id": -1, "method": "shutdown", "params": {}})
    assert error_code(sc.response(None)) == -32600
    assert sc.proc.poll() is None, "bad input never kills the sidecar"


def test_methods_before_initialize_are_32001(spawn):
    sc = spawn()
    assert error_code(sc.request("ocr.recognize", {"items": [ITEM]})) == -32001


def test_engine_that_fails_to_load_disables_its_capabilities(spawn):
    sc, init = started(spawn, env={config.ENV_FAKE_DISABLE: "embed"})
    assert init["capabilities"] == ["ocr", "color", "pii"]
    assert error_code(sc.request("embed.text", {"texts": ["cat"]})) == -32003
    assert error_code(sc.request("classify.zeroShot", {"items": [ITEM], "labels": ["a"]})) == -32003
    assert "result" in sc.request("ocr.recognize", {"items": [ITEM]})


def test_cuda_request_without_cuda_is_32002_and_unknown_model_32602(spawn):
    sc = spawn()
    assert error_code(sc.request("initialize", dict(INIT_PARAMS, device="cuda"))) == -32002
    assert error_code(sc.request("initialize", dict(INIT_PARAMS, embedModel="no-such-model"))) == -32602


def test_cancel_answers_32004_mid_batch(spawn):
    sc, _ = started(spawn, env={config.ENV_FAKE_DELAY_MS: "250"})
    rid = sc.next_id()
    began = time.monotonic()
    sc.send({"jsonrpc": "2.0", "id": rid, "method": "ocr.recognize", "params": {"items": [ITEM] * 16}})
    sc.notification(lambda n: n["method"] == "progress" and n["params"]["requestId"] == rid)
    sc.send({"jsonrpc": "2.0", "method": "$/cancel", "params": {"id": rid}})
    assert error_code(sc.response(rid)) == -32004
    assert time.monotonic() - began < 16 * 0.25, "cancel stopped the batch before its end"


def test_pipelined_requests_are_answered_by_id_in_any_order(spawn):
    sc, _ = started(spawn, env={config.ENV_FAKE_DELAY_MS: "200"})
    slow, fast = sc.next_id(), sc.next_id()
    sc.send({"jsonrpc": "2.0", "id": slow, "method": "ocr.recognize", "params": {"items": [ITEM] * 5}})
    sc.send({"jsonrpc": "2.0", "id": fast, "method": "color.dominant", "params": {"items": [ITEM]}})
    first = sc.response(fast)
    assert "result" in first and not sc.answered(slow), "the short request overtook the long one"
    assert "result" in sc.response(slow)


def test_stdout_carries_only_protocol_lines(spawn):
    sc, _ = started(spawn)
    sc.request("ocr.recognize", {"items": [ITEM]})
    sc.request("pii.detect", {"imageId": 1, "path": "C:\\shots\\a.png", "lines": None})
    sc.request("shutdown", {})
    assert sc.wait_exit() == 0
    assert len(sc.raw_lines) >= 5, "initialize, progress, ocr, pii, shutdown"
    for raw in sc.raw_lines:
        assert raw.endswith(b"\n") and not raw.startswith(b"\xef\xbb\xbf"), raw
        assert_valid(MESSAGE, json.loads(raw), "stdout line")


def test_library_noise_goes_to_stderr(spawn):
    sc, _ = started(spawn)
    sc.request("ocr.recognize", {"items": [ITEM]})
    err = sc.stderr()
    assert "[fake ocr] chatter on sys.stdout" in err and "[fake ocr] chatter on fd 1" in err, err[-2000:]
    assert all(b"chatter" not in raw for raw in sc.raw_lines)


def test_pii_detect_with_null_lines_runs_ocr(spawn):
    sc, _ = started(spawn)
    resp = sc.request("pii.detect", {"imageId": 7, "path": "C:\\shots\\a.png", "lines": None})
    assert [f["kind"] for f in resp["result"]["findings"]] == ["email"]


def run_cli(*args: str, env: dict | None = None) -> subprocess.CompletedProcess:
    full = dict(os.environ, PYTHONUTF8="1", SS_SIDECAR_FAKE_ENGINES="1", **(env or {}))
    return subprocess.run([sys.executable, "-m", "ss_sidecar", *args], cwd=SIDECAR_DIR, env=full,
                          capture_output=True, timeout=60)


def test_selftest_prints_one_json_line_and_exit_code():
    ok = run_cli("--selftest")
    lines = ok.stdout.decode().splitlines()
    assert ok.returncode == 0 and len(lines) == 1, (ok.stdout, ok.stderr[-2000:])
    report = json.loads(lines[0])
    assert {"capabilities", "device", "ok"} <= set(report) and report["ok"] is True
    bad = run_cli("--selftest", env={config.ENV_FAKE_DISABLE: "ocr"})
    assert bad.returncode == 1 and json.loads(bad.stdout)["ok"] is False


def test_sidecar_version_matches_pyproject():
    project = tomllib.loads((SIDECAR_DIR / "pyproject.toml").read_text(encoding="utf-8"))["project"]
    assert config.SIDECAR_VERSION == project["version"]


@pytest.mark.parametrize("text,kinds", [
    ("mail me at someone@example.com", ["email"]),
    ("Gọi 0912 345 678 hoặc +84 912 345 678", ["phone", "phone"]),
    ("hotline (028) 3822 1234", ["phone"]),
    ("see https://example.com/a?b=1.", ["url"]),
    ("card 4111 1111 1111 1111 exp 12/29", ["card"]),
    ("card 4111 1111 1111 1112", []),
    ("ngày 01.02.2026 10:30, v1.2.3, 2026-11-24", []),
])
def test_pii_kinds(text, kinds):
    assert [k for k, *_ in pii.find_spans(text)] == kinds


def test_pii_box_is_the_line_box_narrowed_to_the_match():
    (finding,) = pii.detect([{"text": "mail me at someone@example.com", "box": [10.0, 10.0, 300.0, 30.0]}])
    assert finding == {"kind": "email", "text": "someone@example.com", "box": [116.3, 10.0, 300.0, 30.0]}
    assert pii.find_spans("https://www.example.com")[0][3] == "https://www.example.com"
