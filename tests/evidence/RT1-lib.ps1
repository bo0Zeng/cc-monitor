# RT1 · 虚拟机侧的读数小工具（由 RT1-vm.py 拷进 rt1\ 后在 job 里 dot-source）。
# 守的要求：用户裁决 V115（虚拟机当真机测试资源；session 1 里跑真窗口）。只读：枚举窗口、读 DPI 感知、
# 给窗口发 WM_CLOSE（关我们自己起的那个终端窗口）—— 不改系统任何设置。
$ErrorActionPreference = 'Continue'
$global:RT1 = "C:\Users\zbl\AppData\Local\Temp\rt1"
if (-not ('Rt1W' -as [type])) {
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class Rt1W {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern IntPtr SendMessageTimeout(IntPtr h, uint m, IntPtr w, IntPtr l, uint f, uint t, out IntPtr r);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern IntPtr GetWindowDpiAwarenessContext(IntPtr h);
  [DllImport("user32.dll")] public static extern int GetAwarenessFromDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool AreDpiAwarenessContextsEqual(IntPtr a, IntPtr b);
  [DllImport("kernel32.dll")] public static extern bool AttachConsole(uint pid);
  [DllImport("kernel32.dll")] public static extern bool FreeConsole();
  [DllImport("kernel32.dll")] public static extern bool SetConsoleCtrlHandler(IntPtr h, bool add);
  [DllImport("kernel32.dll")] public static extern bool GenerateConsoleCtrlEvent(uint ev, uint grp);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  public class Win { public long H; public uint Pid; public string Title; public string Cls; public int W; public int Ht; public int Aw; public uint Dpi; public bool PmV2; }
  public static List<Win> Top() {
    var res = new List<Win>();
    EnumWindows((h, l) => {
      if (!IsWindowVisible(h)) return true;
      uint pid; GetWindowThreadProcessId(h, out pid);
      var t = new StringBuilder(512); GetWindowText(h, t, 512);
      var c = new StringBuilder(256); GetClassName(h, c, 256);
      RECT r; GetWindowRect(h, out r);
      var ctx = GetWindowDpiAwarenessContext(h);
      res.Add(new Win { H = h.ToInt64(), Pid = pid, Title = t.ToString(), Cls = c.ToString(), W = r.R - r.L, Ht = r.B - r.T,
        Aw = GetAwarenessFromDpiAwarenessContext(ctx), Dpi = GetDpiForWindow(h),
        PmV2 = AreDpiAwarenessContextsEqual(ctx, new IntPtr(-4)) });
      return true;
    }, IntPtr.Zero);
    return res;
  }
  public static bool Close(long h) { return PostMessage(new IntPtr(h), 0x0010, IntPtr.Zero, IntPtr.Zero); }
  // 往 pid 所在的控制台发 CTRL_C（自己先脱离原控制台、忽略这一下）。回 true = 发出去了。
  public static bool CtrlC(uint pid) { return Ctrl(pid, 0); }
  public static bool Ctrl(uint pid, uint ev) {
    FreeConsole();
    if (!AttachConsole(pid)) return false;
    SetConsoleCtrlHandler(IntPtr.Zero, true);
    bool ok = GenerateConsoleCtrlEvent(ev, 0);
    System.Threading.Thread.Sleep(500);
    FreeConsole();
    return ok;
  }
}
'@
}
function Rt1-Windows {
  [Rt1W]::Top() | % {
    $p = Get-Process -Id $_.Pid -ErrorAction SilentlyContinue
    [pscustomobject]@{ H=$_.H; Pid=$_.Pid; Proc=$(if($p){$p.ProcessName}else{'?'}); Cls=$_.Cls; Title=$_.Title; W=$_.W; H2=$_.Ht; Awareness=$_.Aw; Dpi=$_.Dpi; PerMonV2=$_.PmV2 }
  }
}
function Rt1-Tree([int]$root) {
  $all = Get-CimInstance Win32_Process
  $kids = @{}; $all | % { $kids[[int]$_.ParentProcessId] += ,$_ }
  $out = @(); $q = New-Object System.Collections.Queue; $q.Enqueue($root)
  while ($q.Count) { $p = $q.Dequeue(); foreach ($c in $kids[$p]) { if ($c.ProcessId -ne $p) { $out += $c; $q.Enqueue([int]$c.ProcessId) } } }
  $out | select ProcessId,ParentProcessId,Name,SessionId,CreationDate,CommandLine
}
function Rt1-Env {
  $env:CCM_DATA_DIR = "$RT1\data"
  $env:CLAUDE_CONFIG_DIR = "$RT1\claude"
  $env:WEBVIEW2_USER_DATA_FOLDER = "$RT1\wv2"
  $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"
  $env:RT1_LOG_DIR = "$RT1\logs"
  if ($env:PATH -notlike "$RT1\bin;*") { $env:PATH = "$RT1\bin;" + $env:PATH }
}
function Rt1-StartMonitor([hashtable]$extra = @{}) {
  Rt1-Env
  foreach ($k in $extra.Keys) { Set-Item "env:$k" $extra[$k] }
  $p = Start-Process "$RT1\app\cc-monitor.exe" -WorkingDirectory "$RT1\work" -PassThru
  $p.Id | Set-Content "$RT1\logs\monitor.pid"
  "monitor pid=$($p.Id) start=$($p.StartTime.ToString('o'))"
  return
}
function Rt1-Mine {
  # 我起的进程：路径在 rt1\ 下，或 ~\.cc-monitor\bin\ 下（monitor 自释放的后端 / 全景 / ccm），或 --webview-exe-name=cc-monitor.exe 且 user-data-dir 在 rt1\wv2
  Get-CimInstance Win32_Process | ? { $_.ExecutablePath -like "$RT1\*" -or $_.ExecutablePath -like 'C:\Users\zbl\.cc-monitor\*' -or ($_.CommandLine -like '*--webview-exe-name=cc-monitor.exe*' -and $_.CommandLine -like "*rt1\wv2*") } |
    select ProcessId,ParentProcessId,Name,SessionId,CreationDate
}
function Rt1-Enc([string]$s) { [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($s)) }
# 像用户那样开一个终端窗口跑 claude（替身）：wt.exe 新窗口里 powershell -NoExit。环境显式写进命令
# （Windows Terminal 新标签的环境未必继承 wt.exe 的进程环境）。回新出现的顶层窗口。
function Rt1-OpenClaudeTerminal([string]$extra = '') {
  $before = Rt1-Windows | % { $_.H }
  $cmd = "`$env:CLAUDE_CONFIG_DIR='$RT1\claude'; `$env:RT1_LOG_DIR='$RT1\logs'; $extra & '$RT1\bin\claude.exe'"
  Start-Process wt.exe -ArgumentList @('-w', 'new', '-d', "$RT1\work", 'powershell.exe', '-NoExit', '-EncodedCommand', (Rt1-Enc $cmd))
  Start-Sleep 6
  Rt1-Windows | ? { $before -notcontains $_.H -and $_.W -gt 0 }
}
function Rt1-Claudes { Get-CimInstance Win32_Process -Filter "Name='claude.exe'" | select ProcessId,ParentProcessId,SessionId,CreationDate,CommandLine }
