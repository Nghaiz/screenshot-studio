// Process helpers for phase Accept scripts: build, run, launch and stop the desktop app (phase 1.4).
//
// What is measured is the product, not a dev build (owner, 2026-11-23: "super light, super fast"):
//   1. `vite build` writes the production UI to dist/;
//   2. `cargo build --release -p screenshot-studio --features tauri/custom-protocol` embeds dist/ in the exe
//      (what `cargo tauri build` does, without the NSIS bundle), so the page loads from http://tauri.localhost
//      with no dev server; `ssctl` is built with the same release profile;
//   3. the exe starts with WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<port> (WebView2 reads
//      it when its browser process starts) and SS_DATA_DIR only when a data dir is given (fixture runs); without
//      it the app opens the real data dir, which Accept scripts only READ (README D10).
// Fallbacks the plan names: `cargo tauri dev`, or the debug exe against a served dist/; both measure an
// unoptimized build (the debug exe took 8.9 s to expose CDP), so they are not used for the timing checks.
import { spawn, spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

// Upper-case drive letter: vitest breaks under `d:\` (docs/lessons.md), and every runner shares ROOT.
export const ROOT = path
  .resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..", "..")
  .replace(/^[a-z]:/, (d) => d.toUpperCase());

export const APP_EXE_NAME = "screenshot-studio.exe";
export const APP_EXE = path.join(ROOT, "target", "release", APP_EXE_NAME);
export const SSCTL_EXE = path.join(ROOT, "target", "release", "ssctl.exe");
export const APP_ORIGIN = "http://tauri.localhost"; // embedded assets on Windows WebView2
export const CDP_PORT = 9222;

// Keep the page rendering at full rate when another window covers the app (Chromium otherwise throttles
// occluded or background renderers, which would make the fps probe measure the throttle, not the grid).
const WEBVIEW_FLAGS = [
  "--disable-features=CalculateNativeWinOcclusion",
  "--disable-background-timer-throttling",
  "--disable-renderer-backgrounding",
];

const VITE = path.join(ROOT, "node_modules", "vite", "bin", "vite.js");

/** Runs a command to completion (no shell). Returns { status, out, error, stdout }. */
export function run(cmd, args, opts = {}) {
  const r = spawnSync(cmd, args, { cwd: ROOT, encoding: "utf8", maxBuffer: 256 * 1024 * 1024, ...opts });
  return { status: r.status, error: r.error, out: `${r.stdout ?? ""}${r.stderr ?? ""}`, stdout: r.stdout ?? "" };
}

/** Env for a child: `overrides` set, keys with value `undefined` removed (e.g. SS_DATA_DIR for real-data runs). */
export function childEnv(overrides = {}) {
  const env = { ...process.env };
  for (const [k, v] of Object.entries(overrides)) {
    if (v === undefined) delete env[k];
    else env[k] = v;
  }
  return env;
}

/** PIDs of running processes with this image name (tasklist, CSV, no header). */
export function runningPids(imageName) {
  const r = run("tasklist", ["/FI", `IMAGENAME eq ${imageName}`, "/FO", "CSV", "/NH"]);
  if (r.error || r.status !== 0) throw new Error(`tasklist failed: ${r.error?.message ?? r.out}`);
  return r.stdout
    .split(/\r?\n/)
    .map((l) => l.match(/^"([^"]+)","(\d+)"/))
    .filter((m) => m && m[1].toLowerCase() === imageName.toLowerCase())
    .map((m) => Number(m[2]));
}

/**
 * Production UI bundle into dist/, then the release `ssctl` and the release app with dist/ embedded.
 * Two cargo calls: `--features tauri/custom-protocol` names a dependency of the app crate only.
 * Cargo runs without the sccache wrapper (`RUSTC_WRAPPER=""` overrides `build.rustc-wrapper`): the app crate's
 * rustc command line is ~41,000 chars (one `-L dependency=` per crate), above the Windows 32,767 limit; cargo
 * moves a long command line into an @argfile, sccache 0.19 does not (os error 206). Fingerprints do not include
 * the wrapper, so this rebuilds nothing extra. Returns the first failing step's result, else the last one.
 */
export function buildRelease() {
  const noWrapper = { env: childEnv({ RUSTC_WRAPPER: "" }) };
  const steps = [
    ["vite build", process.execPath, [VITE, "build", "--logLevel", "warn"], {}],
    ["cargo ssctl", "cargo", ["build", "--release", "-p", "ss-core", "--bin", "ssctl"], noWrapper],
    ["cargo app", "cargo", ["build", "--release", "-p", "screenshot-studio", "--features", "tauri/custom-protocol"], noWrapper],
  ];
  let last;
  for (const [name, cmd, args, opts] of steps) {
    last = { name, ...run(cmd, args, opts) };
    if (last.error || last.status !== 0) return last;
  }
  return last;
}

/** Runs ssctl with `--json`; parses its one stdout JSON document. `dataDir` undefined = real data dir. */
export function ssctl(args, { dataDir } = {}) {
  const flags = dataDir ? ["--data-dir", dataDir] : [];
  const r = run(SSCTL_EXE, [...flags, "--json", ...args], { env: childEnv({ SS_DATA_DIR: undefined }) });
  if (r.error) throw new Error(`ssctl ${args[0]}: ${r.error.message}`);
  let json = null;
  try {
    json = JSON.parse(r.stdout.trim().split(/\r?\n/).pop() ?? "null");
  } catch {
    // keep null: the caller reports status + output
  }
  return { status: r.status, json, out: r.out };
}

const sleep = (ms) => new Promise((res) => setTimeout(res, ms));

async function waitHttp(url, timeoutMs, isDead) {
  const end = Date.now() + timeoutMs;
  let last = "";
  while (Date.now() < end) {
    const dead = isDead();
    if (dead) throw new Error(dead);
    try {
      const res = await fetch(url);
      if (res.ok) return res;
      last = `HTTP ${res.status}`;
    } catch (e) {
      last = e.cause?.code ?? e.message;
    }
    await sleep(50);
  }
  throw new Error(`timeout after ${timeoutMs} ms waiting for ${url} (${last})`);
}

export function killTree(pid) {
  if (pid) run("taskkill", ["/PID", String(pid), "/T", "/F"]);
}

/**
 * Starts the app with CDP enabled: the release exe, or `exe` (e.g. the installed copy, phase 2.3; cwd = its dir).
 * `startedAt` is the epoch ms just before spawn (cold-start origin). Resolves once the CDP endpoint answers;
 * `stop()` kills the app's process tree (WebView2 children included).
 */
export async function startApp({ dataDir, cdpPort = CDP_PORT, timeoutMs = 60_000, exe = APP_EXE } = {}) {
  if (!existsSync(exe)) throw new Error(`${exe} missing: build it first`);
  const env = childEnv({
    SS_DATA_DIR: dataDir,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: [`--remote-debugging-port=${cdpPort}`, ...WEBVIEW_FLAGS].join(" "),
  });
  const startedAt = Date.now();
  const proc = spawn(exe, [], { cwd: exe === APP_EXE ? ROOT : path.dirname(exe), env, stdio: ["ignore", "pipe", "pipe"] });
  let log = "";
  proc.stdout.on("data", (d) => (log += d));
  proc.stderr.on("data", (d) => (log += d));
  const stop = () => killTree(proc.pid);
  const dead = () => (proc.exitCode !== null ? `app exited ${proc.exitCode}: ${log.trim().slice(-2000)}` : null);
  try {
    await waitHttp(`http://127.0.0.1:${cdpPort}/json/version`, timeoutMs, dead);
  } catch (e) {
    stop();
    throw e;
  }
  return { pid: proc.pid, startedAt, cdpUrl: `http://127.0.0.1:${cdpPort}`, stop, log: () => log };
}

/**
 * Memory of a process and all its descendants (the app + its WebView2 browser, GPU and renderer processes):
 * { processes, workingSetMB, privateMB, names } from Win32_Process (WorkingSetSize, PrivatePageCount, in bytes).
 * A child must have started after its parent: Windows reuses PIDs and never updates ParentProcessId, so an older
 * process whose dead parent's PID now belongs to one of ours is not ours (phase 2.3 counted a foreign 24 MB
 * `NhNotifSys.exe` under the WebView2 crashpad handler that way).
 */
export function treeMemory(rootPid) {
  const ps =
    "Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,Name,WorkingSetSize,PrivatePageCount," +
    "@{n='Created';e={ if ($_.CreationDate) { ([DateTimeOffset]$_.CreationDate).ToUnixTimeMilliseconds() } else { 0 } }} | ConvertTo-Json -Compress";
  const r = run("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", ps]);
  if (r.error || r.status !== 0) throw new Error(`Win32_Process query failed: ${r.error?.message ?? r.out.slice(-300)}`);
  const rows = JSON.parse(r.stdout);
  const kids = new Map();
  for (const p of rows) kids.set(p.ParentProcessId, [...(kids.get(p.ParentProcessId) ?? []), p]);
  const tree = [];
  const stack = rows.filter((p) => p.ProcessId === rootPid);
  while (stack.length) {
    const p = stack.pop();
    tree.push(p);
    stack.push(...(kids.get(p.ProcessId) ?? []).filter((c) => c.ProcessId !== p.ProcessId && Number(c.Created) >= Number(p.Created)));
  }
  const mb = (k) => Math.round(tree.reduce((a, p) => a + Number(p[k] ?? 0), 0) / 1048576);
  const names = [...new Set(tree.map((p) => p.Name))].sort().join(",");
  return { processes: tree.length, workingSetMB: mb("WorkingSetSize"), privateMB: mb("PrivatePageCount"), names };
}
