# Claude Code SessionStart hook
# 触发环境变量：$env:CLAUDE_SESSION_ID, $env:CLAUDE_PROJECT_DIR

$ErrorActionPreference = "SilentlyContinue"

# Claude 数据目录解析（与 paths.rs 一致）：CLAUDE_CONFIG_DIR > ~/.claude
if ($env:CLAUDE_CONFIG_DIR -and (Test-Path $env:CLAUDE_CONFIG_DIR)) {
    $claude_dir = $env:CLAUDE_CONFIG_DIR
} else {
    $claude_dir = Join-Path $env:USERPROFILE ".claude"
}

# 调试日志：每次 hook 被调用都写一行（不管后面成不成功）
$dbg_log = Join-Path $claude_dir "claudecode-frontend\hook-debug.log"
$dbg_dir = Split-Path $dbg_log -Parent
if (-not (Test-Path $dbg_dir)) { New-Item -ItemType Directory -Path $dbg_dir -Force | Out-Null }
$now = (Get-Date).ToString("o")
$env_dump = @(
    "session_id=$($env:CLAUDE_SESSION_ID)",
    "project_dir=$($env:CLAUDE_PROJECT_DIR)",
    "ps_version=$($PSVersionTable.PSVersion)",
    "pid=$PID",
    "ppid=$( (Get-CimInstance Win32_Process -Filter "ProcessId=$PID" -ErrorAction SilentlyContinue).ParentProcessId )"
) -join " | "
Add-Content -Path $dbg_log -Value "$now CALLED  $env_dump" -Encoding UTF8

$session_id = $env:CLAUDE_SESSION_ID
if (-not $session_id) {
    Add-Content -Path $dbg_log -Value "$now EXIT(no session_id)" -Encoding UTF8
    # 仍要回 {} 给 Claude Code，否则 hook 协议会卡
    Write-Output "{}"
    exit 0
}
$cwd = $env:CLAUDE_PROJECT_DIR

# === 进程树穿透：找第一个拥有可见顶层窗口的祖先 PID ===
# 不写死层数（mintty / Alacritty / WezTerm / tmux 嵌套 / cmd /c claude 等链长不一）

Add-Type @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class W {
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc p, IntPtr l);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    public delegate bool EnumWindowsProc(IntPtr h, IntPtr l);
    public static HashSet<uint> GetPidsWithTopLevelWindow() {
        var s = new HashSet<uint>();
        EnumWindows((h, _) => {
            if (IsWindowVisible(h)) { uint p; GetWindowThreadProcessId(h, out p); s.Add(p); }
            return true;
        }, IntPtr.Zero);
        return s;
    }
}
"@ -ErrorAction SilentlyContinue

function Get-ParentPid([int]$pid_in) {
    (Get-CimInstance Win32_Process -Filter "ProcessId=$pid_in" -ErrorAction SilentlyContinue).ParentProcessId
}

function Find-TerminalPid([int]$start_pid) {
    $windowed = [W]::GetPidsWithTopLevelWindow()
    $cur = $start_pid
    for ($i = 0; $i -lt 16; $i++) {
        $parent = Get-ParentPid $cur
        if (-not $parent -or $parent -eq 0) { break }
        if ($windowed.Contains([uint32]$parent)) { return $parent }
        $cur = $parent
    }
    return $null
}

$hook_pid   = $PID
$claude_pid = Get-ParentPid $hook_pid
$shell_pid  = Get-ParentPid $claude_pid
$term_pid   = Find-TerminalPid $hook_pid

$dir = Join-Path $claude_dir "claudecode-frontend"
if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
$map_path = Join-Path $dir "sessions.json"

$data = @{}
if (Test-Path $map_path) {
    try {
        $raw = Get-Content $map_path -Raw -Encoding UTF8
        if ($raw) { $data = $raw | ConvertFrom-Json -AsHashtable }
    } catch { $data = @{} }
}

$data[$session_id] = @{
    terminal_pid = $term_pid
    shell_pid    = $shell_pid
    cwd          = $cwd
    started_at   = (Get-Date).ToString("o")
}

$tmp = "$map_path.tmp"
$data | ConvertTo-Json -Depth 5 | Set-Content -Path $tmp -Encoding UTF8
Move-Item -Path $tmp -Destination $map_path -Force

Write-Output "{}"
