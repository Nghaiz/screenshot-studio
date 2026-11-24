#!/usr/bin/env node
// Phase 3.4 Accept (P3: OCR hit, planted duplicate, sidecar absent, bundled sidecar; AX-01), extended by phases
// 3.4b and 3.4c:
//   node scripts/accept/p3.mjs [--only tests,fixtures,app]
//
// Prints one stable line per check on stdout:  PASS|FAIL <check> <value>   (INFO lines are measured-only, never
// counted). Exit 0 only when every check passed, 1 otherwise. No prompts. `--only` runs some groups for diagnosis;
// such a partial run always ends `FAIL p3 partial run` and exits 1, so it can never stand in for the Accept.
//
// Checks, in order (plan phase-3.4 steps 1-2):
//   app-not-running   no screenshot-studio.exe is running (single-instance would hijack the launch, README D10)
//   build             `cargo build --release -p ss-core --bin ssctl --locked` (target\release\ssctl.exe is what runs)
// group tests:
//   lane-tests        scripts/accept/lane-tests.mjs over every test file of phases 1.1-3.3: the files each phase's
//                     `Accept:` line names plus every test file on disk (crates/*/tests/*.rs, src/**/*.test.ts(x),
//                     sidecar/tests/test_*.py); exit 0 and one PASS line per file
// group fixtures (generated library + own data dir under %TEMP%, README D10; the real sidecar venv):
//   fixtures          lib/p3_fixtures.py (sidecar venv Python, Pillow): 50 images, #23 renders `Hoá đơn 2026
//                     INVOICE-7731`, planted-exact.png = the bytes of #7, planted-similar.jpg = #9 re-encoded JPEG
//   index             `ssctl add-root` + `ssctl index`: total == thumbOk == 52
//   dupes-similar-no-ai  (3.4c) `ssctl dupes --mode similar --json` before the AI pass (no OCR text, no
//                     embedding: the hashes and the thumbnails' pixels decide, README Deferred decision #35): a
//                     group {#9, planted-similar.jpg}; any other group is the exact pair
//   ai                `ssctl ai --tasks ocr,embed --json`: per task done == candidates == 52, failed == skippedCloud == 0
//   search-ocr        `ssctl search "INVOICE-7731" --json` returns the invoice (no file name carries the text)
//   search-unaccented `ssctl search "hoa don" --json` returns it too (no diacritics, `đ` typed as `d`)
//   dupes-exact       `ssctl dupes --mode exact --json`: exactly one group, {#7, planted-exact.png}
//   dupes-similar     `ssctl dupes --mode similar --json` after the AI pass (OCR text and embeddings stored, so
//                     they prune too): a group {#9, planted-similar.jpg}; any other group is the exact pair
//   sidecar-absent    `ssctl set sidecarVenv "\"<temp>/no-venv\""`, then `ssctl status --json` reports `absent`
//   search-absent     ... and `ssctl search "INVOICE" --json` exits 0 with the invoice (FTS over stored OCR text)
// group app (the INSTALLED app, NSIS bundle of phase 2.3 + the sidecar resources of this phase):
//   installed-exe     <install dir>\screenshot-studio.exe is target\release's build (lib/install.mjs)
//   bundled-files     <install dir>\sidecar holds exactly the tracked sidecar/ss_sidecar/**/*.py, byte-identical
//                     (no tests, no caches, no __pycache__: since 3.4b the sidecar writes none there), and
//                     <install dir>\contracts holds exactly sidecar-jsonrpc.schema.json, byte-identical
//                     (tauri.conf.json bundle.resources)
//   bundled-sidecar   the installed app, started on a fresh temp data dir with SS_SIDECAR_DIR naming a dir that does
//                     not exist, reaches sidecar state `ready` (get_sidecar_status over CDP): the dev-checkout
//                     fallback is disabled that way, so only the bundled source + schema can have been used
//                     (a missing schema answers `initialize` with -32000: state `error`)
//   no-console-window from launch until 3 s after `ready`: no new visible console window (lib/windows.mjs: window
//                     hook + poll, class/name rules) and no sidecar process whose console has a window
//                     (AttachConsole + GetConsoleWindow on each python.exe of the sidecar subtree, which must exist)
//   sidecar-exit      after a user-style close (WM_CLOSE) the app exits and no sidecar process is left
//   INFO sidecar-processes  the sidecar subtree's executables and command lines
//   smart-view-ready  (3.4b; one line per view: stats, duplicates, cleanup, settings-ai) the installed app on the REAL
//                     data dir (read-only: views are opened, nothing in them is clicked), started cold; as soon as the
//                     grid shows a thumbnail, each view is opened from the library by a click in the page and timed
//                     to the frame after its content is in the DOM (requestAnimationFrame; value in ms). FAIL over
//                     SMART_VIEW_READY_MAX_MS, or when any frame in between lost the shell (sidebar or main area
//                     gone or zero-sized) or showed an empty pane (neither the content nor a labelled
//                     role=status aria-busy loading state). The app is then closed with WM_CLOSE.
//   no-pycache-in-install-dir  (3.4b) after both launches, <install dir> holds no __pycache__ dir and no .pyc, and the
//                     bundled-sidecar run's temp data dir holds the bytecode of <install dir>\sidecar under
//                     sidecar-pycache (PYTHONPYCACHEPREFIX): proof Python compiled the bundled source, just elsewhere
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import process from "node:process";
import { connectApp, waitForGrid } from "./lib/cdp.mjs";
import { INSTALL_DIR, INSTALLED_EXE, installedExeCheck, powershell } from "./lib/install.mjs";
import { APP_EXE_NAME, ROOT, childEnv, closeApp as closeAppIn, isSidecarRoot, killTree, run, runningPids, ssctl, startApp, treeMemory } from "./lib/run.mjs";
import { isConsoleWindow, startWindowProbe } from "./lib/windows.mjs";

