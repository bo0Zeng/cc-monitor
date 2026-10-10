# === cc-monitor BEGIN v9 ===
# 自动生成 — 卸载请用 cc-monitor 设置面板 [卸载]，或手动删除 BEGIN/END 之间所有内容。
# 文档: https://github.com/bo0Zeng/cc-monitor

function __ccm_bind {
    # 设置里勾了「用 cc 启动 claude 时自动打开 monitor」：monitor 没在跑就在后台把它启动起来（不抢这个终端的焦点）。
    # 清单里每条别名开头那一行调它（名字沿用）。切到终端不靠这里：cc-monitor 点 ↗ 那一刻按这个终端的控制台认窗口。
    $ccmDir = {{MONITOR_DATA_DIR}}
    $autoLaunchFile = Join-Path $ccmDir 'auto-launch.json'
    if (-not (Test-Path $autoLaunchFile)) { return }
    try {
        $alCfg = Get-Content $autoLaunchFile -Raw -ErrorAction Stop | ConvertFrom-Json
        if (-not ($alCfg.auto_launch_enabled -and $alCfg.monitor_exe_path)) { return }
        $monPath = $alCfg.monitor_exe_path
        if (-not (Test-Path $monPath)) { return }
        foreach ($p in @(Get-Process -ErrorAction SilentlyContinue)) {
            try { if ($p.Path -eq $monPath) { return } } catch {}
        }
        Start-Process -FilePath $monPath -ArgumentList '--background' -ErrorAction SilentlyContinue | Out-Null
    } catch {}
}
# 这个窗口的标签（进程号-起始时刻）：经下面那层 ssh 送到远端，远端只回显，↗ 先按它找这个 PowerShell 所在的窗口。
try { $env:LC_CCM_WINDOW = '{0}-{1}' -f $PID, [System.Diagnostics.Process]::GetCurrentProcess().StartTime.ToFileTime() } catch {}
# ssh 包一层：带上窗口标签（远端 sshd 默认收 LC_* 变量；不收就只是没送到）。其余参数原样交给系统的 ssh。
function ssh {
    $exe = Get-Command ssh -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $exe) { Write-Error 'ssh: command not found'; return }
    & $exe.Source -o SendEnv=LC_CCM_WINDOW @args
}
# 接上 cc-monitor「别名」那一块写的别名文件（没生成过就什么都不做）。
if (Test-Path -LiteralPath "$HOME\.cc-monitor\aliases.ps1") { . "$HOME\.cc-monitor\aliases.ps1" }
# === cc-monitor END ===
