#!/usr/bin/env node
// Phase 4.3 Accept (P4 + every success metric of the design, AX-01):
//   node scripts/accept/p4.mjs [--only regress,live,edit,metrics]
//
// Prints one stable line per check on stdout:  PASS|FAIL <check> <value>   (INFO lines are measured-only, never
// counted), then the metrics table (lines starting `|`), then `PASS|FAIL p4 <passed>/<total> checks passed`.
// Exit 0 only when every check passed, 1 otherwise. No prompts. The p1-p3 children's own output goes to stderr,
// each line prefixed `[pN] `. `--only` runs some groups for diagnosis; such a partial run always ends
// `FAIL p4 partial run` and exits 1, so it can never stand in for the Accept.
//
// Prerequisite (human-approved reinstall, plan phase-4.3 step 1; re-run both after every rebuild):
//   cargo tauri build      (RUSTC_WRAPPER="" and CARGO_BUILD_JOBS=2 on this machine, docs/lessons.md)
//   "target\release\bundle\nsis\Screenshot Studio_<version>_x64-setup.exe" /S /D=D:\IOT_Tools\apps\ScreenshotStudio
//
// Checks, in order (plan phase-4.3 step 2):
//   app-not-running   no screenshot-studio.exe is running (single-instance would hijack the launch, README D10)
//   blake3-selftest   lib/blake3.mjs (an independent JS BLAKE3: Node has none) against published vectors
//   build             `cargo build --release -p ss-core --bin ssctl --locked` (target/release/ssctl.exe is what runs)
// group regress (each a child process; README Self-improvement loop "Guard"):
//   p1, p2, p3        node scripts/accept/p<N>.mjs exits 0 and its last line is `PASS p<N> ...`
//   journal-coverage  lane-tests.mjs over crates/ss-core/tests/fileops_test.rs + delete_safety_test.rs (phase 2.1):
//                     every file op is journaled and undoable, permanent delete is journaled and never undoable
// group live (the INSTALLED app on a temp data dir; the fixture root added with `ssctl --data-dir <temp> add-root`):
//   live-index        a PNG written after t and copied into the watched root (fs.copyFile) is listed by
//                     `ssctl --data-dir <temp> list --since <t> --json` within LIVE_MAX_MS (10 s root poll + 2 s
//                     debounce + margin, phase 4.1 watcher)
//   live-grid         ... and the app's grid shows its thumbnail (an <img> for `thumb/<id>/`, loaded) in that time
// group edit (`ssctl`, Caller::Cli, temp data dir + fixture root; blake3 from lib/blake3.mjs):
//   blake3-vs-core    lib/blake3.mjs of the fixture file == the content_hash ss-core stored for it (blake3 crate)
//   edit-copy         `ssctl edit <id> --crop 0,0,200,200 --mode copy --json`: a journaled batch, a new row
//                     `<name> (edited).png`, that file is a 200x200 PNG, the original's blake3 is unchanged
//   edit-overwrite    `--mode overwrite`: a journaled batch, the original is now 200x200 and byte-identical (blake3)
//                     to the copy (same crop, same encoder), a backup under <data dir>\edit-backups
//   edit-undo         `ssctl undo <batchId of the overwrite> --json`: the original's blake3 is the one before the
//                     edits again, and so is ss-core's content_hash after `ssctl index`
// group metrics (design success metrics; real library READ only, README D10):
//   metric-indexed-count  p1 `real-count`: catalog image count of the real root == image files on disk
//   metric-cold-start     p1 `cold-start`: app start -> first grid thumbnail < 2,000 ms
//   metric-query          `ssctl search` on the real data dir, QUERY_RUNS runs per query: every median < 200 ms
//                         (process wall time: start, catalog open, FTS query, JSON out)
//   metric-grid-fps       p1 `fps`: 10 s scroll of the real library, frame time p50 <= 18.18 ms, p95 <= 33 ms
//   metric-journal        journal-coverage above
//   metric-start-menu     p2 `start-menu`: the Start Menu shortcut targets the installed exe (Windows Search proxy)
import { spawn } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import process from "node:process";
import { DatabaseSync } from "node:sqlite";
import { blake3Hex, selfTest } from "./lib/blake3.mjs";
import { connectApp, waitForGrid } from "./lib/cdp.mjs";
import { INSTALLED_EXE, installedExeCheck } from "./lib/install.mjs";
import { png } from "./lib/png.mjs";
import { APP_EXE_NAME, ROOT, childEnv, closeApp, run, runningPids, ssctl, startApp } from "./lib/run.mjs";