const PLAN_DIR = path.join(ROOT, "plans", "261123-screenshot-studio");
const LANE_TESTS = path.join(ROOT, "scripts", "accept", "lane-tests.mjs");
const FIXTURES_PY = path.join(ROOT, "scripts", "accept", "lib", "p3_fixtures.py");
const DEFAULT_SIDECAR_VENV = "D:\\IOT_Tools\\data\\ScreenshotStudio\\sidecar-venv"; // contract-tauri-ipc § Setting keys
const SIDECAR_VENV = process.env.SS_SIDECAR_VENV || DEFAULT_SIDECAR_VENV; // same override as lane-tests.mjs
const PHASES = { from: [1, 1], to: [3, 3] }; // the phases whose test files lane-tests must run
const FIXTURE_TOTAL = 52; // 50 generated + 2 planted
const READY_TIMEOUT_MS = 180_000; // sidecar `initialize` answers in ~11-14 s on CUDA (phase 3.1/3.2), 120 s budget
const CONSOLE_SETTLE_MS = 3000; // keep watching for a console window after `ready`
const CLOSE_MAX_MS = 10_000;
const GROUPS = ["tests", "fixtures", "app"];
const PYCACHE_DIR = "sidecar-pycache"; // ss-core paths::SIDECAR_PYCACHE_DIR, under the data dir
// smart-view-ready budget. Measured 2026-11-24 on the installed app, real library, cold start, sidecar loading: with
// the views as lazy chunks every view took 306-325 ms to its content although its chunk loaded in 1-2 ms and its IPC
// in <= 6 ms: React 19 holds a Suspense fallback for >= 300 ms (FALLBACK_THROTTLE_MS). Eager views (phase 3.4b):
// 10-29 ms. 150 ms is ~5x the slowest eager open and half that 300 ms floor, so a view that suspends again fails.
const SMART_VIEW_READY_MAX_MS = 150;
const SMART_VIEW_TIMEOUT_MS = 15_000; // keep timing a slow view past the budget, to report a real number
const GRID_READY_MS = 60_000;
// How each view is opened and what "content" means (language-independent hooks: data-nav, data-testid).
const SMART_VIEWS = [
  { view: "stats", open: '[data-nav="stats"]', ready: 'main section [data-testid^="tile-"]' },
  { view: "duplicates", open: '[data-nav="duplicates"]', ready: 'main [data-testid="dup-summary"]' },
  { view: "cleanup", open: '[data-nav="cleanup"]', ready: 'main [data-testid="clean-start"]' },
  { view: "settings-ai", open: '[data-testid="ai-pill"]', ready: '[role="dialog"] [data-testid="ai-state"]', close: '[role="dialog"] .ss-btn' },
];

const results = [];
function report(ok, check, value) {
  results.push(ok);
  console.log(`${ok ? "PASS" : "FAIL"} ${check} ${value}`);
}
const tail = (s, n = 30) => s.trim().split(/\r?\n/).slice(-n).join("\n");
const sleep = (ms) => new Promise((res) => setTimeout(res, ms));
const rel = (p) => path.relative(ROOT, p).split(path.sep).join("/");

