#!/usr/bin/env node
// Phase 1.4 Accept (P1 success checks, AX-01):  node scripts/accept/p1.mjs
//
// Prints one stable line per check on stdout:  PASS|FAIL <check> <value>
// plus measured-only lines (no budget agreed yet, never counted):  INFO <metric> <value>
// Exit 0 only when every check passed, 1 otherwise. Starts and stops the app itself. Diagnostics on stderr.
//
// Checks, in order (plan phase-1.4 step 4):
//   app-not-running  no screenshot-studio.exe is running (single-instance would hijack the run, README D10)
//   build            production UI bundle + release ssctl + release app with the UI embedded (lib/run.mjs);
//                    the first build takes ~25 min on this machine, later ones are incremental
//   lane-tests       scripts/accept/lane-tests.mjs over every Rust and UI test file (phases 1.1-1.3)
//   fixture-count    2,000 generated PNGs in %TEMP%, own data dir: `ssctl stats` total == 2,000
//   fixture-thumbs   ... and thumbOk == 2,000
//   real-count       real catalog (READ only) image count of the real root == image files on disk
//                    (recursive directory listing, metadata only, extensions from `ssctl extensions`)
//   cold-start       app start -> first grid thumbnail paint, real library, < 2,000 ms
//   fps              grid scrolled top to bottom over 10 s: frame time p50 <= 18.18 ms (>= 55 fps), p95 <= 33 ms
//   INFO memory-idle app + WebView2 process tree once the first screen is shown: process count, working set, private
//   INFO memory      the same after the scroll
//   INFO disk        real data dir: catalog and thumbnail bytes; release exe sizes
//
// Needs cargo on PATH and `npm ci` done. The real library is never written: the real data dir is only opened
// by `ssctl stats` and by the app, which indexes nothing unless a user asks it to.
import { mkdirSync, mkdtempSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import process from "node:process";
import { crc32, deflateSync } from "node:zlib";
import { connectApp, firstGridPaint, percentile, scrollFrames, waitForGrid } from "./lib/cdp.mjs";
import { APP_EXE, APP_EXE_NAME, ROOT, SSCTL_EXE, buildRelease, run, runningPids, ssctl, startApp, treeMemory } from "./lib/run.mjs";

const REAL_ROOT = "C:\\Users\\Nghaiz\\OneDrive\\Pictures\\Screenshots"; // design-intent.md default library root
const REAL_DATA_DIR = "D:\\IOT_Tools\\data\\ScreenshotStudio"; // ss-core DEFAULT_DATA_DIR (ssctl without --data-dir)
const FIXTURE_COUNT = 2000;
const FPS_SCROLL_MS = 10_000;
const FPS_P50_MAX_MS = 18.18; // 55 fps
const FPS_P95_MAX_MS = 33;
const COLD_START_MAX_MS = 2000;

const results = [];
function report(ok, check, value) {
  results.push(ok);
  console.log(`${ok ? "PASS" : "FAIL"} ${check} ${value}`);
}
const tail = (s, n = 30) => s.trim().split(/\r?\n/).slice(-n).join("\n");

// --- fixtures ------------------------------------------------------------------------------------

function png(width, height, seed) {
  const raw = Buffer.alloc((width * 3 + 1) * height);
  for (let y = 0; y < height; y++) {
    const row = y * (width * 3 + 1);
    for (let x = 0; x < width; x++) {
      const o = row + 1 + x * 3;
      raw[o] = (x * 3 + seed * 7) & 255;
      raw[o + 1] = (y * 5 + seed * 13) & 255;
      raw[o + 2] = ((x ^ y) + seed * 29) & 255;
    }
  }
  const chunk = (type, data) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(body));
    return Buffer.concat([len, body, crc]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 2; // RGB
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

/** Writes `count` screenshot-named PNGs, a quarter of them in a subfolder. */
function writeFixtures(dir, count) {
  mkdirSync(path.join(dir, "sub"), { recursive: true });
  const base = Date.UTC(2026, 0, 1);
  for (let i = 0; i < count; i++) {
    const d = new Date(base + i * 7 * 60_000);
    const stamp = d.toISOString().replace(/T/, " ").replace(/:/g, "").slice(0, 17);
    const name = `Screenshot ${stamp}_${i}.png`;
    writeFileSync(path.join(dir, i % 4 === 0 ? "sub" : "", name), png(96 + (i % 5) * 16, 64 + (i % 3) * 16, i));
  }
}

function checkFixtures() {
  const tmp = mkdtempSync(path.join(tmpdir(), "ss-p1-"));
  try {
    const lib = path.join(tmp, "library");
    const data = path.join(tmp, "data");
    writeFixtures(lib, FIXTURE_COUNT);
    const add = ssctl(["add-root", lib], { dataDir: data });
    if (add.status !== 0) throw new Error(`add-root exit ${add.status}: ${tail(add.out)}`);
    const idx = ssctl(["index", "--wait"], { dataDir: data });
    if (idx.status !== 0) throw new Error(`index exit ${idx.status}: ${tail(idx.out)}`);
    const stats = ssctl(["stats"], { dataDir: data });
    if (stats.status !== 0 || !stats.json) throw new Error(`stats exit ${stats.status}: ${tail(stats.out)}`);
    const { total, thumbOk } = stats.json;
    report(total === FIXTURE_COUNT, "fixture-count", `catalog=${total} expected=${FIXTURE_COUNT}`);
    report(thumbOk === FIXTURE_COUNT, "fixture-thumbs", `thumbOk=${thumbOk} expected=${FIXTURE_COUNT}`);
  } catch (e) {
    report(false, "fixture-count", e.message.split("\n")[0]);
    report(false, "fixture-thumbs", "not measured");
    console.error(e.message);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
}

// --- real library (read only) ----------------------------------------------------------------------

function countImagesOnDisk(root, extensions) {
  const exts = new Set(extensions.map((e) => e.toLowerCase()));
  return readdirSync(root, { recursive: true, withFileTypes: true }).filter(
    (d) => d.isFile() && exts.has(path.extname(d.name).slice(1).toLowerCase()),
  ).length;
}

function checkRealLibrary() {
  const ext = ssctl(["extensions"]);
  const stats = ssctl(["stats"]);
  if (ext.status !== 0 || !Array.isArray(ext.json) || stats.status !== 0 || !stats.json) {
    report(false, "real-count", `ssctl failed (extensions exit ${ext.status}, stats exit ${stats.status})`);
    console.error(tail(`${ext.out}\n${stats.out}`));
    return false;
  }
  const root = stats.json.roots.find((r) => r.path.toLowerCase() === REAL_ROOT.toLowerCase());
  if (!root) {
    report(false, "real-count", `root not registered: ${REAL_ROOT} (add it in the app, then index)`);
    return false;
  }
  let disk;
  try {
    disk = countImagesOnDisk(REAL_ROOT, ext.json);
  } catch (e) {
    report(false, "real-count", `cannot list ${REAL_ROOT}: ${e.code ?? e.message}`);
    return false;
  }
  const ok = disk > 0 && root.imageCount === disk;
  report(ok, "real-count", `catalog=${root.imageCount} disk=${disk} cloudOnly=${stats.json.cloudOnly} thumbOk=${stats.json.thumbOk}`);
  if (!ok && disk > 0) console.error(`real-count: catalog differs from disk; re-index with \`ssctl index ${root.id} --wait\``);
  return ok;
}

// --- app: cold start + fps -------------------------------------------------------------------------

async function checkApp() {
  let app;
  let cdp;
  let coldDone = false;
  try {
    app = await startApp(); // real data dir: no SS_DATA_DIR
    cdp = await connectApp(app.cdpUrl);
    const items = await waitForGrid(cdp.page);
    const paint = await firstGridPaint(cdp.page);
    coldDone = true;
    if (paint.epochMs === null) report(false, "cold-start", "no grid paint timing found");
    else {
      const ms = Math.round(paint.epochMs - app.startedAt);
      report(ms < COLD_START_MAX_MS, "cold-start", `${ms}ms (max ${COLD_START_MAX_MS}ms, ${paint.method}, items=${items})`);
    }
    await cdp.page.waitForTimeout(1000); // let the first page of thumbnails settle before scrolling
    const idle = treeMemory(app.pid);
    console.log(`INFO memory-idle processes=${idle.processes} workingSet=${idle.workingSetMB}MB private=${idle.privateMB}MB (app + WebView2, first screen shown)`);
    const { frames, scrolledPx, maxPx } = await scrollFrames(cdp.page, FPS_SCROLL_MS);
    const p50 = percentile(frames, 50);
    const p95 = percentile(frames, 95);
    const ok = frames.length > 0 && scrolledPx >= maxPx - 1 && p50 <= FPS_P50_MAX_MS && p95 <= FPS_P95_MAX_MS;
    report(
      ok,
      "fps",
      `p50=${p50.toFixed(2)}ms (${(1000 / p50).toFixed(1)}fps) p95=${p95.toFixed(2)}ms frames=${frames.length} items=${items} scrolled=${Math.round(scrolledPx)}/${Math.round(maxPx)}px`,
    );
    const mem = treeMemory(app.pid);
    console.log(`INFO memory processes=${mem.processes} workingSet=${mem.workingSetMB}MB private=${mem.privateMB}MB (app + WebView2, after scrolling ${items} items)`);
  } catch (e) {
    if (!coldDone) report(false, "cold-start", e.message.split("\n")[0]);
    report(false, "fps", "not measured");
    console.error(e.stack ?? e.message);
  } finally {
    await cdp?.close().catch(() => {});
    app?.stop();
  }
}

// --- footprint on disk (measured only) ---------------------------------------------------------------

function dirBytes(dir) {
  try {
    return readdirSync(dir, { recursive: true, withFileTypes: true })
      .filter((d) => d.isFile())
      .reduce((a, d) => a + statSync(path.join(d.parentPath, d.name)).size, 0);
  } catch (e) {
    if (e.code === "ENOENT") return 0;
    throw e;
  }
}

function reportDisk() {
  const mb = (b) => `${(b / 1048576).toFixed(1)}MB`;
  const data = REAL_DATA_DIR;
  const catalog = readdirSync(data).filter((f) => f.startsWith("catalog.sqlite")).reduce((a, f) => a + statSync(path.join(data, f)).size, 0);
  const thumbs = dirBytes(path.join(data, "thumbs"));
  console.log(`INFO disk catalog=${mb(catalog)} thumbs=${mb(thumbs)} app.exe=${mb(statSync(APP_EXE).size)} ssctl.exe=${mb(statSync(SSCTL_EXE).size)}`);
}

// --- main ------------------------------------------------------------------------------------------

async function main() {
  const pids = runningPids(APP_EXE_NAME);
  report(pids.length === 0, "app-not-running", pids.length ? `running pid(s) ${pids.join(",")}: close Screenshot Studio first` : "none");
  if (pids.length) return 1;

  const build = buildRelease();
  const built = !build.error && build.status === 0;
  report(built, "build", built ? "vite + release ssctl + release app" : `${build.name} exit ${build.error ? build.error.code : build.status}`);
  if (!built) {
    const causes = build.out.split(/\r?\n/).filter((l) => /^error|memory allocation|panicked|STATUS_/.test(l));
    console.error(`${causes.slice(0, 20).join("\n")}\n--- last lines ---\n${tail(build.out, 15)}`);
    return 1;
  }

  const testFiles = [
    ...readdirSync(path.join(ROOT, "crates"), { recursive: true })
      .map((f) => `crates/${f.split(path.sep).join("/")}`)
      .filter((f) => /^crates\/[^/]+\/tests\/[^/]+\.rs$/.test(f)),
    ...readdirSync(path.join(ROOT, "src"), { recursive: true })
      .map((f) => `src/${f.split(path.sep).join("/")}`)
      .filter((f) => /\.test\.tsx?$/.test(f)),
  ].sort();
  const lane = run(process.execPath, [path.join(ROOT, "scripts", "accept", "lane-tests.mjs"), ...testFiles]);
  const fileLines = lane.stdout.trim().split(/\r?\n/).filter(Boolean);
  console.error(lane.stdout.trim());
  report(lane.status === 0, "lane-tests", `${fileLines.filter((l) => l.startsWith("PASS")).length}/${testFiles.length} files (${testFiles.join(" ")})`);
  if (lane.status !== 0) console.error(tail(lane.out));

  checkFixtures();
  checkRealLibrary();
  await checkApp();
  reportDisk();

  const failed = results.filter((ok) => !ok).length;
  console.log(`${failed === 0 ? "PASS" : "FAIL"} p1 ${results.length - failed}/${results.length} checks passed`);
  return failed === 0 ? 0 : 1;
}

process.exitCode = await main();
