# === cc-monitor BEGIN v2 ===
# 自动生成 — 卸载请用 cc-monitor 设置面板 [卸载]，或手动删除 BEGIN/END 之间所有内容。
# 文档: https://github.com/bo0Zeng/cc-monitor

function __ccm_bind {
    # 在这个 PowerShell session 里向 cc-monitor 注册 (PS_PID -> 当前 console hwnd) 映射。
    # 已注册 + 进程指纹一致 → 直接返回（avoid title flicker on every invocation）。
    $ccmDir = Join-Path $env:USERPROFILE '.claude\claudecode-frontend'
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

    # v1.7.1：可选 auto-launch monitor（用户在 monitor UI 里 toggle 开启）
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
                        # 传 --background → monitor 启动时不抢前台焦点（窗口仍可见，但不从
                        # 当前终端偷焦点，用户接着敲 claude 不被打断）。手动双击启动不带此
                        # 参数，仍正常置前。
                        # v2：不再死等 2s——绑定等待循环本身"好了就走"（deadline 3s 覆盖
                        # monitor 启动 + 启动时 drain ps-await），起得快就不用白等。
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

    # v2 竞态修复：**先设窗口标题、再写 await 文件**。monitor 的 notify 在文件落地
    # 瞬间就会 EnumWindows 找 marker——旧顺序（先写文件后设标题）下 monitor 扫得越快
    # 越容易"找不到窗口"，把 await 删掉走失败路径，绑定成败全凭时序运气（v2.21 实测：
    # 每个新 shell 首次 cc 固定烧满超时）。
    $oldTitle = $Host.UI.RawUI.WindowTitle
    $Host.UI.RawUI.WindowTitle = $marker

    # v1.7.8：PS 5.1 `Out-File -Encoding utf8` 写 UTF-8 BOM，monitor 端 serde_json
    # 不剥 BOM 解析失败。这里用 .NET WriteAllText + UTF8Encoding($false) 显式无 BOM。
    # （v1.7.8 monitor 也加了剥 BOM 兜底，所以 v1.7.8 用户即使用老模板也能 work）
    $json = @{ ps_pid = $PID; marker = $marker; proc_start = "$procStart" } |
        ConvertTo-Json -Compress
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($awaitFile, $json, $utf8NoBom)

    # v2：deadline 800ms → 3s（覆盖 monitor 冷启动；循环"好了就走"，正常绑定仍是
    # 几十 ms 量级）；退出条件加"registry 已落地且指纹匹配"——不依赖 await 删除这
    # 一种信号，monitor 任何清理时序下注册一落地就返回。
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
# === cc-monitor END ===