// --- build + lane tests -------------------------------------------------------------------------------

function checkBuild() {
  // No sccache wrapper: a churned sccache served broken artifacts here before (docs/lessons.md, phase 2.1).
  const r = run("cargo", ["build", "--release", "-p", "ss-core", "--bin", "ssctl", "--locked"], { env: childEnv({ RUSTC_WRAPPER: "" }) });
  const ok = !r.error && r.status === 0;
  report(ok, "build", ok ? "target/release/ssctl.exe up to date" : `cargo exit ${r.status ?? r.error?.code}`);
  if (!ok) console.error(tail(r.out, 40));
  return ok;
}

/** [major, minor] of a phase file name (`phase-3.2-core-smart.md`), or null. */
function phaseOf(file) {
  const m = /^phase-(\d+)\.(\d+)[a-z]?-.*\.md$/.exec(file);
  return m ? [Number(m[1]), Number(m[2])] : null;
}
const cmp = (a, b) => a[0] - b[0] || a[1] - b[1];

/** Test files of phases 1.1-3.3: named by their `Accept:` lane-tests lines, plus every test file on disk. */
function laneTestFiles() {
  const named = new Set();
  for (const f of readdirSync(PLAN_DIR)) {
    const v = phaseOf(f);
    if (!v || cmp(v, PHASES.from) < 0 || cmp(v, PHASES.to) > 0) continue;
    const m = /^Accept: `node scripts\/accept\/lane-tests\.mjs ([^`]+)`/m.exec(readFileSync(path.join(PLAN_DIR, f), "utf8"));
    if (m) for (const t of m[1].trim().split(/\s+/)) named.add(t);
  }
  const onDisk = new Set();
  const add = (dir, re) => {
    if (!existsSync(dir)) return;
    for (const d of readdirSync(dir, { recursive: true, withFileTypes: true })) {
      const p = rel(path.join(d.parentPath, d.name));
      if (d.isFile() && re.test(p)) onDisk.add(p);
    }
  };
  add(path.join(ROOT, "crates"), /^crates\/[^/]+\/tests\/[^/]+\.rs$/);
  add(path.join(ROOT, "src"), /^src\/.+\.test\.tsx?$/);
  add(path.join(ROOT, "sidecar", "tests"), /^sidecar\/tests\/test_[^/]+\.py$/);
  return { files: [...new Set([...named, ...onDisk])].sort(), named, onDisk };
}

function checkLaneTests() {
  const { files, named, onDisk } = laneTestFiles();
  const unnamed = [...onDisk].filter((f) => !named.has(f));
  if (unnamed.length) console.log(`INFO lane-tests-unnamed ${unnamed.join(" ")} (on disk, named by no phase Accept; run anyway)`);
  const r = run(process.execPath, [LANE_TESTS, ...files], { env: childEnv({ RUSTC_WRAPPER: "" }) });
  const lines = r.stdout.split(/\r?\n/).filter((l) => /^(PASS|FAIL) /.test(l));
  const passed = lines.filter((l) => l.startsWith("PASS ")).length;
  const kinds = (re) => files.filter((f) => re.test(f)).length;
  const ok = !r.error && r.status === 0 && files.length > 0 && passed === files.length && lines.length === files.length;
  report(
    ok,
    "lane-tests",
    `exit=${r.status ?? r.error?.code} files=${files.length} passed=${passed} (cargo=${kinds(/\.rs$/)} vitest=${kinds(/\.tsx?$/)} pytest=${kinds(/\.py$/)})`,
  );
  if (!ok) console.error(`${lines.filter((l) => l.startsWith("FAIL ")).join("\n")}\n${tail(r.out, 60)}`);
}

// --- fixture library: OCR hit, planted duplicates, sidecar absent ---------------------------------------------

/** Every image of the fixture catalog: Map fileName -> id (and back). */
function catalogIds(dataDir) {
  const r = ssctl(["list", "--limit", "500"], { dataDir }); // ss-core query LIMIT_MAX; the fixture has 52
  if (r.status !== 0 || !r.json?.items) throw new Error(`ssctl list exit ${r.status}: ${tail(r.out, 5)}`);
  const byName = new Map(r.json.items.map((i) => [i.fileName, i.id]));
  const byId = new Map(r.json.items.map((i) => [i.id, i.fileName]));
  return { byName, byId };
}

/** `ssctl search <text>` (mode any): { ok, value } for "the invoice is among the hits". */
function searchHas(dataDir, text, want) {
  const r = ssctl(["search", text], { dataDir });
  const items = r.json?.items ?? [];
  const rank = items.findIndex((i) => i.fileName === want);
  const ok = r.status === 0 && rank >= 0;
  const value = `"${text}" exit=${r.status} hits=${items.length}${rank >= 0 ? ` ${want} at rank ${rank + 1}` : ` ${want} missing (${items.map((i) => i.fileName).join(",") || "none"})`}`;
  if (r.status !== 0) console.error(tail(r.out, 10));
  return { ok, value };
}

/** Groups of `ssctl dupes --mode <mode>` as sorted file-name lists. */
function dupeGroups(dataDir, mode, byId) {
  const r = ssctl(["dupes", "--mode", mode], { dataDir });
  if (r.status !== 0 || !r.json?.groups) throw new Error(`ssctl dupes --mode ${mode} exit ${r.status}: ${tail(r.out, 5)}`);
  return r.json.groups.map((g) => g.imageIds.map((id) => byId.get(id) ?? `#${id}`).sort());
}
const sameSet = (a, b) => a.length === b.length && [...a].sort().every((x, i) => x === [...b].sort()[i]);
const show = (groups) => groups.map((g) => `{${g.join(",")}}`).join(" ") || "none";

