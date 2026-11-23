#!/usr/bin/env node
// Phase 2.3 Accept (P2: the installed app, AX-01):  node scripts/accept/p2.mjs
//
// Prints one stable line per check on stdout:  PASS|FAIL <check> <value>   (INFO lines are measured-only, never
// counted). Exit 0 only when every check passed, 1 otherwise. No prompts. Starts and stops the INSTALLED app itself.
// Prerequisite (human-approved, plan phase-2.3 steps 3-4; re-run both after every rebuild):
//   cargo tauri build      (RUSTC_WRAPPER="" and CARGO_BUILD_JOBS=4 on this machine, docs/lessons.md)
//   "target\release\bundle\nsis\Screenshot Studio_<version>_x64-setup.exe" /S /D=D:\IOT_Tools\apps\ScreenshotStudio
//
// Checks, in order (plan phase-2.3 step 5):
//   app-not-running  no screenshot-studio.exe is running (single-instance would hijack the run, README D10)
//   cargo-tests      `cargo test --workspace --locked` (child process): exit 0, every `test result` ok, passed > 0,
//                    failed = ignored = 0
//   ui-tests         `vitest run` (child process, json reporter, upper-case drive cwd): every file passed, tests > 0,
//                    none failed, skipped or todo
//   installed-exe    <install dir>\screenshot-studio.exe exists and is target\release's build (byte-identical apart from
//                    the bundler's NSIS stamp), so a rebuild without a re-install fails here, not later
//   start-menu       a `Screenshot Studio.lnk` under %APPDATA%\Microsoft\Windows\Start Menu\Programs (recursive) whose
//                    TargetPath (WScript.Shell in a child PowerShell) is the installed exe. The stated PROXY for "found
//                    in Windows Search" (Search lists Start Menu Programs shortcuts); the real search is checked by hand
//   no-desktop-lnk   no `Screenshot Studio.lnk` on the Desktop: it is redirected into OneDrive on this machine and nothing
//                    may write there (installer hook src-tauri/windows/installer-hooks.nsh)
//   window           the installed exe shows a window titled `Screenshot Studio` within 5,000 ms of spawn
//   single-instance  a second launch exits by itself and leaves exactly one screenshot-studio.exe (the first one)
//   close            WM_CLOSE on the window (CloseMainWindow) ends the app and its WebView2 processes within 10 s
//   memory           app + WebView2 private bytes <= 500 MB after scrolling the whole real library (Deferred decision
//                    #31): a 10 s top-to-bottom scroll (lib/cdp.mjs scrollFrames, the method 1.4 measured 1,208 MB
//                    with), then IDLE_MS without input; the PEAK of the samples from the end of the scroll to the end
//                    of the idle is gated. FAIL also when the scroll did not reach the bottom.
//                    INFO memory-visit-all (measured, not gated, orchestrator decision in Deferred decision #31): a
//                    page-by-page pass that waits on each screen until its thumbnails are loaded, so every loadable
//                    thumbnail is decoded once; its peak and its value after IDLE_MS
//   webview-data     the WebView2 browser runs with --user-data-dir under <data dir>\webview, that folder holds the
//                    EBWebView profile, and %LOCALAPPDATA%\dev.nghaiz.screenshotstudio holds no EBWebView
//
// The real data dir is only READ (README D10): the app indexes nothing unless a user asks it to; scrolling serves
// thumbnails that already exist.
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import process from "node:process";
import { GRID, connectApp, percentile, scrollFrames, waitForGrid } from "./lib/cdp.mjs";
import { APP_EXE, APP_EXE_NAME, ROOT, childEnv, killTree, run, runningPids, startApp, treeMemory } from "./lib/run.mjs";

