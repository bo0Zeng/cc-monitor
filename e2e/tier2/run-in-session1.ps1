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

Remove-Item $log -ErrorAction SilentlyContinue
("=== TIER2 START $(Get-Date -Format o) ===")            | Out-File -FilePath $log -Encoding utf8
("SessionId=" + (Get-Process -Id $PID).SessionId)         | Out-File -Append $log -Encoding utf8
("APP_EXE exists? "      + (Test-Path $env:APP_EXE))      | Out-File -Append $log -Encoding utf8
("TAURI_DRIVER exists? " + (Test-Path $env:TAURI_DRIVER)) | Out-File -Append $log -Encoding utf8
("MSEDGEDRIVER exists? " + (Test-Path $env:MSEDGEDRIVER)) | Out-File -Append $log -Encoding utf8

npx wdio run wdio.conf.mjs *>> $log

("WDIO_EXIT=" + $LASTEXITCODE)                | Out-File -Append $log -Encoding utf8
("=== TIER2 END $(Get-Date -Format o) ===")   | Out-File -Append $log -Encoding utf8
Write-Output ("DONE WDIO_EXIT=" + $LASTEXITCODE)