/** `similar` groups: the planted pair {#9, its JPEG} is one; any other group is the planted exact pair. */
function checkSimilar(dataDir, fx, check) {
  try {
    const similar = dupeGroups(dataDir, "similar", catalogIds(dataDir).byId);
    const hit = similar.some((g2) => sameSet(g2, fx.similar));
    const strays = similar.filter((g2) => !sameSet(g2, fx.similar) && !sameSet(g2, fx.exact));
    report(hit && strays.length === 0, check, `groups=${similar.length} ${show(similar)} expected {${fx.similar.join(",")}} (+ the exact pair)`);
  } catch (e) {
    report(false, check, e.message);
  }
}

function checkFixtures(tmp) {
  const lib = path.join(tmp, "lib");
  const dataDir = path.join(tmp, "data");
  mkdirSync(dataDir, { recursive: true });

  const python = path.join(SIDECAR_VENV, "Scripts", "python.exe");
  const g = run(python, ["-I", FIXTURES_PY, lib]);
  let fx = null;
  try {
    fx = g.status === 0 ? JSON.parse(g.stdout.trim()) : null;
  } catch {
    // reported below
  }
  const written = existsSync(lib) ? readdirSync(lib).length : 0;
  const fxOk = !!fx && fx.images.length === FIXTURE_TOTAL && written === FIXTURE_TOTAL;
  report(fxOk, "fixtures", fx ? `${written} files in ${lib} (invoice ${fx.invoice}: "${fx.invoiceText}", exact ${fx.exact.join("=")}, similar ${fx.similar.join("~")})` : `generator exit ${g.status ?? g.error?.code} (${python}): ${tail(g.out, 3)}`);
  if (!fxOk) return;

  const root = ssctl(["add-root", lib], { dataDir });
  const idx = root.status === 0 ? ssctl(["index"], { dataDir }) : root;
  const counts = idx.json ?? {};
  report(idx.status === 0 && counts.total === FIXTURE_TOTAL && counts.thumbOk === FIXTURE_TOTAL, "index", `exit=${idx.status} total=${counts.total} thumbOk=${counts.thumbOk} expected=${FIXTURE_TOTAL}`);
  if (idx.status !== 0) {
    console.error(tail(idx.out, 20));
    return;
  }
  checkSimilar(dataDir, fx, "dupes-similar-no-ai");

  const t0 = Date.now();
  const ai = ssctl(["ai", "--tasks", "ocr,embed"], { dataDir });
  const tasks = ai.json?.tasks ?? [];
  const byTask = (t) => tasks.find((x) => x.task === t);
  const taskOk = (t) => {
    const x = byTask(t);
    return !!x && x.candidates === FIXTURE_TOTAL && x.done === FIXTURE_TOTAL && x.failed === 0 && x.skippedCloud === 0;
  };
  const fmt = (t) => {
    const x = byTask(t);
    return x ? `${t} ${x.done}/${x.candidates} failed=${x.failed} skippedCloud=${x.skippedCloud}` : `${t} missing`;
  };
  const sc = ai.json?.sidecar ?? {};
  report(
    ai.status === 0 && taskOk("ocr") && taskOk("embed"),
    "ai",
    `exit=${ai.status} ${fmt("ocr")}; ${fmt("embed")}; sidecar=${sc.state ?? "?"} device=${sc.device ?? "?"} elapsed=${ai.json?.elapsedMs ?? Date.now() - t0}ms`,
  );
  if (ai.status !== 0) console.error(tail(ai.out, 20));

  let ids;
  try {
    ids = catalogIds(dataDir);
  } catch (e) {
    for (const c of ["search-ocr", "search-unaccented", "dupes-exact", "dupes-similar"]) report(false, c, e.message);
    return;
  }
  const s1 = searchHas(dataDir, "INVOICE-7731", fx.invoice);
  report(s1.ok, "search-ocr", s1.value);
  const s2 = searchHas(dataDir, "hoa don", fx.invoice);
  report(s2.ok, "search-unaccented", s2.value);

  try {
    const exact = dupeGroups(dataDir, "exact", ids.byId);
    report(exact.length === 1 && sameSet(exact[0], fx.exact), "dupes-exact", `groups=${exact.length} ${show(exact)} expected {${fx.exact.join(",")}}`);
  } catch (e) {
    report(false, "dupes-exact", e.message);
  }
  checkSimilar(dataDir, fx, "dupes-similar");

  // Sidecar absent: point the venv setting at a folder that does not exist (contract-tauri-ipc § Setting keys).
  const noVenv = path.join(tmp, "no-venv").split(path.sep).join("/");
  const set = ssctl(["set", "sidecarVenv", JSON.stringify(noVenv)], { dataDir });
  const st = set.status === 0 ? ssctl(["status"], { dataDir }) : set;
  report(set.status === 0 && st.status === 0 && st.json?.state === "absent", "sidecar-absent", `set exit=${set.status} status exit=${st.status} state=${st.json?.state ?? "?"} detail=${JSON.stringify(st.json?.detail ?? null)}`);
  const s3 = searchHas(dataDir, "INVOICE", fx.invoice);
  report(s3.ok, "search-absent", s3.value);
}

