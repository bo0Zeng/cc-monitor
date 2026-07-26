# F-E5 Tier2 —— session-1 payload。
#
# 这是**被 schtasks /it 触发、在交互会话（session 1）里跑**的脚本（见 README 的 hop）。
# 职责：设显式路径 env（非交互会话 PATH 可能瘦）→ 跑 wdio → 输出重定向到 wdio.log
# 并写「TIER2 START/END」标记，供 aya 侧轮询回盘（别信内联回显，回盘读 log 才算数）。
#
# 直接从本脚本所在目录（e2e/tier2/）跑；node_modules 由 Node 向上解析（repo 根 npm install
# 后落 repo/node_modules；或复用 VM e2e-spike 缓存的 node_modules——把本目录内容拷过去即可）。
$ErrorActionPreference = 'Continue'
Set-Location $PSScriptRoot

$log = Join-Path $PSScriptRoot 'wdio.log'

# 显式路径（可用同名 env 覆盖）。非交互会话 PATH 可能不含 .cargo\bin，故内嵌绝对路径。
if (-not $env:APP_EXE)      { $env:APP_EXE      = 'C:/Users/vm260726/cc-monitor/src-tauri/target/debug/monitor.exe' }
if (-not $env:TAURI_DRIVER) { $env:TAURI_DRIVER = "$env:USERPROFILE\.cargo\bin\tauri-driver.exe" }
if (-not $env:MSEDGEDRIVER) { $env:MSEDGEDRIVER = "$env:USERPROFILE\.cargo\bin\msedgedriver.exe" }

# 全程用 PS 5.1 默认编码（Unicode/UTF-16LE），跟 `*>>` 追加的 wdio 输出一致——
# 整个 log 单一编码，aya 侧 `iconv -f UTF-16LE`（或 Get-Content）可干净回盘读，
# 不会 UTF-8 头 + UTF-16 体混编导致乱码。
Remove-Item $log -ErrorAction SilentlyContinue
("=== TIER2 START $(Get-Date -Format o) ===")            | Out-File -FilePath $log
("SessionId=" + (Get-Process -Id $PID).SessionId)         | Out-File -Append $log
("APP_EXE exists? "      + (Test-Path $env:APP_EXE))      | Out-File -Append $log
("TAURI_DRIVER exists? " + (Test-Path $env:TAURI_DRIVER)) | Out-File -Append $log
("MSEDGEDRIVER exists? " + (Test-Path $env:MSEDGEDRIVER)) | Out-File -Append $log

npx wdio run wdio.conf.mjs *>> $log

("WDIO_EXIT=" + $LASTEXITCODE)                | Out-File -Append $log
("=== TIER2 END $(Get-Date -Format o) ===")   | Out-File -Append $log
Write-Output ("DONE WDIO_EXIT=" + $LASTEXITCODE)
