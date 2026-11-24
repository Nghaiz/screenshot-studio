// The installed app (phase 2.3, NSIS currentUser into D:): where it lives, whether it is the current build, and a
// PowerShell runner for the Win32 queries the Accept scripts make. Shared by p2.mjs and p3.mjs.
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { APP_EXE, APP_EXE_NAME, childEnv, run } from "./run.mjs";

export const INSTALL_DIR = "D:\\IOT_Tools\\apps\\ScreenshotStudio"; // design-intent.md § Names and paths
export const INSTALLED_EXE = path.join(INSTALL_DIR, APP_EXE_NAME);

const tail = (s, n) => s.trim().split(/\r?\n/).slice(-n).join("\n");

/** Runs a PowerShell script in a child process (inputs through env vars, never spliced into the script). */
export function powershell(script, env = {}) {
  const r = run("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", script], { env: childEnv(env) });
  if (r.error || r.status !== 0) throw new Error(`powershell failed (${r.error?.message ?? `exit ${r.status}`}): ${tail(r.out, 5)}`);
  return r.stdout.trim();
}

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

/**
 * The `installed-exe` check: <install dir>\screenshot-studio.exe exists and is target\release's build, so a rebuild
 * without a re-install fails here, not later. Returns { ok, present, value }; `present` = the launch checks can run
 * (an installed exe and a build to compare it with; a stale install still launches, and fails this check).
 */
export function installedExeCheck() {
  if (!existsSync(INSTALLED_EXE)) {
    return { ok: false, present: false, value: `${INSTALLED_EXE} missing: build with \`cargo tauri build\`, then run the installer with /S /D=${INSTALL_DIR}` };
  }
  if (!existsSync(APP_EXE)) {
    return { ok: false, present: false, value: `${APP_EXE} missing: cannot tell whether the installed exe is the current build` };
  }
  const { same, why } = sameBuild(INSTALLED_EXE, APP_EXE);
  return { ok: same, present: true, value: same ? `${INSTALLED_EXE} = ${APP_EXE}, ${why}` : `${INSTALLED_EXE} is not ${APP_EXE} (${why}): re-run the installer` };
}
