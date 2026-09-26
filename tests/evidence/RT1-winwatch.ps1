# RT1 · 窗口哨兵：每 150 ms 枚举一次 session 1 的可见顶层窗口，新出现 / 消失的逐条落 logs\winwatch.log。
# 守的要求：`设计/00 §1.5.2`「三条策略」里 `ConsolePolicy::Hidden` —— 起子进程不许闪黑框；
# 一闪而过的控制台窗口截图抓不到，只有连续枚举抓得到。由 conhost --headless 起（它自己不开窗）。
. "C:\Users\user\AppData\Local\Temp\rt1\rt1lib.ps1"
$log = "$RT1\logs\winwatch.log"
$seen = @{}
"{0} START pid={1}" -f [DateTimeOffset]::Now.ToUnixTimeMilliseconds(), $PID | Out-File -Append -Encoding utf8 $log
while (-not (Test-Path "$RT1\logs\winwatch.stop")) {
  $now = @{}
  foreach ($w in [Rt1W]::Top()) {
    if ($w.W -le 0) { continue }
    $now[$w.H] = $w
    if (-not $seen.ContainsKey($w.H)) {
      $p = Get-Process -Id $w.Pid -ErrorAction SilentlyContinue
      "{0} NEW h={1} pid={2} proc={3} cls={4} title={5} size={6}x{7}" -f [DateTimeOffset]::Now.ToUnixTimeMilliseconds(), $w.H, $w.Pid, $(if($p){$p.ProcessName}else{'?'}), $w.Cls, $w.Title, $w.W, $w.Ht | Out-File -Append -Encoding utf8 $log
    }
  }
  foreach ($h in @($seen.Keys)) { if (-not $now.ContainsKey($h)) { "{0} GONE h={1} cls={2} title={3}" -f [DateTimeOffset]::Now.ToUnixTimeMilliseconds(), $h, $seen[$h].Cls, $seen[$h].Title | Out-File -Append -Encoding utf8 $log } }
  $seen = $now
  Start-Sleep -Milliseconds 150
}
"{0} STOP" -f [DateTimeOffset]::Now.ToUnixTimeMilliseconds() | Out-File -Append -Encoding utf8 $log