// --- the installed app: bundled sidecar, no console window, clean exit ----------------------------------------

/** Every file under `dir`, as paths relative to it with `/` (empty when the dir is missing). */
function filesUnder(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { recursive: true, withFileTypes: true })
    .filter((d) => d.isFile())
    .map((d) => path.relative(dir, path.join(d.parentPath, d.name)).split(path.sep).join("/"));
}

function checkBundledFiles() {
  const tracked = run("git", ["ls-files", "--", "sidecar/ss_sidecar", "contracts/sidecar-jsonrpc.schema.json"]);
  if (tracked.status !== 0) return report(false, "bundled-files", `git ls-files exit ${tracked.status}: ${tail(tracked.out, 3)}`);
  const want = tracked.stdout.split(/\r?\n/).filter((f) => /^sidecar\/ss_sidecar\/.+\.py$/.test(f) || f === "contracts/sidecar-jsonrpc.schema.json");
  const installed = [
    ...filesUnder(path.join(INSTALL_DIR, "sidecar")).map((f) => `sidecar/${f}`),
    ...filesUnder(path.join(INSTALL_DIR, "contracts")).map((f) => `contracts/${f}`),
  ];
  const missing = want.filter((f) => !installed.includes(f));
  const differ = want.filter((f) => installed.includes(f) && !readFileSync(path.join(INSTALL_DIR, f)).equals(readFileSync(path.join(ROOT, f))));
  const extra = installed.filter((f) => !want.includes(f));
  const sidecarPy = want.filter((f) => f.endsWith(".py")).length;
  const ok = sidecarPy > 0 && want.includes("contracts/sidecar-jsonrpc.schema.json") && !missing.length && !differ.length && !extra.length;
  report(
    ok,
    "bundled-files",
    `${INSTALL_DIR}: ${sidecarPy} sidecar .py + schema expected; missing=${missing.join(",") || "none"} differ=${differ.join(",") || "none"} extra=${extra.join(",") || "none"}`,
  );
}

