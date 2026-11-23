#Requires -Version 7.0
<#
.SYNOPSIS
  Machine-wide Rust + MSVC toolchain on D: (idempotent). Dry-run by default; pass -Apply to change anything.

.DESCRIPTION
  All paths come from scripts/rust-machine.config.json. Every step is "check, then act only if needed".
  Ends by running scripts/verify-rust-machine.mjs and exits with its code.

.ROLLBACK (never run automatically)
  rustup self uninstall -y
  winget uninstall --id Microsoft.VisualStudio.BuildTools
  (elevated) Remove-ItemProperty 'HKLM:\SOFTWARE\Policies\Microsoft\VisualStudio\Setup' -Name CachePath
  [Environment]::SetEnvironmentVariable('<RUSTUP_HOME|CARGO_HOME|SCCACHE_DIR|HF_HOME>', $null, 'User')
  code --uninstall-extension <id>
#>
[CmdletBinding()]
param(
  [switch]$Apply,
  [switch]$SkipVsBuildTools,
  [switch]$SkipVsCode
)
$ErrorActionPreference = 'Stop'
$cfg = Get-Content -Raw (Join-Path $PSScriptRoot 'rust-machine.config.json') | ConvertFrom-Json
$P = $cfg.paths

function Step([string]$msg) { Write-Host "`n== $msg" -ForegroundColor Cyan }
function Act([string]$what, [scriptblock]$do) {
  if ($Apply) { Write-Host "DO    $what"; & $do } else { Write-Host "WOULD $what" -ForegroundColor Yellow }
}
function FreeGb([string]$drive) { [math]::Round((Get-PSDrive $drive).Free / 1GB, 1) }

