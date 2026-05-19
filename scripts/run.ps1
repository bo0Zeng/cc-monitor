# monitor command wrapper: auto-injects MSVC dev shell env.
# Usage (Windows PowerShell 5.1 / PowerShell 7+):
#   powershell -NoProfile -File scripts\run.ps1 [dev|build|check|clean]

param(
    [Parameter(Position=0)]
    [ValidateSet("dev","build","check","clean")]
    [string]$Cmd = "dev"
)

$ErrorActionPreference = "Stop"

$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path $vswhere)) {
    throw "vswhere.exe not found at $vswhere. Install Visual Studio Build Tools 2022 first."
}

$installPath = & $vswhere -latest -products '*' -requires Microsoft.VisualCpp.Tools.HostX64.TargetX64 -property installationPath
if (-not $installPath) {
    throw "MSVC C++ tools not found via vswhere. Install the VCTools workload."
}

$devShell = Join-Path $installPath "Common7\Tools\Launch-VsDevShell.ps1"
if (-not (Test-Path $devShell)) {
    throw "Launch-VsDevShell.ps1 not found at $devShell"
}

& $devShell -SkipAutomaticLocation -Arch amd64 -HostArch amd64 | Out-Null

Set-Location (Resolve-Path "$PSScriptRoot\..")

switch ($Cmd) {
    "dev"   { npx tauri dev }
    "build" { npx tauri build }
    "check" { cargo check --manifest-path src-tauri\Cargo.toml }
    "clean" { cargo clean --manifest-path src-tauri\Cargo.toml }
}
