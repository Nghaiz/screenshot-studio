"""Stand-in for `ss_sidecar` in the ss-core supervisor tests (phase 3.2). Stdlib only (CI runs it with the
runner's own Python), speaks contract-sidecar-jsonrpc.md, and builds every answer from the contract fixtures in
`contracts/fixtures/sidecar/` (their shapes, error objects and template values).

Behaviour is read once per process start from `<FAKE_SIDECAR_DIR>/mode`: one word, then optional `key=value`:
  ok                answer everything; `delay=<ms>` per request, `chatty=1` floods stderr + progress first
  crash-on-start    exit 3 before reading anything
  crash-after-init  answer `initialize`, then exit 4
  hang              answer `initialize`; every other request waits for `$/cancel`, then answers -32004
  hang-deaf         like `hang`, but `$/cancel` is ignored (the request never answers)
  garbage           answer `initialize`, then write a non-JSON line on stdout
  init-error        answer `initialize` with the fixture error, code -32002
Every event is appended to `<FAKE_SIDECAR_DIR>/log.jsonl` as one JSON object: start, request, cancel,
inflight (the number of requests being handled when one arrives), shutdown, eof.

Deterministic content: image vector = unit(1 at `imageId % 8`, -0.1 elsewhere); text vector of `near:<k>` or of a
text containing `code<k>` = image vector of k, else a hash of the text. OCR = the fixture lines + `code<imageId>`;
a file named `*receipt*` also gets `Hoá đơn tiền điện` and an e-mail line. Files named `*broken*` fail per item.
"""
from __future__ import annotations

import base64
import hashlib
import json
import os
import re
import struct
import sys
import threading
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
FIXTURES = HERE.parents[2] / "contracts" / "fixtures" / "sidecar"
CTL = Path(os.environ["FAKE_SIDECAR_DIR"])
DIM = 8
CHATTY_STDERR_LINES = 300
CHATTY_PROGRESS = 200

OUT = sys.stdout.buffer
OUT_LOCK = threading.Lock()
LOG_LOCK = threading.Lock()
INFLIGHT: dict[int, threading.Event] = {}
INFLIGHT_LOCK = threading.Lock()


def fixture(name: str) -> dict:
    return json.loads((FIXTURES / f"{name}.json").read_text(encoding="utf-8"))


def send(msg: dict) -> None:
    data = (json.dumps(msg, ensure_ascii=False, separators=(",", ":")) + "\n").encode("utf-8")
    with OUT_LOCK:
        OUT.write(data)
        OUT.flush()


def log(event: str, **kw) -> None:
    with LOG_LOCK, open(CTL / "log.jsonl", "a", encoding="utf-8") as f:
        f.write(json.dumps({"event": event, "pid": os.getpid(), **kw}) + "\n")


def read_mode() -> tuple[str, dict]:
    path = CTL / "mode"
    words = path.read_text(encoding="utf-8").split() if path.exists() else []
    words = words or ["ok"]
    return words[0], dict(w.split("=", 1) for w in words[1:] if "=" in w)


def unit(v: list[float]) -> list[float]:
    n = sum(x * x for x in v) ** 0.5
    return [x / n for x in v]


def b64(v: list[float]) -> str:
    return base64.b64encode(struct.pack(f"<{len(v)}f", *v)).decode("ascii")


def image_vector(image_id: int) -> list[float]:
    k = image_id % DIM
    return unit([1.0 if j == k else -0.1 for j in range(DIM)])


def text_vector(text: str) -> list[float]:
    m = re.fullmatch(r"near:(\d+)", text.strip()) or re.search(r"code(\d+)", text)
    if m:
        return image_vector(int(m.group(1)))
    h = hashlib.sha256(text.encode("utf-8")).digest()
    return unit([(b - 128) / 128 + 0.01 for b in h[:DIM]])


class Cancelled(Exception):
    pass


def item_error(code: str, message: str) -> dict:
    return {"code": code, "message": message}


