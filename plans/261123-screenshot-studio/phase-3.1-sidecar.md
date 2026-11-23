# Phase 3.1: Python AI sidecar (lane ai, human-gated downloads)

Lane: ai
Depends-on: 1.1
Gate: human (approve torch cu132 + onnxruntime + rapidocr install and ~1-2 GB model downloads to D:)
Steps: lane-decides
Effort: L
Owns: `sidecar/**`
Creates: `sidecar/tests/`, `sidecar/ss_sidecar/__main__.py`, `sidecar/ss_sidecar/server.py`, `sidecar/tests/test_contract.py`, `sidecar/tests/test_ocr.py`, `sidecar/tests/test_embed.py`, `sidecar/tests/conftest.py`
AX: AX-01 `python -m ss_sidecar --selftest` prints one JSON line `{capabilities, device, ok}` and exits 0/1; `--prefetch` downloads models and exits; protocol mode is the default
Accept: `node scripts/accept/lane-tests.mjs sidecar/tests/test_contract.py sidecar/tests/test_ocr.py sidecar/tests/test_embed.py`

## Goal

A standalone JSON-RPC 2.0 stdio server implementing `contract-sidecar-jsonrpc.md` exactly: OCR (vi + en), SigLIP i18n
image/text embeddings, zero-shot classification, dominant colour, PII detection. Independent of the Rust lanes: it
only needs the contract files from 1.1, so it runs beside phases 1.2-2.3.

## Steps (lane decides the order after step 1)

1. Spike FIRST (Forecast uncertainty 1): RapidOCR 3.x on 20 owner screenshots that are LOCAL (not cloud-only; check the
   attribute first, never open a cloud-only file) copied to `%TEMP%` (read-only copy, D10)
   with Vietnamese text; measure character accuracy against hand-checked text for 5 of them. < 85% -> switch the OCR
   engine to PaddleOCR (design fallback) and record it in Deferred decisions. Also time per image on CPU.
2. Env (human-approved): `UV_PROJECT_ENVIRONMENT=D:\IOT_Tools\data\ScreenshotStudio\sidecar-venv` then
   `uv sync --project sidecar`; `HF_HOME=D:\DevCache\huggingface` (set in phase 0); `python -m ss_sidecar --prefetch`.
   Probe CUDA: `--selftest` reports `device`; failure -> CPU (Deferred decision #19), not a blocker.
3. Server: line-delimited JSON reader on stdin, writer on stdout (flush per line), logging to stderr only; method
   registry as data; per-item errors in-band; `$/cancel`; `progress` notifications; `shutdown`.
4. Engines behind small interfaces (`OcrEngine`, `Embedder`, `ColorExtractor`, `PiiDetector`); `test_contract.py` uses
   fakes, `test_ocr.py` and `test_embed.py` use the REAL models and never skip (lane-tests.mjs fails on any skip, D11).
5. PII: regex for email, phone (VN + intl), URL, card (Luhn) over OCR lines; boxes from OCR line boxes.

## Tests

`test_contract.py`: every response produced for the `contracts/fixtures/sidecar/*.request.json` fixtures validates
against `contracts/sidecar-jsonrpc.schema.json`; unknown method -> -32601; bad params -> -32602; stdout carries no
non-JSON line (spawn `python -m ss_sidecar` as a subprocess and drive it over pipes). `test_ocr.py`: a generated
image with known English and Vietnamese text is recognised (substring match, diacritics compared after NFC).
`test_embed.py`: text "cat" is closer to a cat-like generated image than to a text screenshot; vectors L2-normalized,
dim == `embedDim`. `lane-tests.mjs` runs them with the sidecar venv's `python.exe -m pytest`.

## Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Vietnamese OCR accuracy | medium | medium | step-1 spike with a measured threshold; PaddleOCR fallback |
| CUDA wheel/driver mismatch | medium | low | CPU fallback, reported in `device` |
| torch install size on D: (~3-5 GB) | high | low | venv on D: (data dir), uv cache already on D: |
| stdout pollution by libraries (progress bars) | medium | high | redirect `sys.stdout` to stderr at import; protocol writer holds the real stdout fd; contract test asserts it |
