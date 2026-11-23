# Phase 0: Machine-wide Rust toolchain on D: (serial, human-gated)

Lane: serial
Depends-on: none
Gate: human (approve env vars, HKLM CachePath, UAC for VS Build Tools, software installs; see README Human steps)
Status: DONE 2026-11-23 (verify-rust-machine.mjs: 18/18 PASS; log D:\DevCache\cache\setup-rust-machine.log)
Effort: M
Creates: `scripts/setup-rust-machine.ps1`, `scripts/verify-rust-machine.mjs`, `scripts/rust-machine.config.json`
AX: AX-01 the verifier prints one `PASS|FAIL <check> <value>` line per check plus `--json`-free stable exit codes (0 all pass, 1 any fail); the setup script is dry-run by default and prints every change it would make
Accept: `node scripts/verify-rust-machine.mjs`

## Goal

A machine-wide, reusable Rust + MSVC toolchain whose every byte that can live on D: lives on D:, exactly per the
brainstorm's D: layout table, installed by ONE idempotent PowerShell script that ends with a verify step. Nothing in
this phase is project-specific (no `git init` here, that is phase 1.1).

## D: layout (from the approved design, not re-opened)

| What | Path | Mechanism |
|---|---|---|
| rustup toolchains | `D:\DevCache\tools\rust\rustup` | user env `RUSTUP_HOME` |
| cargo bin/registry | `D:\DevCache\tools\rust\cargo` | user env `CARGO_HOME`, `%CARGO_HOME%\bin` on user PATH (rustup-init adds it) |
| sccache | `D:\DevCache\cache\sccache` | user env `SCCACHE_DIR`; `[build] rustc-wrapper = "sccache"` in the global cargo config |
| VS Build Tools | `D:\IOT_Tools\apps\VSBuildTools` | `--installPath` |
| VS package cache | `D:\DevCache\cache\vs-packages` | `HKLM\SOFTWARE\Policies\Microsoft\VisualStudio\Setup` `CachePath` (REG_SZ; 50-char limit, this is 29) |
| HF / AI models | `D:\DevCache\huggingface` | user env `HF_HOME` |

Global cargo config file: `%CARGO_HOME%\config.toml` = `D:\DevCache\tools\rust\cargo\config.toml` (Deferred decision #4:
with `CARGO_HOME` set, cargo reads `$CARGO_HOME/config.toml`, never `%USERPROFILE%\.cargo\config.toml`).
Known residue on C: (accepted in the design): VS shared components + part of the Windows SDK under
`C:\Program Files (x86)\Microsoft Visual Studio\Shared` / `Windows Kits`, because SSMS 21 fixed `SharedInstallationPath`.

## Files

- `scripts/rust-machine.config.json` — the single data file: paths above, `toolchain: "stable"`, `profile: "minimal"`,
  `components: ["clippy","rustfmt","rust-src"]`, `cargoTools: ["sccache","tauri-cli"]`,
  `vsBuildTools: { wingetId: "Microsoft.VisualStudio.BuildTools", workloads: ["Microsoft.VisualStudio.Workload.VCTools"], includeRecommended: true }`,
  `vsCodeExtensions: [rust-lang.rust-analyzer, tauri-apps.tauri-vscode, ms-vscode.cpptools, tamasfe.even-better-toml, ms-python.python, charliermarsh.ruff, dbaeumer.vscode-eslint, bradlc.vscode-tailwindcss]`,
  `minFreeGbC: 10`. Both scripts read it; no path literal is duplicated in code.
- `scripts/setup-rust-machine.ps1` — PowerShell 7 (`pwsh`, CurrentUser policy is RemoteSigned, MEASURED). Parameters:
  `-Apply` (default off = dry-run), `-SkipVsBuildTools`, `-SkipVsCode`. Every step is "check, then act only if needed".
- `scripts/verify-rust-machine.mjs` — Node 26, read-only. Reads user env from the registry (`reg query HKCU\Environment`)
  so it works in a shell started BEFORE the env change, and calls tools by absolute path under `CARGO_HOME\bin`.

## Steps (setup-rust-machine.ps1, in this order)

1. Preflight: assert Windows x64, `pwsh` 7+, winget present, D: exists; print C: and D: free space; abort if C: free
   < `minFreeGbC`. Load `rust-machine.config.json`.
2. Create dirs (idempotent): `D:\DevCache\tools\rust\{rustup,cargo}`, `D:\DevCache\cache\{sccache,vs-packages,downloads}`,
   `D:\DevCache\huggingface`, `D:\IOT_Tools\apps`, `D:\IOT_Tools\data`.
