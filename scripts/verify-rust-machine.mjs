#!/usr/bin/env node
// Read-only verifier for the machine-wide Rust setup. One `PASS|FAIL|WARN <check> <observed>` line per check.
// Exit 0 = every check passed (WARN does not fail), 1 = any FAIL.
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync, mkdtempSync, rmSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { tmpdir, homedir } from 'node:os';
import { fileURLToPath } from 'node:url';

const cfg = JSON.parse(readFileSync(join(dirname(fileURLToPath(import.meta.url)), 'rust-machine.config.json'), 'utf8'));
const P = cfg.paths;
const bin = join(P.CARGO_HOME, 'bin');
const toolEnv = { ...process.env, RUSTUP_HOME: P.RUSTUP_HOME, CARGO_HOME: P.CARGO_HOME, SCCACHE_DIR: P.SCCACHE_DIR, PATH: `${bin};${process.env.PATH}` };
let failed = false;

const norm = (p) => (p || '').replace(/[\\/]+$/, '').toLowerCase();
function report(status, name, observed) {
  if (status === 'FAIL') failed = true;
  console.log(`${status} ${name} ${String(observed).replace(/\s+/g, ' ').trim()}`);
}
function check(name, fn) {
  try { const [ok, obs] = fn(); report(ok ? 'PASS' : 'FAIL', name, obs); }
  catch (e) { report('FAIL', name, e.message.split('\n')[0]); }
}
function run(file, args, opts = {}) {
  return execFileSync(file, args, { encoding: 'utf8', env: toolEnv, stdio: ['ignore', 'pipe', 'pipe'], ...opts }).trim();
}
function regValue(key, name) {
  try {
    const out = execFileSync('reg', ['query', key, '/v', name], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] });
    const m = out.match(new RegExp(`^\\s*${name}\\s+REG_\\w+\\s+(.*)$`, 'mi'));
    return m ? m[1].trim() : null;
  } catch { return null; }
}

// 1. user env vars (registry, so it works in shells started before the change)
for (const name of cfg.userEnv) {
  check(`env:${name}`, () => { const v = regValue('HKCU\\Environment', name); return [norm(v) === norm(P[name]), v ?? '<unset>']; });
}
// 2. user PATH
check('user-path:cargo-bin', () => {
  const v = regValue('HKCU\\Environment', 'Path') ?? '';
  const ok = v.split(';').some((e) => norm(e.replace(/%CARGO_HOME%/i, P.CARGO_HOME)) === norm(bin));
  return [ok, ok ? bin : `${bin} not in user Path`];
});
// 3. toolchain
check('cargo', () => [true, run(join(bin, 'cargo.exe'), ['--version'])]);
check('rustc', () => [true, run(join(bin, 'rustc.exe'), ['--version'])]);
check('rustup-home', () => { const h = run(join(bin, 'rustup.exe'), ['show', 'home']); return [norm(h) === norm(P.RUSTUP_HOME), h]; });
check('host-tuple', () => {
  const v = run(join(bin, 'rustc.exe'), ['-vV']).match(/^host:\s*(\S+)/m)?.[1];
  return [v === cfg.hostTuple, v];
});
// 4. components
check('components', () => {
  const inst = run(join(bin, 'rustup.exe'), ['component', 'list', '--installed']);
  const missing = cfg.components.filter((c) => !inst.split('\n').some((l) => l.startsWith(`${c}-`) || l === c));
  return [missing.length === 0, missing.length ? `missing: ${missing.join(',')}` : cfg.components.join(',')];
});
// 5. cargo tools
check('sccache', () => [true, run(join(bin, 'sccache.exe'), ['--version'])]);
check('tauri-cli', () => [true, run(join(bin, 'cargo.exe'), ['tauri', '--version'])]);
// 6. global cargo config
check('cargo-config:rustc-wrapper', () => {
  const f = join(P.CARGO_HOME, 'config.toml');
  if (!existsSync(f)) return [false, `${f} missing`];
  return [/^\s*rustc-wrapper\s*=\s*"sccache"/m.test(readFileSync(f, 'utf8')), f];
});
// 7. VS Build Tools on D: with an x64 linker
check('vs-buildtools', () => {
  if (!existsSync(cfg.vswhere)) return [false, 'vswhere missing'];
  const paths = run(cfg.vswhere, ['-products', '*', '-requires', cfg.vsBuildTools.requiresComponent, '-property', 'installationPath']).split(/\r?\n/).filter(Boolean);
  const inst = paths.find((p) => norm(p).startsWith(norm(P.vsBuildToolsInstall)));
  if (!inst) return [false, `no VC tools under ${P.vsBuildToolsInstall} (found: ${paths.join(',') || 'none'})`];
  const msvc = join(inst, 'VC', 'Tools', 'MSVC');
  const link = existsSync(msvc) && readdirSync(msvc).map((v) => join(msvc, v, 'bin', 'Hostx64', 'x64', 'link.exe')).find(existsSync);
  return [Boolean(link), link || `link.exe not found under ${msvc}`];
});
// 8. HKLM CachePath
check('vs-cache-path', () => {
  const v = regValue(cfg.vsPolicyKey.replace(/^HKLM:\\/, 'HKLM\\'), 'CachePath');
  return [norm(v) === norm(P.vsPackageCache), v ?? '<unset>'];
});
// 9. smoke build through sccache + MSVC link
check('smoke-build', () => {
  const root = mkdtempSync(join(tmpdir(), `ss-rust-smoke-${process.pid}-`));
  try {
    run(join(bin, 'cargo.exe'), ['new', '--vcs', 'none', '--quiet', 'hello'], { cwd: root });
    const out = run(join(bin, 'cargo.exe'), ['run', '--quiet'], { cwd: join(root, 'hello') });
    return [out === 'Hello, world!', out];
  } finally { rmSync(root, { recursive: true, force: true }); }
});
// 10. nothing on C: profile
for (const d of ['.cargo', '.rustup']) {
  const p = join(homedir(), d);
  report(existsSync(p) ? 'WARN' : 'PASS', `no-profile-dir:${d}`, existsSync(p) ? `${p} exists` : 'absent');
}

process.exit(failed ? 1 : 0);
