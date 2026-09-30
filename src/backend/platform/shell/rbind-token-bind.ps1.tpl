# 设计/80 §8.2 本地半的写入方 —— 令牌握手前奏（〔P5〕由本机后端渲：`powershell.rs::rbind_bind_prelude`，接线在 `dial/terminal.rs::with_bind_prelude`）。
#
# 开终端时（`terminal-local` · `terminal-ssh` 带 `rbindToken`）把这一段接在要跑的命令**前面**，同一个 `-EncodedCommand` 里。
# 它做的事与 `cc.ps1.tpl` 的 `__ccm_bind` 逐步相同（Era 2 那条握手，monitor `bind.rs` 头注的信息流 1–3），只差一件：
# marker 不是 `ccm-bind-<PID>-<8hex>`，而是 `ccm-rbind-token-<32hex>` —— **令牌就是 marker 本身**（`§8.2`「marker = token」）。
#
# 顺序承重（v2 竞态修复，与 `__ccm_bind` 同一条）：**先设窗口标题、再写 await 文件**。
# monitor 的 notify 在文件落地那一刻就会去扫窗口标题找 marker。
#
# 失败一律静默放行（`catch {}`）：这段只为 ↗ 服务，它坏了不许挡住用户真正要跑的那条命令。
#
# ⚠ 以 `#` 开头的整行在渲染时被剥掉，不进 `-EncodedCommand`（命令行 32767 字符的额度）。
# ⚠ 两个占位符按 PowerShell 单引号字面量填（`dialect::ps_literal`）：{{MARKER}} · {{AWAIT_DIR}}。
# ⚠ 写进 await 文件的三个键是 monitor `bind.rs::AwaitRequest` 的契约（判据 `bind_tests` 读本文件）。
& {
    $m = {{MARKER}}
    $d = {{AWAIT_DIR}}
    try {
        $s = (Get-Process -Id $PID).StartTime.ToFileTime()
        New-Item -ItemType Directory -Path $d -Force -ErrorAction Stop | Out-Null
        $f = Join-Path $d "$PID.json"
        $t = $Host.UI.RawUI.WindowTitle
        $Host.UI.RawUI.WindowTitle = $m
        $j = @{ ps_pid = $PID; marker = $m; proc_start = "$s" } | ConvertTo-Json -Compress
        [System.IO.File]::WriteAllText($f, $j, (New-Object System.Text.UTF8Encoding($false)))
        $e = (Get-Date).AddMilliseconds(3000)
        while ((Test-Path $f) -and ((Get-Date) -lt $e)) { Start-Sleep -Milliseconds 30 }
        if (Test-Path $f) { Remove-Item $f -Force -ErrorAction SilentlyContinue }
        $Host.UI.RawUI.WindowTitle = $t
    } catch {}
}