const ACCEPT_DIR = path.join(ROOT, "scripts", "accept");
const JOURNAL_TESTS = ["crates/ss-core/tests/fileops_test.rs", "crates/ss-core/tests/delete_safety_test.rs"];
const CATALOG_FILE = "catalog.sqlite"; // ss-core paths::CATALOG_FILE
const EDIT_BACKUPS_DIR = "edit-backups"; // ss-core paths::EDIT_BACKUPS_DIR
const LIVE_MAX_MS = 15_000; // WATCHER_ROOT_POLL_S 10 s + WATCHER_DEBOUNCE_MS 2 s + margin (plan phase-4.3 step 2)
const GRID_READY_MS = 60_000;
const CROP = { x: 0, y: 0, w: 200, h: 200 };
const QUERY_MAX_MS = 200; // design metric: query < 200 ms
const QUERY_RUNS = 5;
const QUERIES = ["chieu", "screenshot", "hoa don"]; // a word 3.4 found 141 times, an English word, an unaccented phrase
const GROUPS = ["regress", "live", "edit", "metrics"];

const results = [];
const childChecks = new Map(); // "p1:real-count" -> { ok, value }
const metrics = []; // { name, value, budget, ok, source }
function report(ok, check, value) {
  results.push(ok);
  console.log(`${ok ? "PASS" : "FAIL"} ${check} ${value}`);
}
const tail = (s, n = 30) => s.trim().split(/\r?\n/).slice(-n).join("\n");
const sleep = (ms) => new Promise((res) => setTimeout(res, ms));
const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];

// --- regress: p1, p2, p3 as children ------------------------------------------------------------------------