3. User env vars (HUMAN-APPROVED): for each of `RUSTUP_HOME`, `CARGO_HOME`, `SCCACHE_DIR`, `HF_HOME`: if the current
   user value differs, `[Environment]::SetEnvironmentVariable(name, value, 'User')`; always set the same in the current
   process (`$env:NAME`) so later steps see them. Never touch Machine scope env.
4. Elevated block (HUMAN-APPROVED, one UAC prompt): write `Start-Process pwsh -Verb RunAs -Wait` running an inline
   `-File` child script from `$env:TEMP` that (a) sets `HKLM\SOFTWARE\Policies\Microsoft\VisualStudio\Setup` `CachePath`
   if it differs, (b) runs `winget install --id Microsoft.VisualStudio.BuildTools --exact --source winget --accept-package-agreements --accept-source-agreements --override "--installPath D:\IOT_Tools\apps\VSBuildTools --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --passive --wait --norestart"`
   only if `vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`
   does not already return a path. Read the child's exit code; 3010 (reboot required) is reported, not failed.
5. rustup: if `%CARGO_HOME%\bin\rustup.exe` is absent, download `https://win.rustup.rs/x86_64` to
   `D:\DevCache\cache\downloads\rustup-init.exe` and run `rustup-init.exe -y --default-toolchain stable --profile minimal`
   (it honours the process `RUSTUP_HOME`/`CARGO_HOME` and adds `%CARGO_HOME%\bin` to user PATH). Else `rustup update stable`.
6. `rustup component add clippy rustfmt rust-src` (idempotent).
7. Cargo tools: for each of `cargoTools` not already in `cargo install --list`: `cargo install --locked <tool>`.
   sccache FIRST. Order matters: the `rustc-wrapper` line (step 8) must not exist until `sccache.exe` exists, or every
   cargo invocation fails.
8. Global cargo config `%CARGO_HOME%\config.toml`: create or merge (never clobber other keys) `[build] rustc-wrapper = "sccache"`.
   Write with a TOML-aware merge: if the file exists and already has a `[build]` table, only add the key when absent.
9. VS Code: if `code` is on PATH (MEASURED: `D:\Visual Studio Code\...\bin\code`), install each missing extension with
   `code --install-extension <id>`; skip with a WARN line when `code` is absent.
10. Verify: run `node scripts/verify-rust-machine.mjs` and exit with its code. In dry-run mode, steps 2-9 print
    `WOULD <action>` lines and the verify step runs anyway (it reports what is still missing).

## Verifier checks (verify-rust-machine.mjs), each one line `PASS|FAIL <name> <observed>`

1. User env `RUSTUP_HOME`, `CARGO_HOME`, `SCCACHE_DIR`, `HF_HOME` equal the config paths (registry read).
2. User PATH contains `D:\DevCache\tools\rust\cargo\bin`.
3. `cargo --version`, `rustc --version`, `rustup show home` (must print the D: rustup path), `rustc --print host-tuple`
   = `x86_64-pc-windows-msvc`. All invoked by absolute path.
4. `rustup component list --installed` contains clippy, rustfmt, rust-src.
5. `sccache --version` and `cargo tauri --version` run.
6. `%CARGO_HOME%\config.toml` has `build.rustc-wrapper = "sccache"`.
7. `vswhere` reports an installation whose path starts with `D:\IOT_Tools\apps\VSBuildTools` and contains
   `VC\Tools\MSVC\*\bin\Hostx64\x64\link.exe`.
8. HKLM `CachePath` = `D:\DevCache\cache\vs-packages`.
9. Smoke build: `cargo new --vcs none` a hello crate under `%TEMP%\ss-rust-smoke-<pid>`, `cargo build` it with sccache,
   run the binary, expect `Hello, world!`, delete the temp dir. Confirms MSVC link works end-to-end.
10. No `rustup`/`.cargo` dirs were created under `%USERPROFILE%` by this run (WARN, not FAIL, if pre-existing).

## Rollback

`rustup self uninstall -y` (removes D: toolchains and the PATH entry); `winget uninstall --id Microsoft.VisualStudio.BuildTools`;
delete the HKLM `CachePath` value (elevated); remove the four user env vars; `code --uninstall-extension <id>`. Listed in
the script header; never run automatically.

## Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| VS installer ignores `--installPath` because an instance exists | low | medium | vswhere check first; verifier check 7 |
| Shared path forces several GB onto C: | medium | medium | preflight + post free-space print; abort under 10 GB free |
| `cargo install tauri-cli` takes 10+ min | high | low | runs once, sccache warms; idempotent skip next time |
| rustc-wrapper written before sccache exists breaks cargo | medium | high | strict step order 7 then 8; verifier check 5 before 6 |
| Env vars invisible to already-open shells | high | low | verifier reads the registry; Human step "restart terminals" before 1.1 |
