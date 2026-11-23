"""Screenshot Studio AI sidecar: JSON-RPC 2.0 over stdio (plans/.../contract-sidecar-jsonrpc.md).

This package must stay importable with the standard library + jsonschema only: heavy engines (torch, open_clip,
rapidocr, Pillow) are imported lazily inside `ss_sidecar.engines` when `initialize` loads them.
"""
