# === cc-monitor BEGIN v5 ===
# 自动生成 — 卸载请用 cc-monitor 设置面板 [卸载]，或手动删除 BEGIN/END 之间所有内容。
# 文档: https://github.com/bo0Zeng/cc-monitor

function __ccm_bind {
    # 把这个 PowerShell 窗口登记给 cc-monitor，之后 Tab ↗ 能切到它。已经登记过就直接返回。
    $ccmDir = {{MONITOR_DATA_DIR}}
    $regFile = Join-Path $ccmDir "ps-registry\$PID.json"
    $autoLaunchFile = Join-Path $ccmDir 'auto-launch.json'

    try {
        $procStart = (Get-Process -Id $PID).StartTime.ToFileTime()
    } catch {
        Write-Warning "cc-monitor: get-process StartTime failed: $_"
        return
    }
    if (Test-Path $regFile) {
        try {
            $r = Get-Content $regFile -Raw -ErrorAction Stop | ConvertFrom-Json
            if ($r.ps_proc_start -eq "$procStart") { return }
        } catch {}
    }

    # 设置里勾了「用 cc 启动 claude 时自动打开 monitor」：monitor 没在跑就把它启动起来。
    if (Test-Path $autoLaunchFile) {
        try {
            $alCfg = Get-Content $autoLaunchFile -Raw -ErrorAction Stop | ConvertFrom-Json
            if ($alCfg.auto_launch_enabled -and $alCfg.monitor_exe_path) {
                $monPath = $alCfg.monitor_exe_path
                if (Test-Path $monPath) {
                    $running = $false
                    try {
                        $procs = Get-Process -ErrorAction SilentlyContinue
                        foreach ($p in $procs) {
                            try {
                                if ($p.Path -eq $monPath) { $running = $true; break }
                            } catch {}
                        }
                    } catch {}
                    if (-not $running) {
                        # --background：monitor 在后台启动，不抢这个终端的焦点。
                        Start-Process -FilePath $monPath -ArgumentList '--background' -ErrorAction SilentlyContinue | Out-Null
                    }
                }
            }
        } catch {}
    }

    $marker = "ccm-bind-{0}-{1}" -f $PID, [guid]::NewGuid().ToString('N').Substring(0,8)
    $awaitDir = Join-Path $ccmDir "ps-await"
    try { New-Item -ItemType Directory -Path $awaitDir -Force -ErrorAction Stop | Out-Null } catch {}
    $awaitFile = Join-Path $awaitDir "$PID.json"

    # 先设窗口标题、再写登记文件：monitor 一看到登记文件就按这个标题找窗口。
    $oldTitle = $Host.UI.RawUI.WindowTitle
    $Host.UI.RawUI.WindowTitle = $marker

    # 用不带 BOM 的 UTF-8 写（PowerShell 5.1 的 Out-File 会写 BOM）。
    $json = @{ ps_pid = $PID; marker = $marker; proc_start = "$procStart" } |
        ConvertTo-Json -Compress
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($awaitFile, $json, $utf8NoBom)

    # 最多等 3 秒，登记好了就走。
    $deadline = (Get-Date).AddMilliseconds(3000)
    $bound = $false
    while ((Test-Path $awaitFile) -and ((Get-Date) -lt $deadline)) {
        try {
            $r = Get-Content $regFile -Raw -ErrorAction Stop | ConvertFrom-Json
            if ($r.ps_proc_start -eq "$procStart") { $bound = $true; break }
        } catch {}
        Start-Sleep -Milliseconds 30
    }
    $Host.UI.RawUI.WindowTitle = $oldTitle

    if (-not $bound) {
        try {
            $r = Get-Content $regFile -Raw -ErrorAction Stop | ConvertFrom-Json
            if ($r.ps_proc_start -eq "$procStart") { $bound = $true }
        } catch {}
    }
    if (Test-Path $awaitFile) {
        Remove-Item $awaitFile -Force -ErrorAction SilentlyContinue
    }
    if (-not $bound -and -not (Test-Path $regFile)) {
        Write-Warning "cc-monitor: 绑定超时 (monitor 没在跑？)"
    }
}
{{CC_FUNCTION_BLOCK}}
# 接上 cc-monitor「别名」那一块写的别名文件（没生成过就什么都不做）。
if (Test-Path -LiteralPath "$HOME\.cc-monitor\aliases.ps1") { . "$HOME\.cc-monitor\aliases.ps1" }
# === cc-monitor END ===