def broken(item: dict) -> bool:
    return "broken" in Path(item["path"]).name.lower()


def ocr_lines(item: dict) -> list[dict]:
    template = fixture("ocr.recognize.response")["result"]["results"][0]["lines"]
    lines = [dict(line) for line in template]
    lines.append({"text": f"code{item['imageId']}", "box": [12.0, 72.0, 160.0, 96.0], "score": 0.95})
    if "receipt" in Path(item["path"]).name.lower():
        lines.append({"text": "Hoá đơn tiền điện", "box": [12.0, 104.0, 220.0, 128.0], "score": 0.9})
        lines.append({"text": "mail someone@example.com", "box": [10.0, 136.0, 300.0, 160.0], "score": 0.9})
    return lines


def per_item(rid: int, ev: threading.Event, items: list, ok, fail) -> list:
    results = []
    for done, item in enumerate(items, 1):
        if ev.is_set():
            raise Cancelled()
        results.append(fail(item) if broken(item) else ok(item))
        send({"jsonrpc": "2.0", "method": "progress", "params": {"requestId": rid, "done": done, "total": len(items)}})
    return results


def initialize(p: dict, rid: int, ev: threading.Event) -> dict:
    r = dict(fixture("initialize.response")["result"])
    r["embedDim"] = DIM
    r["embedModel"] = p["embedModel"]
    return r


def ocr_recognize(p: dict, rid: int, ev: threading.Event) -> dict:
    def ok(item):
        lines = ocr_lines(item)
        return {"imageId": item["imageId"], "text": "\n".join(l["text"] for l in lines), "lines": lines, "error": None}

    def fail(item):
        return {"imageId": item["imageId"], "text": "", "lines": [], "error": item_error("unreadable", "cannot decode image")}

    return {"results": per_item(rid, ev, p["items"], ok, fail)}


def embed_images(p: dict, rid: int, ev: threading.Event) -> dict:
    model = fixture("embed.images.response")["result"]["model"]

    def ok(item):
        return {"imageId": item["imageId"], "vector": b64(image_vector(item["imageId"])), "error": None}

    def fail(item):
        return {"imageId": item["imageId"], "vector": None, "error": item_error("unreadable", "cannot decode image")}

    return {"model": model, "dim": DIM, "results": per_item(rid, ev, p["items"], ok, fail)}


def embed_text(p: dict, rid: int, ev: threading.Event) -> dict:
    model = fixture("embed.text.response")["result"]["model"]
    return {"model": model, "dim": DIM, "vectors": [b64(text_vector(t)) for t in p["texts"]]}


def classify(p: dict, rid: int, ev: threading.Event) -> dict:
    labels = p["labels"]

    def ok(item):
        k = item["imageId"] % len(labels)
        scores = [0.02] * len(labels)
        scores[k], scores[(k + 1) % len(labels)] = 0.81, 0.12
        return {"imageId": item["imageId"], "scores": [{"label": l, "score": s} for l, s in zip(labels, scores)], "error": None}

    def fail(item):
        return {"imageId": item["imageId"], "scores": [], "error": item_error("unreadable", "cannot decode image")}

    return {"results": per_item(rid, ev, p["items"], ok, fail)}


def color(p: dict, rid: int, ev: threading.Event) -> dict:
    template = fixture("color.dominant.response")["result"]["results"][0]

    def ok(item):
        i = item["imageId"]
        rgb = f"#{(i * 53) % 256:02X}{(i * 97) % 256:02X}{(i * 31) % 256:02X}"
        return {"imageId": i, "rgb": rgb, "palette": [rgb, *template["palette"][1:]], "error": None}

    def fail(item):
        return {"imageId": item["imageId"], "rgb": None, "palette": [], "error": item_error("unsupported", "format not supported")}

    return {"results": per_item(rid, ev, p["items"], ok, fail)}


