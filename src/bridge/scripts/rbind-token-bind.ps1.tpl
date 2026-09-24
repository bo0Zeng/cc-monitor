# 设计/80 §8.7 步 3 收尾（第二波 T4）：本地半的**生产写入方** —— 令牌握手前奏。
#
# monitor 拉起一个终端窗口（`launch.rs::launch_remote_terminal`）时，把这一段接在要跑的命令**前面**，
# 同一个 `-EncodedCommand` 里。它做的事与 `cc.ps1.tpl` 的 `__ccm_bind` 逐步相同（Era 2 那条握手，
# `bind.rs` 头注的信息流 1–3），只差一件：marker 不是 `ccm-bind-<PID>-<8hex>`，
# 而是 `ccm-rbind-token-<32hex>` —— **令牌就是 marker 本身**（`§8.2`「marker = token」）。
# ⇒ `bind.rs` 那一侧一行不用改：`entry_from_marker_hit` 从 marker 里解出令牌、记进同一张表。
#
# 顺序承重（v2 竞态修复，与 `__ccm_bind` 同一条）：**先设窗口标题、再写 await 文件**。
# monitor 的 notify 在文件落地那一刻就会去扫窗口标题找 marker。
#
# 失败一律静默放行（`catch {}`）：这段只为 ↗ 服务，它坏了不许挡住用户真正要跑的那条命令。
# 放行的代价是 ↗ 那一侧会如实说「这个会话带着令牌，但本地没有登记到它的窗口」（`bind.rs` 分派）。
#
# ⚠ 以 `#` 开头的整行在渲染时被剥掉（`launch.rs::render_rbind_bind_prelude`），不进 `-EncodedCommand`。
# ⚠ 两个占位符由 `launch.rs` 填，已按 PowerShell 单引号字面量转义：{{MARKER}} · {{AWAIT_DIR}}。
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
