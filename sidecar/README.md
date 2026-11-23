# ss-sidecar

Python AI sidecar of Screenshot Studio: OCR (vi + en), SigLIP i18n embeddings, zero-shot labels, dominant colour and
PII findings, served as JSON-RPC 2.0 over stdio. The protocol is `plans/261123-screenshot-studio/contract-sidecar-jsonrpc.md`;
params are validated at run time against `contracts/sidecar-jsonrpc.schema.json`.

## Setup (once)

```powershell
$env:UV_PROJECT_ENVIRONMENT = "D:\IOT_Tools\data\ScreenshotStudio\sidecar-venv"
uv sync --project sidecar
cd sidecar; & "$env:UV_PROJECT_ENVIRONMENT\Scripts\python.exe" -m ss_sidecar --prefetch   # weights -> HF_HOME
```

## Run

| Command (cwd `sidecar/`) | Output |
|---|---|
| `python -m ss_sidecar` | protocol mode (what Rust spawns): requests on stdin, responses + `progress` on stdout |
| `python -m ss_sidecar --selftest` | one JSON line `{capabilities, device, ok, errors}`; exit 0 when every engine loaded and passed a smoke check |
| `python -m ss_sidecar --prefetch` | one JSON line `{ok, models, errors}`; the only mode that downloads (protocol mode runs `HF_HUB_OFFLINE=1`) |

Options: `--device auto|cuda|cpu`, `--embed-model NAME`. stdout carries only this program's JSON lines; every log line
and every library print goes to stderr.

| Env var | Effect |
|---|---|
| `HF_HOME` | model cache (`D:\DevCache\huggingface`) |
| `SS_CONTRACT_SCHEMA` | schema path when the repo layout is not present (bundled app) |
| `SS_SIDECAR_FAKE_ENGINES=1` | deterministic stdlib-only fakes (contract test, CI) |
| `SS_SIDECAR_FAKE_DELAY_MS`, `SS_SIDECAR_FAKE_DISABLE=ocr,embed,color` | fake-only: per-item delay, engines that fail to load |

## Layout

`server.py` (reader, 2 workers, locked writer, `$/cancel`), `methods.py` (handlers + registry), `schema.py`,
`pii.py` (regex + Luhn), `engines/` (`base.py` interfaces; `ocr_rapid.py`, `siglip.py`, `color.py` real; `fakes.py`).

## Tests

`node scripts/accept/lane-tests.mjs sidecar/tests/test_contract.py sidecar/tests/test_ocr.py sidecar/tests/test_embed.py`
from the repo root. `test_contract.py` uses fakes and runs in CI (stdlib + jsonschema + pytest); `test_ocr.py` and
`test_embed.py` drive the real models and run locally only.