/** Win32_Process rows { ProcessId, ParentProcessId, ExecutablePath, CommandLine } of those `pids` still running. */
function processRows(pids) {
  if (!pids.length) return [];
  const out = powershell(
    "$ids = $env:SS_PIDS -split ',' | ForEach-Object { [int]$_ }; " +
      "@(Get-CimInstance Win32_Process | Where-Object { $ids -contains $_.ProcessId } | Select-Object ProcessId,ParentProcessId,ExecutablePath,CommandLine) | ConvertTo-Json -Compress",
    { SS_PIDS: pids.join(",") },
  );
  return [JSON.parse(out || "[]")].flat();
}
const stillRunning = (pids) => processRows(pids).map((p) => p.ProcessId);

const closeApp = (pid) => closeAppIn(pid, CLOSE_MAX_MS);

/** Sidecar processes seen so far + the current sidecar subtree of the app, by PID. */
const mergeProcs = (seen, appPid) => [...new Map([...seen, ...treeMemory(appPid, { exclude: isSidecarRoot }).excluded.procs].map((p) => [p.pid, p])).values()];

/** The bundled-sidecar run's data dir (fresh, under the run's temp dir). */
const appDataDir = (tmp) => path.join(tmp, "app-data");

async function checkInstalledApp(tmp) {
  const inst = installedExeCheck();
  report(inst.ok, "installed-exe", inst.value);
  checkBundledFiles();
  if (!inst.present) {
    for (const c of ["bundled-sidecar", "no-console-window", "sidecar-exit"]) report(false, c, "not run: app not installed");
    return;
  }
  const dataDir = appDataDir(tmp);
  mkdirSync(dataDir, { recursive: true });
  const noSource = path.join(tmp, "no-sidecar-source"); // never created: disables the dev-checkout fallback
  let probe;
  let app;
  let cdp;
  let sidecar = [];
  let status = null;
  let readyMs = null;
  let seen = null;
  try {
    probe = await startWindowProbe();
    app = await startApp({ exe: INSTALLED_EXE, dataDir, env: { SS_SIDECAR_DIR: noSource } });
    cdp = await connectApp(app.cdpUrl);
    const end = app.startedAt + READY_TIMEOUT_MS;
    while (Date.now() < end) {
      status = await cdp.page.evaluate(() => window.__TAURI_INTERNALS__.invoke("get_sidecar_status"));
      sidecar = mergeProcs(sidecar, app.pid); // the launcher's child may come and go: keep every PID seen
      if (status.state !== "starting") break;
      await sleep(500);
    }
    if (status?.state === "ready") readyMs = Date.now() - app.startedAt;
    await sleep(CONSOLE_SETTLE_MS);
    sidecar = mergeProcs(sidecar, app.pid);
    const pythons = sidecar.filter((p) => /^pythonw?\.exe$/i.test(p.name)).map((p) => p.pid);
    seen = await probe.stop(stillRunning(pythons));
    probe = null;
    for (const p of processRows(sidecar.map((x) => x.pid))) {
      console.log(`INFO sidecar-processes pid=${p.ProcessId} ppid=${p.ParentProcessId} ${p.ExecutablePath ?? "?"} :: ${p.CommandLine ?? "?"}`);
    }
  } catch (e) {
    console.error(e.stack ?? e.message);
    if (!status) status = { state: `not read: ${e.message.split("\n")[0]}` };
  } finally {
    if (probe) await probe.stop([]).catch(() => {});
    await cdp?.close().catch(() => {});
  }

  report(
    status?.state === "ready",
    "bundled-sidecar",
    `state=${status?.state ?? "?"}${readyMs !== null ? ` after ${readyMs}ms` : ""} device=${status?.device ?? "?"} capabilities=${(status?.capabilities ?? []).join(",") || "none"} detail=${JSON.stringify(status?.detail ?? null)} (SS_SIDECAR_DIR=${noSource} does not exist)`,
  );

  if (seen) {
    const pids = sidecar.map((p) => p.pid);
    const consoleWins = seen.windows.filter((w) => isConsoleWindow(w, pids));
    const withWindow = seen.consoles.filter((c) => c.hwnd !== 0);
    const consoles = seen.consoles.map((c) => `pid${c.pid}:${c.attached ? `hwnd=${c.hwnd}${c.hwnd ? `(${c.class},visible=${c.visible})` : ""}` : `no console (error ${c.error})`}`);
    report(
      seen.hooked && seen.consoles.length > 0 && consoleWins.length === 0 && withWindow.length === 0,
      "no-console-window",
      `console=${consoleWins.map((w) => `${w.process}#${w.pid}:${w.class}`).join(",") || "none"} sidecarConsoles=${consoles.join(",") || "none inspected"} ` +
        `otherNewWindows=${seen.windows.filter((w) => !consoleWins.includes(w)).map((w) => `${w.process}:${w.class}`).join(",") || "none"} hooked=${seen.hooked} watched=${seen.ms}ms`,
    );
  } else {
    report(false, "no-console-window", "not measured (see stderr)");
  }

  if (!app) return report(false, "sidecar-exit", "not run: the app did not start");
  const c = await closeApp(app.pid);
  const pids = sidecar.map((p) => p.pid);
  let left = stillRunning(pids);
  const t0 = Date.now();
  while (left.length && Date.now() - t0 < CLOSE_MAX_MS) {
    await sleep(250);
    left = stillRunning(pids);
  }
  report(
    c.graceful && pids.length > 0 && left.length === 0,
    "sidecar-exit",
    `app ${c.graceful ? `exited ${c.ms}ms after WM_CLOSE` : `still running ${c.ms}ms after WM_CLOSE: killed`}; sidecar pids ${pids.join(",") || "none seen"}: ${left.length ? `still running ${left.join(",")}` : "gone"}`,
  );
  for (const pid of left) killTree(pid);
}

