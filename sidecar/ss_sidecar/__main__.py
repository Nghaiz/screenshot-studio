"""`python -m ss_sidecar [--selftest | --prefetch] [--device auto|cuda|cpu] [--embed-model NAME]`.

Default: JSON-RPC protocol mode on stdin/stdout (contract-sidecar-jsonrpc.md). In every mode the real stdout is
claimed FIRST for this program's own output, and fd 1 + sys.stdout are pointed at stderr, so no library (progress
bars, prints, native logging) can ever write a non-protocol line to the pipe Rust reads.
"""
from __future__ import annotations

import argparse
import json
import os
import sys

from . import config


def parse_args(argv: list[str] | None) -> argparse.Namespace:
    p = argparse.ArgumentParser(prog="python -m ss_sidecar", description="Screenshot Studio AI sidecar")
    mode = p.add_mutually_exclusive_group()
    mode.add_argument("--selftest", action="store_true", help="load engines, smoke-test them, print one JSON line")
    mode.add_argument("--prefetch", action="store_true", help="download/load every model, print one JSON line")
    p.add_argument("--device", choices=("auto", "cuda", "cpu"), default="auto")
    p.add_argument("--embed-model", default=config.DEFAULT_EMBED_MODEL)
    return p.parse_args(argv)


def claim_stdio():
    """Take private binary streams on the original stdin/stdout; give fd 0 NUL and fd 1 stderr for everyone else.

    stdout: no library can write a non-protocol line. stdin: on Windows a thread blocked in a synchronous ReadFile
    on the stdin pipe stalls any other call on that pipe (a library probing stdin while importing hung `initialize`
    until the next request line arrived, phase 3.1), so libraries must never see the protocol pipe at all.
    """
    sys.stdout.flush()
    protocol_out = os.fdopen(os.dup(1), "wb", buffering=0)
    protocol_in = os.fdopen(os.dup(0), "rb")
    null = os.open(os.devnull, os.O_RDONLY)
    os.dup2(null, 0)
    os.close(null)
    sys.stdin = open(os.devnull, encoding="utf-8")  # noqa: SIM115 - lives for the whole process
    os.dup2(2, 1)  # the guard: native code, child processes and sys.stdout itself now write to stderr
    sys.stdout = sys.stderr  # prints share stderr's buffer, so they interleave in order with the log lines
    return protocol_in, protocol_out


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)  # before claiming stdout, so --help still prints to stdout
    inp, out = claim_stdio()
    import logging

    logging.basicConfig(stream=sys.stderr, level=logging.INFO,
                        format="%(asctime)s %(levelname)s %(name)s: %(message)s")
    for noisy in ("httpx", "httpx2", "urllib3"):
        logging.getLogger(noisy).setLevel(logging.WARNING)
    os.environ.setdefault("TOKENIZERS_PARALLELISM", "false")
    if not args.prefetch:
        # Only --prefetch downloads; the app's sidecar loads from the HF_HOME cache and never touches the network.
        os.environ.setdefault("HF_HUB_OFFLINE", "1")
        os.environ.setdefault("HF_HUB_DISABLE_PROGRESS_BARS", "1")

    if args.selftest or args.prefetch:
        from . import tools

        report = tools.selftest(args.device, args.embed_model) if args.selftest else tools.prefetch(args.embed_model)
        out.write((json.dumps(report, ensure_ascii=False, separators=(",", ":")) + "\n").encode())
        return 0 if report["ok"] else 1

    from .server import Server

    return Server(out, inp).serve()


if __name__ == "__main__":
    code = main()
    sys.stderr.flush()
    os._exit(code)  # do not wait for worker threads stuck in a model call; protocol output is unbuffered
