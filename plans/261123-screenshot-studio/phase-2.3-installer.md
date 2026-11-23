# Phase 2.3: P2 integration, installer, Start Menu, Windows Search (serial, human-gated)

Lane: serial
Depends-on: 2.1, 2.2
Gate: human (approve installing Screenshot Studio into D:\IOT_Tools\apps\ScreenshotStudio)
Effort: M
Creates: `scripts/accept/p2.mjs`
Owns: `src-tauri/**` (serial integration phase; lanes are merged and idle)
AX: AX-01 `p2.mjs` prints `PASS|FAIL <check> <value>` lines, exit 0 only when all pass; no prompts
Accept: `node scripts/accept/p2.mjs`

## Goal

Merge the P2 lanes, make the app installable from an NSIS installer into `D:\IOT_Tools\apps\ScreenshotStudio` with a
Start Menu entry so "Screenshot Studio" is found in Windows Search and launches. App is usable from here.

## Steps

1. Merge `lane/core` and `lane/ui` (D7); full `cargo test`, `bunx vitest run`.
2. `tauri.conf.json`: `productName: "Screenshot Studio"`, `bundle.targets: ["nsis"]`, `bundle.windows.nsis.installMode: "currentUser"`
   (no UAC), `startMenuFolder` unset (shortcut directly in Programs so search shows "Screenshot Studio"), app icon set
   (generate with `cargo tauri icon` from one 1024 px source), `webviewInstallMode` default. Single-instance: second
   launch focuses the existing window (plugin registered in 1.1; behaviour here).
3. `cargo tauri build`. Installer path `src-tauri/target/release/bundle/nsis/Screenshot Studio_<version>_x64-setup.exe`.
4. Human-approved install: `"<installer>" /S /D=D:\IOT_Tools\apps\ScreenshotStudio` (`/D=` must be last; Deferred decision #13).
5. `scripts/accept/p2.mjs` checks (first: FAIL if a `screenshot-studio.exe` process is already running, D10): tests green (child processes); `D:\IOT_Tools\apps\ScreenshotStudio\screenshot-studio.exe`
   exists; a Start Menu shortcut `Screenshot Studio.lnk` exists under `%APPDATA%\Microsoft\Windows\Start Menu\Programs`
   (recursive) and its target (read via PowerShell `WScript.Shell` CreateShortcut `.TargetPath` in a child process)
   points into the install dir; launching the exe shows a window titled `Screenshot Studio` within 5 s (process + window
   title check), then closes it; a second launch does not create a second process (single-instance);
   `memory`: app + WebView2 private bytes <= 500 MB after scrolling the whole real library (Deferred decision #31;
   reuse `scripts/accept/lib/run.mjs` `treeMemory` and `lib/cdp.mjs` `scrollFrames`; 1.4 measured 1,208 MB).
6. Windows Search check with the `cua-driver` MCP (owner rule): open Start, type `Screenshot Studio`, verify the top
   result is the app, launch it, `verify_state` the window. Record the result in the phase notes. (Not the Accept command:
   it needs the MCP, the Accept stays a plain node run.) The Accept's Start Menu shortcut check is a stated PROXY for
   "found in Windows Search" (Search lists Start Menu Programs shortcuts); the cua-driver result is recorded in `docs/lessons.md`
   and the phase is not reported done without it.

## Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| `/D=` ignored in currentUser mode | low | medium | p2 checks the exe path; fallback: installer UI dir page (human) |
| Windows Search index lags | medium | low | shortcut presence is the Accept; cua-driver check retried after 1 min |
| App data written to C: by WebView2 (EBWebView) | medium | low | set the WebView2 user data folder under the data dir via Tauri `data_directory` on the window builder; p2 asserts `%LOCALAPPDATA%\dev.nghaiz.screenshotstudio` holds no EBWebView |
