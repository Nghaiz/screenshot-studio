#!/usr/bin/env node
// Lane Accept runner (plan README D11, AX-01).
//
//   node scripts/accept/lane-tests.mjs <test file> [<test file> ...]
//
// Runs exactly the named test files and prints one stable line per file on stdout:
//   PASS|FAIL <file> <counts>
// Exit 0 only when every file ran >= 1 test with none failed/skipped/ignored/pending; 1 otherwise; 2 on usage.
// A missing file is a FAIL, never a skip. Runner output of a FAIL goes to stderr for diagnosis.
//
// File kinds:
//   crates/<crate>/tests/<name>.rs  -> cargo test -p <crate> --test <name>     (crate dir name = package name)
//   src/**/*.test.ts(x)             -> one vitest run over all of them, --reporter=json
//   sidecar/tests/*.py              -> <sidecar-venv>\Scripts\python.exe -m pytest <file> -rA
//     venv: env SS_SIDECAR_VENV, else the `sidecarVenv` setting default (contract-tauri-ipc.md § Setting keys).
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const DEFAULT_SIDECAR_VENV = "D:\\IOT_Tools\\data\\ScreenshotStudio\\sidecar-venv";
const USAGE = "usage: node scripts/accept/lane-tests.mjs <crates/<crate>/tests/<name>.rs | src/**/*.test.ts(x) | sidecar/tests/*.py> ...";