# 1. Preflight
Step 'Preflight'
if (-not [Environment]::Is64BitOperatingSystem) { throw 'Windows x64 required' }
if (-not (Get-Command winget -ErrorAction SilentlyContinue)) { throw 'winget not found' }
if (-not (Test-Path 'D:\')) { throw 'Drive D: not found' }
$freeC = FreeGb 'C'; $freeD = FreeGb 'D'
Write-Host "Free space: C: $freeC GB, D: $freeD GB  (mode: $(if ($Apply) {'APPLY'} else {'DRY-RUN'}))"
if ($freeC -lt $cfg.minFreeGbC) { throw "C: has $freeC GB free, below minFreeGbC=$($cfg.minFreeGbC)" }

# 2. Directories
Step 'Directories'
$dirs = @($P.RUSTUP_HOME, $P.CARGO_HOME, $P.SCCACHE_DIR, $P.vsPackageCache, $P.downloads, $P.HF_HOME) + $cfg.extraDirs
foreach ($d in $dirs) {
  if (Test-Path $d) { Write-Host "OK    $d" } else { Act "create $d" { New-Item -ItemType Directory -Force $d | Out-Null } }
}

# 3. User env vars (current process always gets them so later steps see them)
Step 'User environment variables'
foreach ($name in $cfg.userEnv) {
  $want = $P.$name
  $have = [Environment]::GetEnvironmentVariable($name, 'User')
  if ($have -eq $want) { Write-Host "OK    $name=$want" }
  else { Act "set User $name=$want (was '$have')" { [Environment]::SetEnvironmentVariable($name, $want, 'User') } }
  Set-Item "env:$name" $want
}

# 4. Elevated block: HKLM CachePath + VS Build Tools (one UAC prompt)
Step 'Elevated: VS package cache policy + VS Build Tools'
$curCache = (Get-ItemProperty $cfg.vsPolicyKey -Name CachePath -ErrorAction SilentlyContinue).CachePath
$needCache = $curCache -ne $P.vsPackageCache
$vcPath = $null
if (Test-Path $cfg.vswhere) {
  $vcPath = & $cfg.vswhere -products * -requires $cfg.vsBuildTools.requiresComponent -property installationPath | Select-Object -First 1
}
$needVs = (-not $SkipVsBuildTools) -and (-not $vcPath)
Write-Host "CachePath: current='$curCache' -> $(if ($needCache) {'needs set'} else {'OK'})"
Write-Host "VC tools : $(if ($vcPath) {"found at $vcPath"} elseif ($SkipVsBuildTools) {'skipped'} else {'missing -> install'})"
if ($needCache -or $needVs) {
  $wl = ($cfg.vsBuildTools.workloads | ForEach-Object { "--add $_" }) -join ' '
  if ($cfg.vsBuildTools.includeRecommended) { $wl += ' --includeRecommended' }
  $override = "--installPath `"$($P.vsBuildToolsInstall)`" $wl --passive --wait --norestart"
  $child = @"
`$ErrorActionPreference = 'Stop'
`$code = 0
if (`$$needCache) {
  New-Item -Path '$($cfg.vsPolicyKey)' -Force | Out-Null
  Set-ItemProperty -Path '$($cfg.vsPolicyKey)' -Name CachePath -Value '$($P.vsPackageCache)' -Type String
}
if (`$$needVs) {
  winget install --id $($cfg.vsBuildTools.wingetId) --exact --source winget --accept-package-agreements --accept-source-agreements --override '$override'
  `$code = `$LASTEXITCODE
}
exit `$code
"@
  Act "run elevated child (UAC): CachePath=$needCache, VSBuildTools=$needVs, override: $override" {
    $childFile = Join-Path $env:TEMP "ss-rust-elevated-$PID.ps1"
    Set-Content -Path $childFile -Value $child -Encoding UTF8
    $proc = Start-Process pwsh -Verb RunAs -Wait -PassThru -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $childFile)
    Remove-Item $childFile -ErrorAction SilentlyContinue
    if ($proc.ExitCode -eq 3010) { Write-Warning 'VS Build Tools installed; a reboot is required (3010).' }
    elseif ($proc.ExitCode -ne 0) { throw "Elevated step failed with exit code $($proc.ExitCode)" }
  }
}

# 5. rustup
Step 'rustup'
$bin = Join-Path $P.CARGO_HOME 'bin'
$rustup = Join-Path $bin 'rustup.exe'
$cargo = Join-Path $bin 'cargo.exe'
if (-not (Test-Path $rustup)) {
  $init = Join-Path $P.downloads 'rustup-init.exe'
  Act "download $($cfg.rustupInitUrl) -> $init and install $($cfg.toolchain) ($($cfg.profile))" {
    Invoke-WebRequest -Uri $cfg.rustupInitUrl -OutFile $init
    & $init -y --default-toolchain $cfg.toolchain --profile $cfg.profile
    if ($LASTEXITCODE -ne 0) { throw "rustup-init failed ($LASTEXITCODE)" }
  }
} else {
  Act "rustup update $($cfg.toolchain)" { & $rustup update $cfg.toolchain; if ($LASTEXITCODE -ne 0) { throw 'rustup update failed' } }
}
$env:PATH = "$bin;$env:PATH"

# 6. Components
Step 'Components'
Act "rustup component add $($cfg.components -join ' ')" {
  & $rustup component add @($cfg.components); if ($LASTEXITCODE -ne 0) { throw 'component add failed' }
}

# 7. Cargo tools (sccache first; rustc-wrapper is written only after sccache exists)
Step 'Cargo tools'
foreach ($t in $cfg.cargoTools) {
  if (Test-Path (Join-Path $bin $t.bin)) { Write-Host "OK    $($t.crate)"; continue }
  Act "cargo install --locked $($t.crate)" {
    & $cargo install --locked $t.crate; if ($LASTEXITCODE -ne 0) { throw "cargo install $($t.crate) failed" }
  }
}

# 8. Global cargo config (merge, never clobber)
Step 'Global cargo config'
$cfgToml = Join-Path $P.CARGO_HOME 'config.toml'
$sccacheExists = Test-Path (Join-Path $bin 'sccache.exe')
$text = if (Test-Path $cfgToml) { Get-Content -Raw $cfgToml } else { '' }
if ($text -match '(?m)^\s*rustc-wrapper\s*=') { Write-Host "OK    rustc-wrapper already set in $cfgToml" }
elseif ($Apply -and -not $sccacheExists) { throw 'sccache.exe missing; refusing to write rustc-wrapper' }
else {
  Act "add [build] rustc-wrapper = `"sccache`" to $cfgToml" {
    $line = 'rustc-wrapper = "sccache"'
    if ($text -match '(?m)^\[build\]\s*$') { $new = $text -replace '(?m)^\[build\]\s*$', "[build]`n$line" }
    else { $new = ($text.TrimEnd() + "`n`n[build]`n$line`n").TrimStart() }
    Set-Content -Path $cfgToml -Value $new -Encoding UTF8NoBOM
  }
}

# 9. VS Code extensions
Step 'VS Code extensions'
$code = Get-Command code -ErrorAction SilentlyContinue
if ($SkipVsCode) { Write-Host 'skipped' }
elseif (-not $code) { Write-Warning '`code` not on PATH; skipping extensions' }
else {
  $have = (& code --list-extensions) | ForEach-Object { $_.ToLowerInvariant() }
  foreach ($ext in $cfg.vsCodeExtensions) {
    if ($have -contains $ext.ToLowerInvariant()) { Write-Host "OK    $ext" }
    else { Act "code --install-extension $ext" { & code --install-extension $ext --force | Out-Host } }
  }
}

# 10. Verify
Step 'Verify'
& node (Join-Path $PSScriptRoot 'verify-rust-machine.mjs')
exit $LASTEXITCODE
