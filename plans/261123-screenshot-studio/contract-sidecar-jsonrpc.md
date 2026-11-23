# Contract: Rust <-> Python sidecar (JSON-RPC 2.0 over stdio)

Owner: plan author. Version 1. Transcribed by phase 1.1 into `contracts/sidecar-jsonrpc.schema.json`
(JSON Schema 2020-12, one `$defs` entry per method's params and result) and `contracts/fixtures/sidecar/*.json`
(one request + one response example per method, plus one error example). Rust serde types live in
`crates/ss-core/src/contract/sidecar.rs`.

## Transport

- Process: `<venv>\Scripts\python.exe -m ss_sidecar` (D6), cwd = sidecar source dir, env adds `HF_HOME`,
  `PYTHONUTF8=1`, `PYTHONUNBUFFERED=1`. Venv default `D:\IOT_Tools\data\ScreenshotStudio\sidecar-venv`
  (setting `sidecarVenv` overrides).
- Framing: newline-delimited JSON. One JSON object per line on stdin (requests) and stdout (responses and
  notifications), UTF-8, no embedded newlines, no BOM. stdout carries ONLY protocol lines; all logging goes to
  stderr (Rust forwards stderr lines to the app log at debug level).
- Rust is the only client. Requests are sequential per method class but may be pipelined: Rust keeps at most
  2 outstanding requests; the sidecar answers in any order, matched by `id` (integer, increasing).
- Paths: absolute Windows paths as JSON strings. Images are read by the sidecar from disk; no image bytes on the pipe.
- Vectors: base64 of little-endian float32, L2-normalized.

## Lifecycle

1. Rust spawns, sends `initialize` within 1 s; sidecar must answer within `SIDECAR_INIT_TIMEOUT_S = 120`
   (model load). State `starting` until then, `ready` after.
2. Absent venv/python -> state `absent` (no spawn attempted). Spawn/initialize failure -> `error` with `detail`.
   Crash -> restart with backoff 1 s, 5 s, 30 s; after 3 crashes in 10 min -> `error` until `restart_sidecar`.
3. Per-request timeout `SIDECAR_REQUEST_TIMEOUT_S = 300` -> Rust sends `$/cancel`, then kills and restarts if no
   answer within 10 s.
4. App exit: Rust sends `shutdown`, waits 3 s, then kills the process.
5. Constants live in `crates/ss-core/src/sidecar/config.rs`; never inline literals elsewhere.

## Methods

| Method | params | result |
|---|---|---|
| `initialize` | `{ protocolVersion: 1, device: "auto"|"cuda"|"cpu", languages: ["vi","en"], embedModel: string, ocrEngine: "rapidocr" }` | `{ protocolVersion: 1, sidecarVersion: string, capabilities: ("ocr"|"embed"|"classify"|"color"|"pii")[], device: "cuda"|"cpu", embedModel: string, embedDim: number }` |
| `ocr.recognize` | `{ items: { imageId: number, path: string }[] }` (max 16) | `{ results: { imageId, text: string, lines: { text, box: [x1,y1,x2,y2], score }[], error: RpcItemError|null }[] }` |
| `embed.images` | `{ items: { imageId, path }[] }` (max 32) | `{ model: string, dim: number, results: { imageId, vector: string|null, error: RpcItemError|null }[] }` |
| `embed.text` | `{ texts: string[] }` (max 16) | `{ model: string, dim: number, vectors: string[] }` |
| `classify.zeroShot` | `{ items: { imageId, path }[], labels: string[] }` | `{ results: { imageId, scores: { label, score }[], error: RpcItemError|null }[] }` |
| `color.dominant` | `{ items: { imageId, path }[] }` | `{ results: { imageId, rgb: "#RRGGBB"|null, palette: string[], error: RpcItemError|null }[] }` |
| `pii.detect` | `{ imageId, path, lines: { text, box }[] | null }` (null = sidecar runs OCR itself) | `{ findings: { kind: "email"|"phone"|"url"|"card", text, box: [x1,y1,x2,y2] }[] }` |
| `shutdown` | `{}` | `{}` then the process exits 0 |

Notification (sidecar -> Rust, no `id`): `progress` `{ requestId: number, done: number, total: number }`.
Notification (Rust -> sidecar, no `id`): `$/cancel` `{ id: number }`; the sidecar answers the cancelled request
with error `-32004`.

`RpcItemError` = `{ code: "unreadable"|"unsupported"|"timeout"|"internal", message: string }` — a per-item
failure inside a successful batch; the batch still returns every item.

## Errors (whole-request)

Standard JSON-RPC 2.0 error object `{ code, message, data: { detail: string|null } }`:
`-32700` parse error, `-32600` invalid request, `-32601` method not found, `-32602` invalid params,
`-32001` model not loaded, `-32002` device unavailable, `-32003` capability disabled, `-32004` cancelled,
`-32000` internal.

## Degradation (the app must work without the sidecar)

- `SidecarStatus.capabilities` gates every AI UI control: absent capability -> control disabled with a visible
  notice ("AI features need the Python sidecar", link to Settings > AI showing the setup command).
- Filename/date/size/tag/colour-less search, duplicates (Rust), sessions, blur/tiny, all file ops and the editor
  never call the sidecar.
- `query_images` with `textMode: "semantic"` while the state is not `ready` -> error `sidecarUnavailable`.
- `textMode: "any"` uses the semantic arm only when the state is `ready` AND `embed` is a capability, and bounds the
  `embed.text` call by `SEMANTIC_QUERY_TIMEOUT_MS = 1500`; on any failure or timeout it returns the FTS results alone
  (the UI shows the notice once per session). A `starting` or hung sidecar never delays ordinary search.