const INSTALL_DIR = "D:\\IOT_Tools\\apps\\ScreenshotStudio"; // design-intent.md § Names and paths
const INSTALLED_EXE = path.join(INSTALL_DIR, APP_EXE_NAME);
const DATA_DIR = "D:\\IOT_Tools\\data\\ScreenshotStudio"; // ss-core DEFAULT_DATA_DIR (the app runs without SS_DATA_DIR)
const WEBVIEW_DIR = path.join(DATA_DIR, "webview"); // src-tauri/src/window.rs WEBVIEW_DIR
const IDENTIFIER = "dev.nghaiz.screenshotstudio"; // tauri.conf.json identifier
const TITLE = "Screenshot Studio";
const LNK_NAME = `${TITLE}.lnk`;
const WINDOW_MAX_MS = 5000;
const SECOND_EXIT_MS = 10_000;
const CLOSE_MAX_MS = 10_000;
const MEMORY_MAX_MB = 500; // Deferred decision #31
const SCROLL_MS = 10_000;
const PAGE_SETTLE_MS = 200; // > src/lib/defaults.ts THUMB_SETTLE_MS (150): a cell gets its <img> only after that
const PAGE_MAX_WAIT_MS = 2000;
const MAX_UNLOADED_SHARE = 0.01;
const IDLE_MS = 10_000; // no input after a scroll: the last screen's thumbnails load, Chromium's idle purge runs
const SAMPLE_GAP_MS = 500; // between memory samples (each Win32_Process query itself takes ~1 s)

const results = [];
function report(ok, check, value) {
  results.push(ok);
  console.log(`${ok ? "PASS" : "FAIL"} ${check} ${value}`);
}
const tail = (s, n = 30) => s.trim().split(/\r?\n/).slice(-n).join("\n");
const sleep = (ms) => new Promise((res) => setTimeout(res, ms));

/** Runs a PowerShell script in a child process (inputs through env vars, never spliced into the script). */
function powershell(script, env = {}) {
  const r = run("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", script], { env: childEnv(env) });
  if (r.error || r.status !== 0) throw new Error(`powershell failed (${r.error?.message ?? `exit ${r.status}`}): ${tail(r.out, 5)}`);
  return r.stdout.trim();
}

// --- tests (child processes) -------------------------------------------------------------------------