// Vitest loads itself twice (and finds no suite) when cwd has a lowercase drive letter (d:\...), which is how
// VS Code starts terminals. Every runner gets an upper-case drive cwd. See docs/lessons.md.
const upperDrive = (p) => p.replace(/^[a-z]:/, (d) => d.toUpperCase());
const ROOT = upperDrive(path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", ".."));

function classify(rel) {
  let m = rel.match(/^crates\/([^/]+)\/tests\/([^/]+)\.rs$/);
  if (m) return { kind: "cargo", crate: m[1], name: m[2] };
  if (/^src\/.+\.test\.tsx?$/.test(rel)) return { kind: "vitest" };
  if (/^sidecar\/tests\/[^/]+\.py$/.test(rel)) return { kind: "pytest" };
  return null;
}

function run(cmd, args, opts = {}) {
  const r = spawnSync(cmd, args, { cwd: ROOT, encoding: "utf8", maxBuffer: 64 * 1024 * 1024, ...opts });
  const out = `${r.stdout ?? ""}${r.stderr ?? ""}`;
  return { status: r.status, error: r.error, out };
}

const tail = (s, n = 40) => s.split(/\r?\n/).slice(-n).join("\n");

function cargoFile(file, { crate, name }) {
  const r = run("cargo", ["test", "-p", crate, "--test", name]);
  if (r.error) return { ok: false, counts: `cargo not runnable: ${r.error.code ?? r.error.message}`, out: "" };
  const lines = [...r.out.matchAll(/test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];
  const sum = (i) => lines.reduce((a, l) => a + Number(l[i]), 0);
  const [passed, failed, ignored] = [sum(2), sum(3), sum(4)];
  const counts = lines.length ? `passed=${passed} failed=${failed} ignored=${ignored}` : "no test result (build failed?)";
  const ok = r.status === 0 && lines.length > 0 && lines.every((l) => l[1] === "ok") && passed > 0 && failed === 0 && ignored === 0;
  return { ok, counts, out: r.out };
}

function vitestFiles(files) {
  const results = new Map();
  const dir = mkdtempSync(path.join(tmpdir(), "lane-tests-"));
  const report = path.join(dir, "vitest.json");
  try {
    const entry = path.join(ROOT, "node_modules", "vitest", "vitest.mjs");
    if (!existsSync(entry)) {
      for (const f of files) results.set(f, { ok: false, counts: "vitest not installed (npm install)", out: "" });
      return results;
    }
    const r = run(process.execPath, [entry, "run", ...files, "--reporter=json", `--outputFile=${report}`]);
    if (!existsSync(report)) {
      for (const f of files) results.set(f, { ok: false, counts: `vitest wrote no report (exit ${r.status})`, out: r.out });
      return results;
    }
    const json = JSON.parse(readFileSync(report, "utf8"));
    const key = (p) => upperDrive(path.resolve(ROOT, p)).toLowerCase();
    const byFile = new Map(json.testResults.map((t) => [key(t.name), t]));
    for (const f of files) {
      const t = byFile.get(key(f));
      if (!t) {
        results.set(f, { ok: false, counts: "not run by vitest", out: r.out });
        continue;
      }
      const st = (s) => t.assertionResults.filter((a) => a.status === s).length;
      const [passed, failed] = [st("passed"), st("failed")];
      const other = t.assertionResults.length - passed - failed; // pending / skipped / todo / disabled
      const counts = `passed=${passed} failed=${failed} skipped=${other}${t.status === "failed" && !t.assertionResults.length ? " (suite error)" : ""}`;
      const ok = t.status === "passed" && passed > 0 && failed === 0 && other === 0;
      results.set(f, { ok, counts, out: ok ? "" : `${t.message ?? ""}\n${r.out}` });
    }
    if (json.numPendingTests !== 0 || json.numTodoTests !== 0) {
      for (const [f, v] of results) if (v.ok) results.set(f, { ...v, ok: false, counts: `${v.counts} (run had pending/todo tests)` });
    }
    return results;
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

function pytestFile(file) {
  const venv = process.env.SS_SIDECAR_VENV || DEFAULT_SIDECAR_VENV;
  const python = path.join(venv, "Scripts", "python.exe");
  if (!existsSync(python)) return { ok: false, counts: `sidecar venv python not found: ${python}`, out: "" };
  const r = run(python, ["-m", "pytest", file, "-rA", "-p", "no:cacheprovider"]);
  const summary = r.out.split(/\r?\n/).filter((l) => /^=+ .*\bin [\d.]+s/.test(l)).pop() ?? "";
  const n = (word) => Number((summary.match(new RegExp(`(\\d+) ${word}`)) ?? [, 0])[1]);
  const c = { passed: n("passed"), failed: n("failed"), errors: n("errors?"), skipped: n("skipped"), deselected: n("deselected"), xfailed: n("xfailed"), xpassed: n("xpassed") };
  const counts = Object.entries(c).filter(([k, v]) => v || k === "passed").map(([k, v]) => `${k}=${v}`).join(" ");
  const bad = c.failed + c.errors + c.skipped + c.deselected + c.xfailed + c.xpassed;
  return { ok: r.status === 0 && c.passed > 0 && bad === 0, counts: summary ? counts : "no pytest summary", out: r.out };
}

function main(argv) {
  if (argv.length === 0 || argv.includes("-h") || argv.includes("--help")) {
    console.error(USAGE);
    return 2;
  }
  const items = argv.map((arg) => {
    const rel = path.relative(ROOT, path.resolve(ROOT, arg)).split(path.sep).join("/");
    return { arg, rel, spec: classify(rel) };
  });
  const unknown = items.filter((i) => !i.spec);
  if (unknown.length) {
    console.error(`unsupported test file kind: ${unknown.map((i) => i.arg).join(", ")}\n${USAGE}`);
    return 2;
  }

  const results = new Map();
  for (const i of items) if (!existsSync(path.join(ROOT, i.rel))) results.set(i.rel, { ok: false, counts: "file not found", out: "" });
  const present = items.filter((i) => !results.has(i.rel));

  for (const i of present.filter((i) => i.spec.kind === "cargo")) results.set(i.rel, cargoFile(i.rel, i.spec));
  const vitest = present.filter((i) => i.spec.kind === "vitest").map((i) => i.rel);
  if (vitest.length) for (const [f, v] of vitestFiles(vitest)) results.set(f, v);
  for (const i of present.filter((i) => i.spec.kind === "pytest")) results.set(i.rel, pytestFile(i.rel));

  let allOk = true;
  for (const i of items) {
    const r = results.get(i.rel);
    console.log(`${r.ok ? "PASS" : "FAIL"} ${i.rel} ${r.counts}`);
    if (!r.ok) {
      allOk = false;
      if (r.out) console.error(`--- ${i.rel} (last lines) ---\n${tail(r.out)}`);
    }
  }
  return allOk ? 0 : 1;
}

process.exitCode = main(process.argv.slice(2));