/** Runs a script with node, its output streamed to stderr as `[tag] line`; resolves { status, stdout }. */
function runChild(tag, script, args = []) {
  return new Promise((resolve) => {
    // -j 4 ran out of commit memory beside the sidecar (docs/lessons.md phase 4.3).
    const env = childEnv({ RUSTC_WRAPPER: "", CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS || "2" });
    const proc = spawn(process.execPath, [script, ...args], { cwd: ROOT, env, stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "";
    const echo = (stream, keep) => {
      let buf = "";
      stream.setEncoding("utf8");
      stream.on("data", (d) => {
        if (keep) stdout += d;
        buf += d;
        const lines = buf.split(/\r?\n/);
        buf = lines.pop();
        for (const l of lines) process.stderr.write(`[${tag}] ${l}\n`);
      });
      stream.on("end", () => buf && process.stderr.write(`[${tag}] ${buf}\n`));
    };
    echo(proc.stdout, true);
    echo(proc.stderr, false);
    proc.on("error", (e) => resolve({ status: null, stdout, error: e }));
    proc.on("close", (status) => resolve({ status, stdout }));
  });
}

async function checkPhase(n) {
  const t0 = Date.now();
  const r = await runChild(`p${n}`, path.join(ACCEPT_DIR, `p${n}.mjs`));
  const lines = r.stdout.split(/\r?\n/).filter((l) => /^(PASS|FAIL) /.test(l));
  for (const l of lines) {
    const m = /^(PASS|FAIL) (\S+) ?(.*)$/.exec(l);
    childChecks.set(`p${n}:${m[2]}`, { ok: m[1] === "PASS", value: m[3] });
  }
  const last = lines.at(-1) ?? "";
  const ok = r.status === 0 && new RegExp(`^PASS p${n} `).test(last);
  const failed = lines.filter((l) => l.startsWith("FAIL ")).map((l) => l.split(" ")[1]);
  report(ok, `p${n}`, `exit=${r.status ?? r.error?.code} "${last}" ${Math.round((Date.now() - t0) / 1000)}s${failed.length ? ` failed: ${failed.join(",")}` : ""}`);
}

function checkJournalCoverage() {
  const r = run(process.execPath, [path.join(ACCEPT_DIR, "lane-tests.mjs"), ...JOURNAL_TESTS], { env: childEnv({ RUSTC_WRAPPER: "" }) });
  const lines = r.stdout.split(/\r?\n/).filter((l) => /^(PASS|FAIL) /.test(l));
  const passed = lines.filter((l) => l.startsWith("PASS ")).length;
  const ok = !r.error && r.status === 0 && passed === JOURNAL_TESTS.length;
  report(ok, "journal-coverage", `exit=${r.status ?? r.error?.code} ${lines.join(" ; ") || "no result lines"}`);
  if (!ok) console.error(tail(r.out, 40));
  return ok;
}

function checkBuild() {
  // No sccache wrapper (docs/lessons.md phase 1.4 / 2.1).
  const r = run("cargo", ["build", "--release", "-p", "ss-core", "--bin", "ssctl", "--locked"], { env: childEnv({ RUSTC_WRAPPER: "" }) });
  const ok = !r.error && r.status === 0;
  report(ok, "build", ok ? "target/release/ssctl.exe up to date" : `cargo exit ${r.status ?? r.error?.code}`);
  if (!ok) console.error(tail(r.out, 40));
  return ok;
}

// --- fixtures ----------------------------------------------------------------------------------------------

/** A fixture library + its own data dir under `tmp` (README D10): `count` PNGs named `<stem> <i>.png`, indexed. */
function fixtureLibrary(tmp, name, stem, count, size) {
  const root = path.join(tmp, `${name}-root`);
  const dataDir = path.join(tmp, `${name}-data`);
  mkdirSync(root, { recursive: true });
  mkdirSync(dataDir, { recursive: true });
  for (let i = 1; i <= count; i++) writeFileSync(path.join(root, `${stem} ${i}.png`), png(size[0], size[1], 40 + i));
  const add = ssctl(["add-root", root], { dataDir });
  const idx = add.status === 0 ? ssctl(["index"], { dataDir }) : add;
  const c = idx.json ?? {};
  const ok = idx.status === 0 && c.total === count && c.thumbOk === count;
  if (!ok) throw new Error(`fixture ${name}: add-root/index exit ${idx.status} total=${c.total} thumbOk=${c.thumbOk} expected ${count}: ${tail(idx.out, 5)}`);
  return { root, dataDir };
}

/** Width and height from a PNG's IHDR. */
function pngSize(file) {
  const b = readFileSync(file);
  if (b.length < 24 || b.readUInt32BE(12) !== 0x49484452) return null; // "IHDR"
  return { w: b.readUInt32BE(16), h: b.readUInt32BE(20) };
}
const hashOf = (file) => blake3Hex(readFileSync(file));
const short = (h) => (h ? h.slice(0, 12) : String(h));

/** ss-core's stored blake3 for image `id` (the temp catalog only; README D10). */
function coreHash(dataDir, id) {
  const db = new DatabaseSync(path.join(dataDir, CATALOG_FILE));
  try {
    return db.prepare("SELECT content_hash AS h FROM image WHERE id = ?").get(id)?.h ?? null;
  } finally {
    db.close();
  }
}

// --- live: watcher -> catalog -> grid on the installed app ------------------------------------------------------

async function checkLive(tmp) {
  const inst = installedExeCheck();
  if (!inst.present) {
    for (const c of ["live-index", "live-grid"]) report(false, c, `not run: ${inst.value}`);
    return;
  }
  if (!inst.ok) console.log(`INFO live-installed-exe ${inst.value}`);
  let lib;
  try {
    lib = fixtureLibrary(tmp, "live", "p4 live fixture", 3, [320, 200]);
  } catch (e) {
    for (const c of ["live-index", "live-grid"]) report(false, c, e.message);
    return;
  }
  const src = path.join(tmp, "live-src");
  mkdirSync(src, { recursive: true });
  const name = "p4-live-copy.png"; // no date in the name: taken_ms comes from the file times, which are >= t
  let app;
  let cdp;
  let found = null;
  let gridMs = null;
  let t = null;
  try {
    app = await startApp({ exe: INSTALLED_EXE, dataDir: lib.dataDir });
    cdp = await connectApp(app.cdpUrl);
    await waitForGrid(cdp.page, GRID_READY_MS);
    t = Date.now();
    writeFileSync(path.join(src, name), png(480, 300, 99)); // written after t, so its mtime is too
    copyFileSync(path.join(src, name), path.join(lib.root, name));
    while (Date.now() - t < LIVE_MAX_MS && !found) {
      const r = ssctl(["list", "--since", String(t), "--limit", "50"], { dataDir: lib.dataDir });
      const hit = (r.json?.items ?? []).find((i) => i.fileName === name);
      if (hit) found = { id: hit.id, ms: Date.now() - t };
      else await sleep(250);
    }
    if (found) {
      const left = Math.max(0, LIVE_MAX_MS - (Date.now() - t));
      try {
        await cdp.page.waitForFunction(
          (id) =>
            [...document.querySelectorAll('[role="grid"] img')].some((i) => decodeURIComponent(i.currentSrc || i.src).includes(`thumb/${id}/`) && i.complete && i.naturalWidth > 0),
          found.id,
          { timeout: left + 1, polling: 100 },
        );
        gridMs = Date.now() - t;
      } catch {
        // reported below
      }
    }
  } catch (e) {
    console.error(e.stack ?? e.message);
  } finally {
    await cdp?.close().catch(() => {});
    if (app) {
      const c = await closeApp(app.pid);
      if (!c.graceful) console.error(`live: the app did not exit ${c.ms} ms after WM_CLOSE; killed`);
    }
  }
  report(!!found && found.ms <= LIVE_MAX_MS, "live-index", found ? `${name} listed as #${found.id} ${found.ms}ms after the copy (max ${LIVE_MAX_MS}ms)` : `${name} not listed by \`list --since ${t}\` within ${LIVE_MAX_MS}ms`);
  report(gridMs !== null, "live-grid", gridMs !== null ? `thumbnail of #${found.id} loaded in the grid ${gridMs}ms after the copy` : `no loaded grid thumbnail for ${found ? `#${found.id}` : name} within ${LIVE_MAX_MS}ms`);
}

// --- edit round-trip through ssctl ----------------------------------------------------------------------------

function checkEdit(tmp) {
  const names = ["blake3-vs-core", "edit-copy", "edit-overwrite", "edit-undo"];
  let lib;
  let item;
  try {
    lib = fixtureLibrary(tmp, "edit", "p4 edit source", 1, [640, 400]);
    item = ssctl(["list", "--limit", "10"], { dataDir: lib.dataDir }).json?.items?.[0];
    if (!item) throw new Error("ssctl list returned no image");
  } catch (e) {
    for (const c of names) report(false, c, e.message);
    return;
  }
  const file = path.join(lib.root, item.fileName);
  const h0 = hashOf(file);
  const core0 = coreHash(lib.dataDir, item.id);
  report(core0 === h0, "blake3-vs-core", `${item.fileName} (${readFileSync(file).length} bytes): js ${short(h0)} core ${short(core0)}`);

  const crop = `${CROP.x},${CROP.y},${CROP.w},${CROP.h}`;
  const copy = ssctl(["edit", String(item.id), "--crop", crop, "--mode", "copy"], { dataDir: lib.dataDir });
  const copyName = item.fileName.replace(/\.png$/i, " (edited).png");
  const copyFile = path.join(lib.root, copyName);
  const copySize = existsSync(copyFile) ? pngSize(copyFile) : null;
  const hc = existsSync(copyFile) ? hashOf(copyFile) : null;
  const copyOk =
    copy.status === 0 &&
    typeof copy.json?.batchId === "number" &&
    copy.json?.image?.fileName === copyName &&
    copy.json.image.id !== item.id &&
    copySize?.w === CROP.w &&
    copySize?.h === CROP.h &&
    hashOf(file) === h0;
  report(copyOk, "edit-copy", `exit=${copy.status} batchId=${copy.json?.batchId} row #${copy.json?.image?.id} ${copy.json?.image?.fileName} ${copySize ? `${copySize.w}x${copySize.h}` : "no file"} blake3 ${short(hc)}; original blake3 ${hashOf(file) === h0 ? "unchanged" : "CHANGED"}`);
  if (copy.status !== 0) console.error(tail(copy.out, 10));

  const over = ssctl(["edit", String(item.id), "--crop", crop, "--mode", "overwrite"], { dataDir: lib.dataDir });
  const h1 = hashOf(file);
  const overSize = pngSize(file);
  const backupDir = path.join(lib.dataDir, EDIT_BACKUPS_DIR);
  const backups = existsSync(backupDir) ? readdirSync(backupDir, { recursive: true, withFileTypes: true }).filter((d) => d.isFile()).length : 0;
  const batch = over.json?.batchId;
  const overOk = over.status === 0 && typeof batch === "number" && h1 !== h0 && h1 === hc && overSize?.w === CROP.w && overSize?.h === CROP.h && backups > 0;
  report(overOk, "edit-overwrite", `exit=${over.status} batchId=${batch} original now ${overSize ? `${overSize.w}x${overSize.h}` : "?"} blake3 ${short(h1)} (${h1 === hc ? "= the copy" : "!= the copy"}, ${h1 !== h0 ? "!= before" : "= before"}); backups=${backups}`);
  if (over.status !== 0) console.error(tail(over.out, 10));

  const undo = typeof batch === "number" ? ssctl(["undo", String(batch)], { dataDir: lib.dataDir }) : { status: null, out: "no batch id", json: null };
  const h2 = hashOf(file);
  const reindex = ssctl(["index"], { dataDir: lib.dataDir });
  const core2 = coreHash(lib.dataDir, item.id);
  report(
    undo.status === 0 && h2 === h0 && core2 === h0 && reindex.status === 0,
    "edit-undo",
    `undo ${batch} exit=${undo.status} file blake3 ${short(h2)} (${h2 === h0 ? "= before the edits" : "!= before"}); after index core ${short(core2)} (${core2 === h0 ? "=" : "!="})`,
  );
  if (undo.status !== 0) console.error(tail(undo.out, 10));
}

// --- metrics ---------------------------------------------------------------------------------------------------

/** Times `ssctl search <q>` on the real data dir (read-only). */
function timeQueries() {
  const per = [];
  for (const q of QUERIES) {
    const ms = [];
    let hits = null;
    let status = 0;
    for (let i = 0; i < QUERY_RUNS; i++) {
      const t0 = performance.now();
      const r = ssctl(["search", q, "--limit", "100"]);
      ms.push(performance.now() - t0);
      status ||= r.status;
      hits = r.json?.items?.length ?? hits;
    }
    per.push({ q, median: Math.round(median(ms)), all: ms.map(Math.round), hits, status });
  }
  return per;
}

function metric(name, ok, value, budget, source) {
  metrics.push({ name, ok, value, budget, source });
  report(ok, `metric-${name}`, `${value} (budget ${budget}; ${source})`);
}

function fromChild(key) {
  const c = childChecks.get(key);
  return c ?? { ok: false, value: `not reported (${key.split(":")[0]} did not run or did not print it)` };
}

function checkMetrics(journalOk) {
  const count = fromChild("p1:real-count");
  metric("indexed-count", count.ok, count.value, "catalog == files on disk", "p1 real-count");
  const cold = fromChild("p1:cold-start");
  metric("cold-start", cold.ok, cold.value, "< 2,000 ms", "p1 cold-start");
  const q = timeQueries();
  const worst = Math.max(...q.map((x) => x.median));
  const qOk = q.every((x) => x.status === 0 && x.median < QUERY_MAX_MS);
  metric("query", qOk, `max median ${worst}ms; ${q.map((x) => `"${x.q}" ${x.median}ms [${x.all.join(",")}] hits=${x.hits} exit=${x.status}`).join("; ")}`, `< ${QUERY_MAX_MS} ms per query`, `ssctl search x${QUERY_RUNS}, real data dir`);
  const fps = fromChild("p1:fps");
  metric("grid-fps", fps.ok, fps.value, "p50 <= 18.18 ms (55 fps), p95 <= 33 ms", "p1 fps");
  metric("journal", journalOk ?? false, journalOk === null ? "not run" : journalOk ? "every file op journaled + undoable; permanent delete journaled, never undoable" : "see journal-coverage", "fileops_test + delete_safety_test green", "journal-coverage");
  const lnk = fromChild("p2:start-menu");
  metric("start-menu", lnk.ok, lnk.value, "shortcut targets the installed exe", "p2 start-menu");
}

function printMetricsTable() {
  const cell = (s) => String(s).replace(/\|/g, "/");
  console.log("| metric | result | value | budget | source |");
  console.log("|---|---|---|---|---|");
  for (const m of metrics) console.log(`| ${m.name} | ${m.ok ? "PASS" : "FAIL"} | ${cell(m.value)} | ${cell(m.budget)} | ${cell(m.source)} |`);
}

// --- main ------------------------------------------------------------------------------------------------------

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
  const st = selfTest();
  report(st.ok, "blake3-selftest", st.value);
  if (!checkBuild()) return 1;

  let journalOk = null;
  if (want("regress")) {
    for (const n of [1, 2, 3]) await checkPhase(n);
    journalOk = checkJournalCoverage();
  }
  const tmp = mkdtempSync(path.join(tmpdir(), "ss-p4-"));
  try {
    if (want("live")) await checkLive(tmp);
    if (want("edit")) checkEdit(tmp);
  } finally {
    rmSync(tmp, { recursive: true, force: true, maxRetries: 20, retryDelay: 500 }); // WebView2 may hold its profile briefly
  }
  if (want("metrics")) checkMetrics(journalOk);
  if (metrics.length) printMetricsTable();

  const failed = results.filter((ok) => !ok).length;
  if (only) {
    console.log(`FAIL p4 partial run (--only ${only.join(",")}): ${results.length - failed}/${results.length} checks passed`);
    return 1;
  }
  console.log(`${failed === 0 ? "PASS" : "FAIL"} p4 ${results.length - failed}/${results.length} checks passed`);
  return failed === 0 ? 0 : 1;
}

process.exitCode = await main(process.argv.slice(2));