function checkCargoTests() {
  // No sccache wrapper: a churned sccache served broken artifacts here before (docs/lessons.md, phase 2.1).
  const r = run("cargo", ["test", "--workspace", "--locked"], { env: childEnv({ RUSTC_WRAPPER: "" }) });
  if (r.error) return report(false, "cargo-tests", `cargo not runnable: ${r.error.code ?? r.error.message}`);
  const lines = [...r.out.matchAll(/test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];
  const sum = (i) => lines.reduce((a, l) => a + Number(l[i]), 0);
  const [passed, failed, ignored] = [sum(2), sum(3), sum(4)];
  const ok = r.status === 0 && lines.length > 0 && lines.every((l) => l[1] === "ok") && passed > 0 && failed === 0 && ignored === 0;
  report(ok, "cargo-tests", `exit=${r.status} binaries=${lines.length} passed=${passed} failed=${failed} ignored=${ignored}`);
  if (!ok) console.error(tail(r.out, 60));
}

function checkUiTests() {
  const dir = mkdtempSync(path.join(tmpdir(), "ss-p2-vitest-"));
  const out = path.join(dir, "vitest.json");
  try {
    const entry = path.join(ROOT, "node_modules", "vitest", "vitest.mjs");
    const r = run(process.execPath, [entry, "run", "--reporter=json", `--outputFile=${out}`]);
    if (!existsSync(out)) {
      report(false, "ui-tests", `vitest wrote no report (exit ${r.status ?? r.error?.code})`);
      console.error(tail(r.out, 60));
      return;
    }
    const j = JSON.parse(readFileSync(out, "utf8"));
    const files = j.testResults ?? [];
    const badFiles = files.filter((f) => f.status !== "passed").map((f) => path.basename(f.name));
    const skipped = (j.numPendingTests ?? 0) + (j.numTodoTests ?? 0);
    const ok = r.status === 0 && j.success === true && files.length > 0 && j.numTotalTests > 0 && j.numFailedTests === 0 && skipped === 0 && badFiles.length === 0;
    report(ok, "ui-tests", `exit=${r.status} files=${files.length} tests=${j.numTotalTests} passed=${j.numPassedTests} failed=${j.numFailedTests} skipped=${skipped}${badFiles.length ? ` bad=${badFiles.join(",")}` : ""}`);
    if (!ok) console.error(tail(r.out, 60));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

// --- install: exe, Start Menu shortcut, no Desktop shortcut ------------------------------------------------

// tauri-bundler stamps the package type into the exe copy it packs (`..._NSS` for NSIS) and restores `..._UNK` in
// target\release afterwards (tauri-bundler 2.10 bundle.rs BUNDLE_VAR_TOKEN), so the two differ in exactly that marker.
const BUNDLE_MARK = "__TAURI_BUNDLE_TYPE_VAR_";

/** Same build = same bytes once the installed copy's NSIS stamp is put back to the unbundled one. */
function sameBuild(installedExe, builtExe) {
  const a = readFileSync(installedExe);
  const b = readFileSync(builtExe);
  if (a.length !== b.length) return { same: false, why: `size ${a.length} vs ${b.length} bytes` };
  const nss = Buffer.from(`${BUNDLE_MARK}NSS`);
  const unk = Buffer.from(`${BUNDLE_MARK}UNK`);
  const at = a.indexOf(nss);
  if (at < 0 || b.indexOf(unk) !== at) return { same: false, why: `no ${BUNDLE_MARK}NSS stamp at the built exe's ${BUNDLE_MARK}UNK offset` };
  unk.copy(a, at);
  const hash = (buf) => createHash("sha256").update(buf).digest("hex").slice(0, 16);
  return a.equals(b) ? { same: true, why: `sha256 ${hash(b)} (NSIS stamp normalized)` } : { same: false, why: "bytes differ beyond the NSIS stamp" };
}

function checkInstalledExe() {
  if (!existsSync(INSTALLED_EXE)) {
    report(false, "installed-exe", `${INSTALLED_EXE} missing: build with \`cargo tauri build\`, then run the installer with /S /D=${INSTALL_DIR}`);
    return false;
  }
  if (!existsSync(APP_EXE)) {
    report(false, "installed-exe", `${APP_EXE} missing: cannot tell whether the installed exe is the current build`);
    return false;
  }
  const { same, why } = sameBuild(INSTALLED_EXE, APP_EXE);
  report(same, "installed-exe", same ? `${INSTALLED_EXE} = ${APP_EXE}, ${why}` : `${INSTALLED_EXE} is not ${APP_EXE} (${why}): re-run the installer`);
  return true;
}

function checkShortcuts() {
  const programs = path.join(process.env.APPDATA ?? "", "Microsoft", "Windows", "Start Menu", "Programs");
  let lnks = null; // null = the folder could not be listed (already reported)
  try {
    lnks = readdirSync(programs, { recursive: true, withFileTypes: true })
      .filter((d) => d.isFile() && d.name.toLowerCase() === LNK_NAME.toLowerCase())
      .map((d) => path.join(d.parentPath, d.name));
  } catch (e) {
    report(false, "start-menu", `cannot list ${programs}: ${e.code ?? e.message}`);
  }
  // One child PowerShell: every shortcut's TargetPath plus the (possibly OneDrive-redirected) Desktop folder.
  const script = [
    "$sh = New-Object -ComObject WScript.Shell",
    "$t = @($env:SS_LNKS -split [char]10 | Where-Object { $_ } | ForEach-Object { $sh.CreateShortcut($_).TargetPath })",
    "@{ desktop = [Environment]::GetFolderPath('Desktop'); targets = $t } | ConvertTo-Json -Compress",
  ].join("; ");
  let info;
  try {
    info = JSON.parse(powershell(script, { SS_LNKS: (lnks ?? []).join("\n") }));
  } catch (e) {
    if (lnks) report(false, "start-menu", e.message.split("\n")[0]);
    report(false, "no-desktop-lnk", e.message.split("\n")[0]);
    return;
  }
  if (lnks) {
    const targets = [info.targets ?? []].flat();
    const norm = (p) => path.resolve(p).toLowerCase();
    const hit = lnks.findIndex((_, i) => targets[i] && norm(targets[i]) === norm(INSTALLED_EXE));
    report(
      hit >= 0,
      "start-menu",
      hit >= 0 ? `${lnks[hit]} -> ${targets[hit]}` : `${lnks.length} shortcut(s) named ${LNK_NAME}, none targets ${INSTALLED_EXE} (${targets.join(", ") || "none"})`,
    );
  }
  const desktopLnk = path.join(info.desktop, LNK_NAME);
  const onDesktop = existsSync(desktopLnk);
  report(!onDesktop, "no-desktop-lnk", onDesktop ? `${desktopLnk} exists (Desktop = ${info.desktop})` : `none in ${info.desktop}`);
}

// --- launch: window, single instance, close ------------------------------------------------------------

/** Real data dir (no SS_DATA_DIR) and no CDP flags inherited from the caller's environment. */
const launchEnv = () => childEnv({ SS_DATA_DIR: undefined, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: undefined });

/** Spawns the installed exe; `exited` resolves with the exit code (or the spawn error code). */
function launch() {
  const startedAt = Date.now();
  const proc = spawn(INSTALLED_EXE, [], { cwd: INSTALL_DIR, env: launchEnv(), stdio: "ignore" });
  const exited = new Promise((res) => {
    proc.once("exit", (code) => res(code));
    proc.once("error", (e) => res(e.code ?? e.message));
  });
  return { pid: proc.pid, startedAt, exited };
}

const within = (promise, ms) => Promise.race([promise, sleep(ms).then(() => "timeout")]);

/** Epoch ms when process `pid` first shows a main window titled TITLE ("exited" / "timeout" otherwise). */
function waitWindowTitle(pid, deadlineMs) {
  const script = [
    "$deadline = [DateTimeOffset]::FromUnixTimeMilliseconds([int64]$env:SS_DEADLINE)",
    "while ([DateTimeOffset]::UtcNow -lt $deadline) {",
    "  $p = Get-Process -Id ([int]$env:SS_PID) -ErrorAction SilentlyContinue",
    "  if (-not $p) { 'exited'; exit }",
    "  if ($p.MainWindowTitle -eq $env:SS_TITLE) { [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds(); exit }",
    "  Start-Sleep -Milliseconds 50",
    "}",
    "'timeout'",
  ].join("\n");
  return powershell(script, { SS_PID: String(pid), SS_TITLE: TITLE, SS_DEADLINE: String(deadlineMs) });
}

/** msedgewebview2.exe processes: { pid, ppid, cmd }. */
function webviewProcs() {
  const out = powershell(
    "@(Get-CimInstance Win32_Process -Filter \"Name='msedgewebview2.exe'\" | Select-Object ProcessId,ParentProcessId,CommandLine) | ConvertTo-Json -Compress",
  );
  return [JSON.parse(out || "[]")].flat().map((p) => ({ pid: p.ProcessId, ppid: p.ParentProcessId, cmd: p.CommandLine ?? "" }));
}

/** True for a WebView2 process of this app (either profile location: the data dir, or the old %LOCALAPPDATA% one). */
const isOurWebview = (p) => {
  const cmd = p.cmd.toLowerCase();
  return cmd.includes(WEBVIEW_DIR.toLowerCase()) || cmd.includes(`\\${IDENTIFIER}\\`);
};

/** `--user-data-dir` of the WebView2 browser process started by `appPid`, or null. */
function userDataDirOf(appPid) {
  const browser = webviewProcs().find((p) => p.ppid === appPid && p.cmd.includes("--user-data-dir"));
  const m = browser?.cmd.match(/--user-data-dir=(?:"([^"]+)"|(\S+))/);
  return m ? (m[1] ?? m[2]) : null;
}

/**
 * Asks the app to close the way a user does (taskkill without /F posts WM_CLOSE), then waits until the app and
 * every WebView2 process of its profile are gone. Kills the tree if not. Returns { graceful, ms, detail }.
 */
async function closeApp(pid, exited) {
  const t0 = Date.now();
  run("taskkill", ["/PID", String(pid)]);
  const gone = exited
    ? (await within(exited, CLOSE_MAX_MS)) !== "timeout"
    : await (async () => {
        while (Date.now() - t0 < CLOSE_MAX_MS) {
          if (!runningPids(APP_EXE_NAME).includes(pid)) return true;
          await sleep(200);
        }
        return false;
      })();
  let left = [];
  while (gone && Date.now() - t0 < CLOSE_MAX_MS) {
    left = webviewProcs().filter(isOurWebview);
    if (!left.length) break;
    await sleep(200);
  }
  const ms = Date.now() - t0;
  if (gone && !left.length) return { graceful: true, ms, detail: `exited in ${ms}ms, WebView2 gone` };
  killTree(pid);
  for (const p of webviewProcs().filter(isOurWebview)) killTree(p.pid);
  return { graceful: false, ms, detail: gone ? `WebView2 pid(s) ${left.map((p) => p.pid).join(",")} still running after ${ms}ms: killed` : `still running after ${ms}ms: killed` };
}

async function checkLaunch() {
  const first = launch();
  let closed = false;
  try {
    const seen = waitWindowTitle(first.pid, first.startedAt + 3 * WINDOW_MAX_MS);
    const ms = /^\d+$/.test(seen) ? Number(seen) - first.startedAt : null;
    report(ms !== null && ms <= WINDOW_MAX_MS, "window", ms !== null ? `"${TITLE}" after ${ms}ms (max ${WINDOW_MAX_MS}ms), pid ${first.pid}` : `no "${TITLE}" window: ${seen}`);
    if (ms === null) return;

    const second = launch();
    const code = await within(second.exited, SECOND_EXIT_MS);
    const pids = runningPids(APP_EXE_NAME);
    const ok = code !== "timeout" && pids.length === 1 && pids[0] === first.pid;
    report(ok, "single-instance", `second launch pid ${second.pid} ${code === "timeout" ? `still running after ${SECOND_EXIT_MS}ms` : `exited ${code}`}; running: ${pids.join(",") || "none"} (first ${first.pid})`);
    if (code === "timeout") killTree(second.pid);

    const c = await closeApp(first.pid, first.exited);
    closed = true;
    report(c.graceful, "close", c.detail);
  } finally {
    if (!closed) await closeApp(first.pid, first.exited);
  }
}

// --- memory after scrolling the whole real library ---------------------------------------------------------

/**
 * Pages through the grid one screen at a time; on each screen waits (up to PAGE_MAX_WAIT_MS) until no visible cell
 * shows a shimmer or a still-loading <img>. Returns { pages, waitedOut, seen, loaded, icons } where `seen` counts
 * distinct item indices shown, `loaded` those whose thumbnail decoded, `icons` those that have no thumbnail
 * (cloud-only or undecodable: an icon instead of an image).
 */
function pageThrough(page) {
  return page.evaluate(
    async ({ sel, settleMs, maxWaitMs }) => {
      const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
      const el = document.querySelector(sel);
      const max = el.scrollHeight - el.clientHeight;
      const seen = new Set();
      const loaded = new Set();
      const icons = new Set();
      let pages = 0;
      let waitedOut = 0;
      const visible = () => {
        const v = el.getBoundingClientRect();
        return [...el.querySelectorAll('[role="gridcell"]')].filter((c) => {
          const r = c.getBoundingClientRect();
          return r.bottom > v.top && r.top < v.bottom;
        });
      };
      for (let top = 0; ; top = Math.min(max, top + el.clientHeight)) {
        el.scrollTop = top;
        pages++;
        await sleep(settleMs);
        const end = performance.now() + maxWaitMs;
        let cells;
        for (;;) {
          cells = visible();
          const busy = cells.some((c) => c.querySelector(".ss-shimmer") || [...c.querySelectorAll("img")].some((i) => !i.complete));
          if (!busy) break;
          if (performance.now() > end) {
            waitedOut++;
            break;
          }
          await sleep(25);
        }
        for (const c of cells) {
          const i = Number(c.dataset.index);
          seen.add(i);
          const img = c.querySelector("img");
          if (img && img.complete && img.naturalWidth > 0) loaded.add(i);
          else if (!img && !c.querySelector(".ss-shimmer")) icons.add(i);
        }
        if (top >= max) break;
      }
      for (const i of loaded) icons.delete(i);
      return { pages, waitedOut, seen: seen.size, loaded: loaded.size, icons: icons.size };
    },
    { sel: GRID, settleMs: PAGE_SETTLE_MS, maxWaitMs: PAGE_MAX_WAIT_MS },
  );
}

/**
 * Samples the app tree's memory from now until IDLE_MS have passed with no input (the caller just finished a
 * scroll). Returns { peak, last, count }: `peak` is the sample with the most private bytes, `last` the final one.
 */
async function idleSamples(page, pid) {
  const end = Date.now() + IDLE_MS;
  const samples = [treeMemory(pid)];
  while (Date.now() < end) {
    await page.waitForTimeout(Math.min(SAMPLE_GAP_MS, Math.max(0, end - Date.now())));
    samples.push(treeMemory(pid));
  }
  const peak = samples.reduce((a, b) => (b.privateMB > a.privateMB ? b : a));
  return { peak, last: samples.at(-1), count: samples.length };
}

/** Returns the WebView2 `--user-data-dir` seen while the app ran (for webview-data), or null. */
async function checkMemory() {
  let app;
  let cdp;
  let userDataDir = null;
  try {
    app = await startApp({ exe: INSTALLED_EXE }); // real data dir: no SS_DATA_DIR
    cdp = await connectApp(app.cdpUrl);
    const items = await waitForGrid(cdp.page);
    userDataDir = userDataDirOf(app.pid);
    await cdp.page.waitForTimeout(1000);
    const idle = treeMemory(app.pid);
    console.log(`INFO memory-idle processes=${idle.processes} workingSet=${idle.workingSetMB}MB private=${idle.privateMB}MB (first screen, ${items} items)`);
    const { frames, scrolledPx, maxPx } = await scrollFrames(cdp.page, SCROLL_MS);
    const scroll = await idleSamples(cdp.page, app.pid);
    const reachedBottom = Math.round(scrolledPx) >= Math.round(maxPx);
    report(
      scroll.peak.privateMB <= MEMORY_MAX_MB && reachedBottom,
      "memory",
      `peak=${scroll.peak.privateMB}MB (max ${MEMORY_MAX_MB}MB) afterIdle=${scroll.last.privateMB}MB workingSetPeak=${scroll.peak.workingSetMB}MB samples=${scroll.count}/${IDLE_MS / 1000}s processes=${scroll.peak.processes} (${scroll.peak.names}) items=${items} scrolled=${Math.round(scrolledPx)}/${Math.round(maxPx)}px p50=${percentile(frames, 50).toFixed(2)}ms p95=${percentile(frames, 95).toFixed(2)}ms`,
    );
    const t0 = Date.now();
    const pass = await pageThrough(cdp.page);
    const visit = await idleSamples(cdp.page, app.pid);
    const unloaded = pass.seen - pass.loaded - pass.icons;
    const covered = pass.seen === items && unloaded <= items * MAX_UNLOADED_SHARE;
    console.log(
      `INFO memory-visit-all peak=${visit.peak.privateMB}MB afterIdle=${visit.last.privateMB}MB (not gated, Deferred decision #31) covered=${covered} items=${items} visited=${pass.seen} thumbsLoaded=${pass.loaded} noThumb=${pass.icons} unloaded=${unloaded} pages=${pass.pages} waitedOut=${pass.waitedOut} pass=${Math.round((Date.now() - t0) / 1000)}s`,
    );
  } catch (e) {
    report(false, "memory", e.message.split("\n")[0]);
    console.error(e.stack ?? e.message);
  } finally {
    await cdp?.close().catch(() => {});
    if (app) {
      const c = await closeApp(app.pid, null);
      if (!c.graceful) console.error(`memory run: app close ${c.detail}`);
    }
  }
  return userDataDir;
}

function checkWebviewData(userDataDir) {
  const oldProfile = path.join(process.env.LOCALAPPDATA ?? "", IDENTIFIER, "EBWebView");
  const newProfile = path.join(WEBVIEW_DIR, "EBWebView");
  const underDataDir = !!userDataDir && path.resolve(userDataDir).toLowerCase().startsWith(`${WEBVIEW_DIR.toLowerCase()}\\`);
  const ok = underDataDir && existsSync(newProfile) && !existsSync(oldProfile);
  report(ok, "webview-data", `--user-data-dir=${userDataDir ?? "not seen"} ${newProfile}=${existsSync(newProfile) ? "present" : "missing"} ${oldProfile}=${existsSync(oldProfile) ? "present" : "absent"}`);
}

// --- main ------------------------------------------------------------------------------------------------

async function main() {
  const pids = runningPids(APP_EXE_NAME);
  report(pids.length === 0, "app-not-running", pids.length ? `running pid(s) ${pids.join(",")}: close Screenshot Studio first` : "none");
  if (pids.length) return 1;

  checkCargoTests();
  checkUiTests();
  const installed = checkInstalledExe();
  checkShortcuts();
  let userDataDir = null;
  if (installed) {
    await checkLaunch();
    userDataDir = await checkMemory();
  } else {
    for (const c of ["window", "single-instance", "close", "memory"]) report(false, c, "not run: app not installed");
  }
  checkWebviewData(userDataDir);

  const failed = results.filter((ok) => !ok).length;
  console.log(`${failed === 0 ? "PASS" : "FAIL"} p2 ${results.length - failed}/${results.length} checks passed`);
  return failed === 0 ? 0 : 1;
}

process.exitCode = await main();
