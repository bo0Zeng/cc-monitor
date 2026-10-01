# F-E5 Tier2 —— session-1 payload。
#
# 这是**被 schtasks /it 触发、在交互会话（session 1）里跑**的脚本（见 README 的 hop）。
# 职责：设显式路径 env（非交互会话 PATH 可能瘦）→ 跑 wdio → 输出重定向到 wdio.log
# 并写「TIER2 START/END」标记，供 aya 侧轮询回盘（别信内联回显，回盘读 log 才算数）。
#
# 直接从本脚本所在目录（tests/e2e/tier2/）跑；node_modules 由 Node 向上解析（repo 根 npm install
# 后落 repo/node_modules；或复用 VM e2e-spike 缓存的 node_modules——把本目录内容拷过去即可）。
$ErrorActionPreference = 'Continue'
Set-Location $PSScriptRoot

$log = Join-Path $PSScriptRoot 'wdio.log'

# 🔴APP_EXE 原先在这里兜着一个**三段全错**的默认值
#（用户名 vm260726 / 构建目录 src/frontend/shell/target / 那台机器上没有这个仓），
# 而 wdio.conf.mjs 里**抄着同一个错值** ⇒ 一个错的默认值有两处住址。
# ⇒ 改成**说不出就当场停**，理由与失败签名逐条写在 wdio.conf.mjs 那一处。
if (-not $env:APP_EXE) {
  throw "APP_EXE 没给 —— 这一档必须被告知被测的 cc-monitor.exe 在哪（不猜默认值）。同目录还要有 WebView2Loader.dll。"
}
# 这两个可以留空：tauri-driver 自己会去 PATH 里找 msedgedriver。
if (-not $env:TAURI_DRIVER) { $env:TAURI_DRIVER = "$env:USERPROFILE\.cargo\bin\tauri-driver.exe" }

# 🔴**node 那一格原先漏了。** 上面三个 env 都显式给了路径，
# 理由逐字是「非交互会话 PATH 可能瘦」—— 而下面那句 `npx wdio` 同样吃 PATH，
# 却没人给它。现打：那台虚拟机上 **node 与 npm 都不在 PATH 里**
# ⇒ 这一行在它该跑的机器上**必挂**，而挂出来的话跟 wdio 无关。
# ⇒ 用 NODE_DIR 指到便携版 node 的解压目录（里面是 node.exe / npx.cmd）。
if ($env:NODE_DIR) { $env:PATH = "$env:NODE_DIR;$env:PATH" }
if (-not (Get-Command npx -ErrorAction SilentlyContinue)) {
  throw "PATH 里找不到 npx —— 给 NODE_DIR 指到便携版 node 的解压目录（里面有 node.exe / npx.cmd）。"
}

# 全程用 PS 5.1 默认编码（Unicode/UTF-16LE），跟 `*>>` 追加的 wdio 输出一致——
# 整个 log 单一编码，aya 侧 `iconv -f UTF-16LE`（或 Get-Content）可干净回盘读，
# 不会 UTF-8 头 + UTF-16 体混编导致乱码。
Remove-Item $log -ErrorAction SilentlyContinue
("=== TIER2 START $(Get-Date -Format o) ===")            | Out-File -FilePath $log
("SessionId=" + (Get-Process -Id $PID).SessionId)         | Out-File -Append $log
("APP_EXE exists? "      + (Test-Path $env:APP_EXE))      | Out-File -Append $log
("TAURI_DRIVER exists? " + (Test-Path $env:TAURI_DRIVER)) | Out-File -Append $log
# ⚠ MSEDGEDRIVER 允许为空（让 tauri-driver 自己去 PATH 找）⇒ 空串不能喂 Test-Path。
("MSEDGEDRIVER = " + $(if ($env:MSEDGEDRIVER) { "$env:MSEDGEDRIVER exists? " + (Test-Path $env:MSEDGEDRIVER) } else { "(空 —— 交给 tauri-driver 自寻 PATH)" })) | Out-File -Append $log
("node = " + (Get-Command node -ErrorAction SilentlyContinue).Source) | Out-File -Append $log

npx wdio run wdio.conf.mjs *>> $log

("WDIO_EXIT=" + $LASTEXITCODE)                | Out-File -Append $log
("=== TIER2 END $(Get-Date -Format o) ===")   | Out-File -Append $log
Write-Output ("DONE WDIO_EXIT=" + $LASTEXITCODE)