// --- the installed app on the real library: smart views open at once (phase 3.4b) ------------------------

/**
 * Opens `v` by a click inside the page and follows it frame by frame (requestAnimationFrame) until its content is
 * in the DOM, then one frame more (the frame that painted it). Each frame is classified: `content`, `loading` (a
 * labelled role=status aria-busy element), `empty` (neither: what the user sees as a blank pane) or `shell-gone`
 * (the sidebar or the main area missing or zero-sized).
 */
function timeView(page, v) {
  return page.evaluate(
    async ({ v, timeoutMs }) => {
      const $ = (sel) => document.querySelector(sel);
      const sized = (el) => {
        const r = el?.getBoundingClientRect();
        return !!r && r.width > 0 && r.height > 0;
      };
      const loading = () => [...document.querySelectorAll('[role="status"][aria-busy="true"]')].some((e) => e.textContent.trim().length > 0);
      const state = () => (!sized($("aside")) || !sized($("main")) ? "shell-gone" : $(v.ready) ? "content" : loading() ? "loading" : "empty");
      const frame = () => new Promise((resolve) => requestAnimationFrame(resolve));
      const opener = $(v.open);
      if (!opener) return { error: `no ${v.open} in the page` };
      await frame();
      const t0 = performance.now();
      opener.click();
      const frames = {};
      let readyMs = null;
      while (performance.now() - t0 < timeoutMs) {
        await frame();
        const s = state();
        frames[s] = (frames[s] ?? 0) + 1;
        if (s === "content") {
          await frame();
          readyMs = performance.now() - t0;
          break;
        }
      }
      if (v.close) $(v.close)?.click();
      return { readyMs, frames };
    },
    { v, timeoutMs: SMART_VIEW_TIMEOUT_MS },
  );
}

/** Back to the library grid between two views (as a user would come from it). */
async function toLibrary(page) {
  await page.click('[data-nav="library"]');
  await page.waitForSelector('[role="grid"]', { timeout: 10_000 });
}

async function checkSmartViews() {
  const pending = [...SMART_VIEWS];
  const failRest = (why) => pending.splice(0).forEach((v) => report(false, "smart-view-ready", `${v.view} not measured: ${why}`));
  if (!installedExeCheck().present) return failRest("app not installed");
  let app;
  let cdp;
  let sidecar = [];
  try {
    app = await startApp({ exe: INSTALLED_EXE }); // real data dir: no SS_DATA_DIR (README D10: only read)
    cdp = await connectApp(app.cdpUrl);
    await waitForGrid(cdp.page, GRID_READY_MS);
    const gridMs = Date.now() - app.startedAt;
    while (pending.length) {
      const v = pending[0];
      await toLibrary(cdp.page);
      const r = await timeView(cdp.page, v);
      pending.shift();
      if (r.error) {
        report(false, "smart-view-ready", `${v.view} ${r.error}`);
        continue;
      }
      const bad = (r.frames.empty ?? 0) + (r.frames["shell-gone"] ?? 0);
      const ms = r.readyMs === null ? null : Math.round(r.readyMs);
      const frames = Object.entries(r.frames).map(([k, n]) => `${k}=${n}`).join(" ");
      report(
        ms !== null && ms <= SMART_VIEW_READY_MAX_MS && bad === 0,
        "smart-view-ready",
        `${v.view} ${ms === null ? `not ready in ${SMART_VIEW_TIMEOUT_MS}` : ms}ms (budget ${SMART_VIEW_READY_MAX_MS}ms; frames ${frames}; grid shown ${gridMs}ms after spawn)`,
      );
    }
    sidecar = mergeProcs([], app.pid);
  } catch (e) {
    console.error(e.stack ?? e.message);
    failRest(e.message.split(/\r?\n/)[0]);
  } finally {
    await cdp?.close().catch(() => {});
    if (app) {
      const c = await closeApp(app.pid);
      if (!c.graceful) console.error(`smart views: the app did not exit ${c.ms} ms after WM_CLOSE; killed`);
      // The sidecar exits with the app (sidecar-exit); wait for it before the install dir is checked.
      const pids = sidecar.map((p) => p.pid);
      const t0 = Date.now();
      while (Date.now() - t0 < CLOSE_MAX_MS && stillRunning(pids).length) await sleep(250);
      for (const pid of stillRunning(pids)) killTree(pid);
    }
  }
}