def pii_detect(p: dict, rid: int, ev: threading.Event) -> dict:
    if p["lines"] is None:
        return fixture("pii.detect.response")["result"]
    findings = []
    for line in p["lines"]:
        for m in re.finditer(r"[\w.+-]+@[\w-]+\.[\w.]+", line["text"]):
            findings.append({"kind": "email", "text": m.group(0), "box": line["box"]})
    return {"findings": findings}


METHODS = {
    "initialize": initialize,
    "ocr.recognize": ocr_recognize,
    "embed.images": embed_images,
    "embed.text": embed_text,
    "classify.zeroShot": classify,
    "color.dominant": color,
    "pii.detect": pii_detect,
}


def error(rid: int, code: int, message: str) -> None:
    template = fixture("error")
    err = dict(template["error"], code=code, message=message)
    send({"jsonrpc": "2.0", "id": rid, "error": err})


def chatter(rid: int) -> None:
    filler = "x" * 4000
    for n in range(CHATTY_STDERR_LINES):
        sys.stderr.write(f"fake chatter {rid} {n} {filler}\n")
    sys.stderr.flush()
    for n in range(CHATTY_PROGRESS):
        send({"jsonrpc": "2.0", "method": "progress", "params": {"requestId": rid, "done": n, "total": CHATTY_PROGRESS}})


def worker(msg: dict, mode: str, opts: dict) -> None:
    rid, method, params = msg["id"], msg["method"], msg.get("params", {})
    ev = threading.Event()
    with INFLIGHT_LOCK:
        INFLIGHT[rid] = ev
        log("inflight", n=len(INFLIGHT))
    try:
        if method == "initialize" and mode == "init-error":
            error(rid, -32002, "device unavailable")
            return
        if method != "initialize" and mode in ("hang", "hang-deaf"):
            ev.wait()
            if mode == "hang":
                error(rid, -32004, "cancelled")
            return
        if int(opts.get("delay", "0")):
            time.sleep(int(opts["delay"]) / 1000)
        if opts.get("chatty") == "1":
            chatter(rid)
        handler = METHODS.get(method)
        if handler is None:
            error(rid, -32601, f"unknown method {method!r}")
            return
        send({"jsonrpc": "2.0", "id": rid, "result": handler(params, rid, ev)})
        if method == "initialize" and mode == "garbage":
            with OUT_LOCK:
                OUT.write(b"this is not json\n")
                OUT.flush()
    except Cancelled:
        error(rid, -32004, "cancelled")
    except Exception as e:  # a bug in this fake must fail the test loudly, not hang it
        error(rid, -32603, f"fake sidecar bug: {e!r}")
    finally:
        with INFLIGHT_LOCK:
            INFLIGHT.pop(rid, None)


def main() -> None:
    mode, opts = read_mode()
    log("start", mode=mode)
    if mode == "crash-on-start":
        sys.stderr.write("fake sidecar: crash on start\n")
        sys.stderr.flush()
        os._exit(3)
    for raw in iter(sys.stdin.buffer.readline, b""):
        line = raw.strip()
        if not line:
            continue
        msg = json.loads(line.decode("utf-8"))
        method = msg.get("method")
        if "id" not in msg:
            target = msg.get("params", {}).get("id")
            log("cancel", id=target)
            if mode != "hang-deaf":
                with INFLIGHT_LOCK:
                    ev = INFLIGHT.get(target)
                if ev is not None:
                    ev.set()
            continue
        log("request", id=msg["id"], method=method, params=msg.get("params"))
        if method == "shutdown":
            send({"jsonrpc": "2.0", "id": msg["id"], "result": fixture("shutdown.response")["result"]})
            log("shutdown")
            os._exit(0)
        if method == "initialize" and mode == "crash-after-init":
            worker(msg, mode, opts)
            time.sleep(0.05)
            sys.stderr.write("fake sidecar: crash after initialize\n")
            sys.stderr.flush()
            os._exit(4)
        threading.Thread(target=worker, args=(msg, mode, opts), daemon=True).start()
    log("eof")
    os._exit(0)


if __name__ == "__main__":
    main()
