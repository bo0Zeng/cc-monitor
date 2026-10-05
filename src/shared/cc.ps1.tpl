# === cc-monitor BEGIN v8 ===
# 自动生成 — 卸载请用 cc-monitor 设置面板 [卸载]，或手动删除 BEGIN/END 之间所有内容。
# 文档: https://github.com/bo0Zeng/cc-monitor

function __ccm_bind {
    # 把这个 PowerShell 窗口登记给 cc-monitor，之后 Tab ↗ 能切到它。已经登记过就直接返回。
    # 开 PowerShell 时（块尾）带 -Background 调一次：不等、不出声，认领交给后台 —— monitor 在跑就当场认，没在跑就等它起来再认。
    # 敲 cc 时（清单里 cc 开头那一行）不带参数调：勾了自动打开就先把 monitor 开起来，当场认领，最多等 3 秒。
    param([switch]$Background)
    $ccmDir = {{MONITOR_DATA_DIR}}

    # 认领一次。只用 .NET、不用任何命令（后台那一份跑在一个空的 runspace 里）。
    # 先设记号标题、再写待认领那份：monitor 一看到那份就按标题找窗口。monitor 认了（登记文件落地、指纹对上）、
    # 把那份删了、或过了 3 秒 ⇒ 标题若还是记号就还原。返回 bound 登记上了 · missed 标题一直挂着记号也没认上
    # （比如这个标签页当时不在前面）· again 中途标题被别人改了或 monitor 没回话 · none 只看了一眼（$checkOnly）、还没登记。
    $claim = {
        param($dir, $procId, $procStart, $checkOnly)
        $regFile = [System.IO.Path]::Combine($dir, 'ps-registry', "$procId.json")
        $awaitDir = [System.IO.Path]::Combine($dir, 'ps-await')
        $awaitFile = [System.IO.Path]::Combine($awaitDir, "$procId.json")
        $isBound = {
            if (-not [System.IO.File]::Exists($regFile)) { return $false }
            try {
                $m = [regex]::Match([System.IO.File]::ReadAllText($regFile), '"ps_proc_start"\s*:\s*"(\d+)"')
                $m.Success -and ($m.Groups[1].Value -eq "$procStart")
            } catch { $false }
        }
        if (& $isBound) { return 'bound' }
        if ($checkOnly) { return 'none' }
        $marker = 'ccm-bind-{0}-{1}' -f $procId, [guid]::NewGuid().ToString('N').Substring(0, 8)
        $kept = $false
        try {
            $oldTitle = [System.Console]::Title
            [System.Console]::Title = $marker
            [void][System.IO.Directory]::CreateDirectory($awaitDir)
            # 不带 BOM 的 UTF-8（PowerShell 5.1 的 Out-File 会写 BOM）。
            $json = '{"ps_pid":' + $procId + ',"marker":"' + $marker + '","proc_start":"' + $procStart + '"}'
            [System.IO.File]::WriteAllText($awaitFile, $json, [System.Text.UTF8Encoding]::new($false))
            $deadline = [DateTime]::UtcNow.AddMilliseconds(3000)
            while ([System.IO.File]::Exists($awaitFile) -and ([DateTime]::UtcNow -lt $deadline)) {
                if (& $isBound) { break }
                [System.Threading.Thread]::Sleep(30)
            }
            $kept = (-not [System.IO.File]::Exists($awaitFile)) -and ([System.Console]::Title -eq $marker)
        } catch {
        } finally {
            try { if ([System.Console]::Title -eq $marker) { [System.Console]::Title = $oldTitle } } catch {}
            try { [System.IO.File]::Delete($awaitFile) } catch {}
        }
        if (& $isBound) { 'bound' } elseif ($kept) { 'missed' } else { 'again' }
    }

    # 后台那一份（开 PowerShell 时这里什么都不等，连「登记过没有」也交给它看）。不定时醒，只等两样系统对象：
    # $up —— monitor 起来时置位、正常退出时复位的事件；$alive —— monitor 活着时一直占着的互斥量（崩了系统替它放手）。
    # 等到 $up ⇒ 先看一眼 $alive：拿得到 ⇒ 置位是上一个没正常退出的 monitor 留下的，清掉接着等；拿不到 ⇒ monitor 在跑，认领。
    # 没认上（标题中途被改、这个标签页当时不在前面……）⇒ 等这个 monitor 走（拿到 $alive），清掉 $up，再等下一个起来。
    $watch = {
        param($claimText, $gate, $state, $dir, $upName, $aliveName)
        [System.Threading.Thread]::CurrentThread.IsBackground = $true
        $claim = [scriptblock]::Create($claimText)
        $me = [System.Diagnostics.Process]::GetCurrentProcess()
        $procId = $me.Id
        $procStart = $me.StartTime.ToFileTime()
        if ((& $claim $dir $procId $procStart $true) -eq 'bound') { $state.done = $true; return }
        $up = [System.Threading.EventWaitHandle]::new($false, [System.Threading.EventResetMode]::ManualReset, $upName)
        $alive = [System.Threading.Mutex]::new($false, $aliveName)
        # 拿到 $alive ⇒ 此刻没有 monitor 在跑：趁拿着把 $up 清掉再放手（下一个 monitor 要先拿到它才置位，清不掉它的）。
        # $block：一直等到拿到为止；否则只看一眼。
        $noMonitor = {
            param($block)
            $got = $false
            try {
                if ($block) { $got = $alive.WaitOne() } else { $got = $alive.WaitOne(0) }
            } catch [System.Threading.AbandonedMutexException] { $got = $true }
            if ($got) {
                [void]$up.Reset()
                $alive.ReleaseMutex()
            }
            $got
        }
        while (-not $state.done) {
            [void]$up.WaitOne()
            if ($state.done) { break }
            if (& $noMonitor $false) { continue }
            $r = 'again'
            $gate.Wait()
            try { $r = & $claim $dir $procId $procStart $false } catch {} finally { [void]$gate.Release() }
            if ($r -eq 'bound') { $state.done = $true; break }
            [void](& $noMonitor $true)
        }
    }

    # 读那两个全局量不走 $global:xxx：配置文件里开了 Set-StrictMode 时，读一个还没设过的变量会报错。
    $vars = $ExecutionContext.SessionState.PSVariable
    if ($Background) {
        # 同一个 PowerShell 里配置文件又加载了一次 ⇒ 后台那一份已经在了。
        if ($vars.GetValue('__ccm_gate')) { return }
        try {
            # 同一时刻只许一份认领在改窗口标题（后台这一份 ↔ 敲 cc 时那一份）。
            $global:__ccm_gate = [System.Threading.SemaphoreSlim]::new(1, 1)
            $global:__ccm_state = [hashtable]::Synchronized(@{ done = $false })
            $iss = [System.Management.Automation.Runspaces.InitialSessionState]::Create()
            $iss.LanguageMode = [System.Management.Automation.PSLanguageMode]::FullLanguage
            $rs = [System.Management.Automation.Runspaces.RunspaceFactory]::CreateRunspace($iss)
            $rs.Open()
            $ps = [System.Management.Automation.PowerShell]::Create()
            $ps.Runspace = $rs
            [void]$ps.AddScript($watch.ToString())
            foreach ($a in @($claim.ToString(), $global:__ccm_gate, $global:__ccm_state, $ccmDir, {{MONITOR_UP}}, {{MONITOR_ALIVE}})) {
                [void]$ps.AddArgument($a)
            }
            [void]$ps.BeginInvoke()
            $global:__ccm_watch = $ps
        } catch {}
        return
    }

    try {
        $procStart = [System.Diagnostics.Process]::GetCurrentProcess().StartTime.ToFileTime()
    } catch {
        Write-Warning "cc-monitor: get-process StartTime failed: $_"
        return
    }
    if ((& $claim $ccmDir $PID $procStart $true) -eq 'bound') { return }

    # 设置里勾了「用 cc 启动 claude 时自动打开 monitor」：monitor 没在跑就把它启动起来。
    $autoLaunchFile = Join-Path $ccmDir 'auto-launch.json'
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

    # 后台那一份正在认领 ⇒ 等它做完（同一时刻只许一份改窗口标题）；等不到就不另起一份。
    $gate = $vars.GetValue('__ccm_gate')
    $held = -not $gate
    if ($gate) { try { $held = $gate.Wait(4000) } catch {} }
    $r = 'none'
    if ($held) {
        try { $r = & $claim $ccmDir $PID $procStart $false } finally { if ($gate) { [void]$gate.Release() } }
    }
    if ($r -eq 'bound') {
        $state = $vars.GetValue('__ccm_state')
        if ($state) { $state.done = $true }
        return
    }
    if (-not (Test-Path (Join-Path $ccmDir "ps-registry\$PID.json"))) {
        Write-Warning "cc-monitor: 绑定超时 (monitor 没在跑？)"
    }
}
# 这个窗口的标签（进程号-起始时刻，与登记表同一个键）：经下面那层 ssh 送到远端，远端只回显，↗ 先按它找窗口。
try { $env:LC_CCM_WINDOW = '{0}-{1}' -f $PID, [System.Diagnostics.Process]::GetCurrentProcess().StartTime.ToFileTime() } catch {}
# ssh 包一层：带上窗口标签（远端 sshd 默认收 LC_* 变量；不收就只是没送到）。其余参数原样交给系统的 ssh。
function ssh {
    $exe = Get-Command ssh -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $exe) { Write-Error 'ssh: command not found'; return }
    & $exe.Source -o SendEnv=LC_CCM_WINDOW @args
}
# 每开一个 PowerShell 登记一次（不等、不出声）。
__ccm_bind -Background
# 接上 cc-monitor「别名」那一块写的别名文件（没生成过就什么都不做）。
if (Test-Path -LiteralPath "$HOME\.cc-monitor\aliases.ps1") { . "$HOME\.cc-monitor\aliases.ps1" }
# === cc-monitor END ===