/** Every `__pycache__` dir and `.pyc` file under `dir` (paths relative to it). */
function pycacheUnder(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { recursive: true, withFileTypes: true })
    .filter((d) => (d.isDirectory() && d.name === "__pycache__") || (d.isFile() && d.name.endsWith(".pyc")))
    .map((d) => path.relative(dir, path.join(d.parentPath, d.name)).split(path.sep).join("/"));
}

function checkNoPycache(tmp) {
  const inInstall = pycacheUnder(INSTALL_DIR);
  // PYTHONPYCACHEPREFIX mirrors a source's absolute path without its drive: <prefix>\IOT_Tools\apps\...\sidecar\...
  const mirrored = path.join(appDataDir(tmp), PYCACHE_DIR, INSTALL_DIR.replace(/^[A-Za-z]:[\\/]*/, ""), "sidecar");
  const compiled = pycacheUnder(mirrored).filter((f) => f.endsWith(".pyc"));
  report(
    inInstall.length === 0 && compiled.length > 0,
    "no-pycache-in-install-dir",
    `${INSTALL_DIR}: ${inInstall.length ? `${inInstall.length} cache entries (${inInstall.slice(0, 4).join(", ")})` : "no __pycache__, no .pyc"}; ` +
      `bundled sidecar's bytecode in <app data>/${PYCACHE_DIR}: ${compiled.length} .pyc${compiled.length ? "" : " (missing: the redirect is not proven)"}`,
  );
}

// --- main ------------------------------------------------------------------------------------------------

function parseOnly(argv) {
  const i = argv.indexOf("--only");
  if (i < 0) return null;
  const groups = (argv[i + 1] ?? "").split(",").filter(Boolean);
  const bad = groups.filter((g) => !GROUPS.includes(g));
  if (!groups.length || bad.length) throw new Error(`--only takes ${GROUPS.join(",")}; got ${argv[i + 1] ?? "nothing"}`);
  return groups;
}

async function main(argv) {
  let only;
  try {
    only = parseOnly(argv);
  } catch (e) {
    console.error(e.message);
    return 2;
  }
  const want = (g) => !only || only.includes(g);
  const pids = runningPids(APP_EXE_NAME);
  report(pids.length === 0, "app-not-running", pids.length ? `running pid(s) ${pids.join(",")}: close Screenshot Studio first` : "none");
  if (pids.length) return 1;

  const tmp = mkdtempSync(path.join(tmpdir(), "ss-p3-"));
  try {
    if ((want("tests") || want("fixtures")) && !checkBuild()) return 1;
    if (want("tests")) checkLaneTests();
    if (want("fixtures")) checkFixtures(tmp);
    if (want("app")) {
      await checkInstalledApp(tmp);
      await checkSmartViews();
      checkNoPycache(tmp);
    }
  } finally {
    // WebView2 may hold its profile for a moment after the app exits.
    rmSync(tmp, { recursive: true, force: true, maxRetries: 20, retryDelay: 500 });
  }

  const failed = results.filter((ok) => !ok).length;
  if (only) {
    console.log(`FAIL p3 partial run (--only ${only.join(",")}): ${results.length - failed}/${results.length} checks passed`);
    return 1;
  }
  console.log(`${failed === 0 ? "PASS" : "FAIL"} p3 ${results.length - failed}/${results.length} checks passed`);
  return failed === 0 ? 0 : 1;
}

process.exitCode = await main(process.argv.slice(2));
