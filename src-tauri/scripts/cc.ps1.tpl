# === cc-monitor BEGIN v1 ===
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
    # monitor 启动时把自己 exe 路径写到 auto-launch.json，cc function 不硬编码路径
    if (Test-Path $autoLaunchFile) {
        try {
            $alCfg = Get-Content $autoLaunchFile -Raw -ErrorAction Stop | ConvertFrom-Json
            if ($alCfg.auto_launch_enabled -and $alCfg.monitor_exe_path) {
                $monPath = $alCfg.monitor_exe_path
                if (Test-Path $monPath) {
                    # 已有同路径 monitor 进程 → 不重复启动
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
                        Start-Process -FilePath $monPath -ErrorAction SilentlyContinue | Out-Null
                        # 等 monitor BindRegistry watcher 起来（Tauri 初始化 + setup() 约 1-2s）
                        Start-Sleep -Milliseconds 2000
                    }
                }
            }
        } catch {}
    }

    $marker = "ccm-bind-{0}-{1}" -f $PID, [guid]::NewGuid().ToString('N').Substring(0,8)
    $awaitDir = Join-Path $ccmDir "ps-await"
    try { New-Item -ItemType Directory -Path $awaitDir -Force -ErrorAction Stop | Out-Null } catch {}
    $awaitFile = Join-Path $awaitDir "$PID.json"
    @{ ps_pid = $PID; marker = $marker; proc_start = "$procStart" } |
        ConvertTo-Json -Compress | Out-File -Encoding utf8 $awaitFile

    $oldTitle = $Host.UI.RawUI.WindowTitle
    $Host.UI.RawUI.WindowTitle = $marker
    $deadline = (Get-Date).AddMilliseconds(800)
    while ((Test-Path $awaitFile) -and ((Get-Date) -lt $deadline)) {
        Start-Sleep -Milliseconds 30
    }
    $Host.UI.RawUI.WindowTitle = $oldTitle

    if (Test-Path $awaitFile) {
        Remove-Item $awaitFile -Force -ErrorAction SilentlyContinue
        Write-Warning "cc-monitor: 绑定超时 (monitor 没在跑？)"
    }
}

function {{COMMAND_NAME}} {
    [CmdletBinding()] param(
        [Parameter(ValueFromRemainingArguments = $true)] $RemainingArgs
    )
    __ccm_bind
    & claude $RemainingArgs
}
# === cc-monitor END ===
