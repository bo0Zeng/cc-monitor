//! 活跃 session 探测 —— 不用 hook，直接读 Claude Code 自己维护的 `~/.claude/sessions/<PID>.json`。
//!
//! 每个 PID.json 含：`{pid, sessionId, cwd, startedAt, procStart, status, ...}`
//! - `procStart` 是 .NET DateTime.ToFileTime() 字符串（100ns 自 1601 UTC = Win32 FILETIME 整数）
//! - 跟 `GetProcessTimes` 返回的 FILETIME 直接 u64 等值比对（容差几毫秒）

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult};
use parking_lot::RwLock;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

/// session 集合变化（added/removed）—— 由 watcher 线程每次重扫后比对旧表得出。
#[derive(Debug, Clone)]
pub struct SessionChange {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SessionInfo {
    pub pid: u32,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub cwd: String,
    /// .NET DateTime.ToFileTime() —— FILETIME 100ns 自 1601-01-01 UTC，**字符串** 形式
    #[serde(rename = "procStart")]
    pub proc_start: String,
    #[serde(default)]
    pub status: Option<String>,
    /// Claude 给会话起的语义名（aka ai-title）。Claude Code 同时把它设到 console
    /// title，所以 WindowsTerminal 窗口的 tab title 实际是这个值——bring_to_front
    /// 用它做 window title 匹配。
    #[serde(default)]
    pub name: Option<String>,
}

pub struct SessionMap {
    dir: PathBuf,
    /// session_id → SessionInfo
    by_id: Arc<RwLock<HashMap<String, SessionInfo>>>,
}

impl SessionMap {
    pub fn load(dir: PathBuf) -> Arc<Self> {
        let (me, _rx) = Self::load_with_changes(dir);
        me
    }

    /// 同 `load`，额外返回一个 channel 接收 session 集合变化。
    /// 用于 lib.rs 把 session-ended 事件透传给前端。
    pub fn load_with_changes(dir: PathBuf) -> (Arc<Self>, mpsc::Receiver<SessionChange>) {
        tracing::info!("session_map scanning {} (exists={})", dir.display(), dir.exists());
        let initial = scan_dir(&dir);
        tracing::info!("session_map loaded {} entries", initial.len());
        for (sid, info) in &initial {
            tracing::info!("  session: {} pid={} cwd={}", sid, info.pid, info.cwd);
        }
        let (tx, rx) = mpsc::channel::<SessionChange>();
        let me = Arc::new(Self {
            dir: dir.clone(),
            by_id: Arc::new(RwLock::new(initial)),
        });
        Self::spawn_watcher(&me, Some(tx));
        (me, rx)
    }

    fn spawn_watcher(this: &Arc<Self>, change_tx: Option<mpsc::Sender<SessionChange>>) {
        let dir = this.dir.clone();
        let by_id = this.by_id.clone();
        std::thread::Builder::new()
            .name("session-map-watcher".into())
            .spawn(move || run_watcher(dir, by_id, change_tx))
            .ok();
    }

    pub fn is_session_active(&self, session_id: &str) -> bool {
        let info = match self.by_id.read().get(session_id).cloned() {
            Some(i) => i,
            None => {
                tracing::debug!("active? {session_id} -> NOT in session map");
                return false;
            }
        };
        let alive = is_process_alive(info.pid, Some(&info.proc_start));
        tracing::debug!(
            "active? {session_id} pid={} alive={alive}",
            info.pid
        );
        alive
    }

    #[allow(dead_code)]
    pub fn get(&self, session_id: &str) -> Option<SessionInfo> {
        self.by_id.read().get(session_id).cloned()
    }

    /// 把指定 session 对应的终端窗口调到前台。
    ///
    /// 四级匹配策略（从精确到宽松）：
    ///  A. 祖先链 PID 的窗口 + title 含 session.cwd 项目名（最精确，能区分独
    ///     立 conhost 同名窗口）
    ///  B. 祖先链 PID 的窗口（任意 title）
    ///  C. 终端类进程的窗口 + title 含项目名（WT 多窗口时按项目名区分）
    ///  D. 终端类进程的任一窗口（兜底，WT 默认 title 不含项目名时退化为此）
    ///
    /// **WT 限制**：多个 WT 窗口共享同一 WT.exe PID，只有 hwnd 不同。若用户没
    /// 把 WT tab/window title 设成含 cwd / 项目名的字符串，A/C tier 永远匹配
    /// 不上，所有 session 的 ↗ 都会落到 D tier 的第一个 WT 窗口。要分到具体
    /// 窗口，用户需要在 PowerShell startup 里加：
    ///     `$Host.UI.RawUI.WindowTitle = Split-Path -Leaf $PWD`
    #[cfg(windows)]
    pub fn bring_terminal_to_front(&self, session_id: &str) -> Result<(), String> {
        use std::collections::HashSet;

        let info = self
            .by_id
            .read()
            .get(session_id)
            .cloned()
            .ok_or_else(|| format!("session {session_id} not in map"))?;
        let snap = process_info_snapshot().ok_or_else(|| "snapshot failed".to_string())?;
        let windows_list = enumerate_top_level_windows();

        // 祖先 PID 集合（跳过系统 shell）
        let mut ancestors: HashSet<u32> = HashSet::new();
        let mut cur = info.pid;
        for _ in 0..32 {
            ancestors.insert(cur);
            let Some(p) = snap.get(&cur) else { break };
            if p.parent == 0 || p.parent == cur {
                break;
            }
            cur = p.parent;
        }

        // 匹配 term 优先级：
        //   1) session.name (= ai-title)，Claude Code 实际写到 console title 的字符串
        //      —— 这是 WindowsTerminal tab title 的真实内容
        //   2) cwd 项目名（cwd 最后一段），fallback 给没 ai-title 的早期 session
        let project = info
            .cwd
            .rsplit(['\\', '/'])
            .find(|s| !s.is_empty())
            .unwrap_or("");
        let ai_title = info.name.as_deref().unwrap_or("").trim();
        // 候选 search terms（按优先级），过滤掉太短的（< 4 字符避免误匹配）
        let mut search_terms: Vec<String> = Vec::new();
        if ai_title.len() >= 4 {
            search_terms.push(ai_title.to_string());
            // ai-title 前 12 字符前缀，应对 WT title 截断长 title
            let prefix: String = ai_title.chars().take(12).collect();
            if prefix.len() >= 4 && prefix != ai_title {
                search_terms.push(prefix);
            }
        }
        if project.len() >= 4 {
            search_terms.push(project.to_string());
            let p: String = project.chars().take(8).collect();
            if p.len() >= 4 && p != project {
                search_terms.push(p);
            }
        }

        let mut tier_a: Option<windows::Win32::Foundation::HWND> = None;
        let mut tier_b: Option<windows::Win32::Foundation::HWND> = None;
        let mut tier_c: Option<windows::Win32::Foundation::HWND> = None;
        let mut tier_d: Option<windows::Win32::Foundation::HWND> = None;

        for w in &windows_list {
            let proc_name = snap.get(&w.pid).map(|p| p.name.as_str()).unwrap_or("");
            // 命中任一 search term 即视为 title 匹配
            let title_match = search_terms.iter().any(|term| w.title.contains(term));
            let in_ancestors = ancestors.contains(&w.pid);
            let is_terminal = is_terminal_process(proc_name);
            let is_system = is_system_shell_process(proc_name);

            if in_ancestors && !is_system {
                if title_match && tier_a.is_none() {
                    tier_a = Some(w.hwnd);
                } else if tier_b.is_none() {
                    tier_b = Some(w.hwnd);
                }
            }
            if is_terminal {
                if title_match && tier_c.is_none() {
                    tier_c = Some(w.hwnd);
                } else if tier_d.is_none() {
                    tier_d = Some(w.hwnd);
                }
            }
        }

        let (hwnd, tier) = if let Some(h) = tier_a {
            (h, "A:ancestor+title")
        } else if let Some(h) = tier_b {
            (h, "B:ancestor")
        } else if let Some(h) = tier_c {
            (h, "C:terminal+title")
        } else if let Some(h) = tier_d {
            (h, "D:terminal-any")
        } else {
            // 完全没命中 —— 打全部终端类窗口的 (pid, title) 给诊断用
            let terminal_windows: Vec<String> = windows_list
                .iter()
                .filter(|w| {
                    snap.get(&w.pid)
                        .map(|p| is_terminal_process(&p.name))
                        .unwrap_or(false)
                })
                .map(|w| format!("pid={} title={:?}", w.pid, w.title))
                .collect();
            return Err(format!(
                "no terminal window for session {session_id} (pid {}, search_terms={:?}); \
                 candidates: [{}]",
                info.pid,
                search_terms,
                terminal_windows.join(" | ")
            ));
        };

        tracing::info!(
            "bring_to_front sid={session_id} terms={:?} tier={tier} hwnd={:?}",
            search_terms,
            hwnd
        );

        // tier D 兜底意味着按项目名找不到精确窗口；输出所有终端类窗口的 title
        // 给诊断（仅 D 时打，避免 A/B/C 命中时刷屏）
        if tier == "D:terminal-any" {
            let terminal_windows: Vec<String> = windows_list
                .iter()
                .filter(|w| {
                    snap.get(&w.pid)
                        .map(|p| is_terminal_process(&p.name))
                        .unwrap_or(false)
                })
                .map(|w| format!("{:?}=\"{}\"", w.hwnd, w.title))
                .collect();
            tracing::info!(
                "  ↳ tier-D candidates ({} terminal windows): [{}]",
                terminal_windows.len(),
                terminal_windows.join(" | ")
            );
        }

        activate_window(hwnd)
    }

    #[cfg(not(windows))]
    pub fn bring_terminal_to_front(&self, _session_id: &str) -> Result<(), String> {
        Err("only supported on Windows".into())
    }
}

#[derive(Clone)]
struct ProcInfo {
    parent: u32,
    name: String,
}

/// 系统 shell / 显示 / 服务宿主进程——出现在 claude 祖先链上不代表"终端窗口"
/// （PowerShell 从开始菜单启动 parent 就是 explorer.exe）。
fn is_system_shell_process(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "explorer.exe"
            | "dwm.exe"
            | "services.exe"
            | "svchost.exe"
            | "wininit.exe"
            | "csrss.exe"
            | "smss.exe"
            | "lsass.exe"
            | "winlogon.exe"
            | "shellexperiencehost.exe"
            | "searchhost.exe"
            | "startmenuexperiencehost.exe"
    )
}

/// 已知终端类进程：fallback 阶段用来找"任意一个终端窗口"调到前台。
fn is_terminal_process(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "windowsterminal.exe"
            | "wt.exe"
            | "conhost.exe"
            | "openconsole.exe"
            | "cmd.exe"
            | "powershell.exe"
            | "pwsh.exe"
            | "mintty.exe"
            | "alacritty.exe"
            | "wezterm-gui.exe"
            | "tabby.exe"
    )
}

/// pid → (parent_pid, exe_name) 一次性 ToolHelp 快照。
#[cfg(windows)]
fn process_info_snapshot() -> Option<HashMap<u32, ProcInfo>> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32,
        TH32CS_SNAPPROCESS,
    };
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        if snap.is_invalid() {
            return None;
        }
        let mut entry = PROCESSENTRY32 {
            dwSize: std::mem::size_of::<PROCESSENTRY32>() as u32,
            ..Default::default()
        };
        let mut map = HashMap::new();
        if Process32First(snap, &mut entry).is_ok() {
            loop {
                let name_end = entry
                    .szExeFile
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(entry.szExeFile.len());
                let bytes: Vec<u8> = entry.szExeFile[..name_end]
                    .iter()
                    .map(|&b| b as u8)
                    .collect();
                let name = String::from_utf8_lossy(&bytes).into_owned();
                map.insert(
                    entry.th32ProcessID,
                    ProcInfo {
                        parent: entry.th32ParentProcessID,
                        name,
                    },
                );
                if Process32Next(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
        Some(map)
    }
}

/// 单个 top-level 窗口的快照信息。
#[cfg(windows)]
struct WindowSnap {
    hwnd: windows::Win32::Foundation::HWND,
    pid: u32,
    title: String,
}

/// 枚举所有可见 top-level（无 owner）窗口，连同它们的 PID 与 title。
#[cfg(windows)]
fn enumerate_top_level_windows() -> Vec<WindowSnap> {
    use std::cell::RefCell;
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
        IsWindowVisible, GW_OWNER,
    };

    thread_local! {
        static COLLECTOR: RefCell<Vec<WindowSnap>> = const { RefCell::new(Vec::new()) };
    }
    COLLECTOR.with(|c| c.borrow_mut().clear());

    unsafe extern "system" fn cb(hwnd: HWND, _lp: LPARAM) -> BOOL {
        if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return BOOL(1);
        }
        if unsafe { GetWindow(hwnd, GW_OWNER) }.0 != 0 {
            return BOOL(1);
        }
        let mut pid: u32 = 0;
        let _ = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid == 0 {
            return BOOL(1);
        }
        let title = unsafe {
            let len = GetWindowTextLengthW(hwnd);
            if len <= 0 {
                String::new()
            } else {
                let mut buf = vec![0u16; (len + 1) as usize];
                let n = GetWindowTextW(hwnd, &mut buf);
                String::from_utf16_lossy(&buf[..n as usize])
            }
        };
        COLLECTOR.with(|c| {
            c.borrow_mut().push(WindowSnap { hwnd, pid, title });
        });
        BOOL(1)
    }

    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(0));
    }
    COLLECTOR.with(|c| std::mem::take(&mut *c.borrow_mut()))
}

#[cfg(windows)]
fn activate_window(hwnd: windows::Win32::Foundation::HWND) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::{
        IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        if SetForegroundWindow(hwnd).as_bool() {
            Ok(())
        } else {
            // SetForegroundWindow 偶尔被 OS 拒（用户没近期交互），不视为致命
            Err("SetForegroundWindow refused (window may have flashed in taskbar)".into())
        }
    }
}

fn scan_dir(dir: &Path) -> HashMap<String, SessionInfo> {
    let mut out = HashMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().map_or(false, |e| e == "json") {
            if let Some(info) = read_one(&p) {
                out.insert(info.session_id.clone(), info);
            }
        }
    }
    out
}

fn read_one(path: &Path) -> Option<SessionInfo> {
    let s = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&s).ok()
}

fn run_watcher(
    dir: PathBuf,
    by_id: Arc<RwLock<HashMap<String, SessionInfo>>>,
    change_tx: Option<mpsc::Sender<SessionChange>>,
) {
    if !dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::warn!("create {} failed: {e}", dir.display());
            return;
        }
    }

    let (tx, rx) = std::sync::mpsc::channel::<DebounceEventResult>();
    let mut debouncer = match new_debouncer(Duration::from_millis(80), tx) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("session_map debouncer init failed: {e}");
            return;
        }
    };

    if let Err(e) = debouncer.watcher().watch(&dir, RecursiveMode::NonRecursive) {
        tracing::error!("watch failed for {}: {e}", dir.display());
        return;
    }

    while let Ok(_evt) = rx.recv() {
        let next = scan_dir(&dir);
        let n = next.len();
        let next_keys: HashSet<String> = next.keys().cloned().collect();
        let prev_keys: HashSet<String> = by_id.read().keys().cloned().collect();
        let removed: Vec<String> = prev_keys.difference(&next_keys).cloned().collect();
        let added: Vec<String> = next_keys.difference(&prev_keys).cloned().collect();
        *by_id.write() = next;
        if !removed.is_empty() || !added.is_empty() {
            tracing::info!(
                "session_map: {n} active (+{} -{})",
                added.len(),
                removed.len()
            );
            if let Some(tx) = &change_tx {
                let _ = tx.send(SessionChange { added, removed });
            }
        }
    }
}

// === 进程探活（Windows） ===
//
// 两道关卡：
//   1) OpenProcess + GetExitCodeProcess == STILL_ACTIVE：PID 当前被占用着
//   2) GetProcessTimes 返回的 creation FILETIME（u64）与 sessions/<PID>.json 里
//      记录的 procStart 字符串（.NET DateTime.ToFileTime()，与 Win32 FILETIME 同零点同单位）
//      在 100ms 容差内吻合
//
// 没有第二关，残留的死 session PID.json 会因为 PID 被另一个进程复用而被误判为活跃
// （Windows PID 短期内复用很常见）。早期注释里"代价仅是多显示一个 Tab"的判断不成立，
// 用户实际看到的是 4 个僵尸 Tab。

#[cfg(windows)]
fn is_process_alive(pid: u32, expected_proc_start: Option<&str>) -> bool {
    use windows::Win32::Foundation::{CloseHandle, FILETIME};
    use windows::Win32::System::Threading::{
        GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    const STILL_ACTIVE: u32 = 259;
    /// 100ms = 1,000,000 个 100ns tick
    const PROC_START_TOLERANCE_TICKS: u64 = 1_000_000;

    if pid == 0 {
        return false;
    }

    unsafe {
        let handle = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) if !h.is_invalid() => h,
            _ => return false,
        };

        // 1) STILL_ACTIVE
        let mut code: u32 = 0;
        if GetExitCodeProcess(handle, &mut code).is_err() || code != STILL_ACTIVE {
            let _ = CloseHandle(handle);
            return false;
        }

        // 2) procStart 校验 —— 防 PID 复用
        //
        // 单位换算（实测在 UTC+8 host 上 diff = 504_911_519_999_999_999 = 1601 offset
        // + 8h 时区 + 1 tick 抖动验证得到的公式）：
        //
        //   Claude Code 写的 procStart  = .NET DateTime.Now.Ticks
        //                               = 自 0001-01-01 本地时间起算的 100ns
        //   GetProcessTimes 给的 FILETIME = 自 1601-01-01 UTC 起算的 100ns
        //
        // 转换：FILETIME (UTC) → 本地 FILETIME → +1601 偏移 → 与 expected 比对
        if let Some(expected_str) = expected_proc_start {
            if let Ok(expected) = expected_str.parse::<u64>() {
                let mut creation = FILETIME::default();
                let mut exit_t = FILETIME::default();
                let mut kernel = FILETIME::default();
                let mut user = FILETIME::default();
                if GetProcessTimes(handle, &mut creation, &mut exit_t, &mut kernel, &mut user)
                    .is_ok()
                {
                    let actual_net = filetime_to_net_local_ticks(&creation);
                    let diff = actual_net.abs_diff(expected);
                    if diff > PROC_START_TOLERANCE_TICKS {
                        tracing::debug!(
                            "pid {pid} proc_start mismatch: expected={expected} \
                             actual_net={actual_net} diff={diff} — PID reused"
                        );
                        let _ = CloseHandle(handle);
                        return false;
                    }
                }
            }
        }

        let _ = CloseHandle(handle);
        true
    }
}

/// Win32 FILETIME (UTC, 自 1601-01-01) → .NET DateTime.Now.Ticks (Local, 自 0001-01-01)。
/// Claude Code 写入 sessions/<PID>.json 的 procStart 字段就是 .NET Ticks 形式。
///
/// `FileTimeToLocalFileTime` 在 windows-rs 0.56 没有方便的封装路径，直接 raw FFI
/// 调 kernel32.dll；windows::Win32::Foundation::FILETIME 是 `#[repr(C)]` 与 Win32
/// 原生类型二进制兼容。
#[cfg(windows)]
fn filetime_to_net_local_ticks(utc: &windows::Win32::Foundation::FILETIME) -> u64 {
    use windows::Win32::Foundation::FILETIME;
    /// 从 .NET 0001-01-01 起到 Win32 1601-01-01 之间的 100ns 数。
    const NET_EPOCH_TO_WIN32_FILETIME_TICKS: u64 = 504_911_232_000_000_000;

    #[link(name = "kernel32")]
    extern "system" {
        fn FileTimeToLocalFileTime(lpFileTime: *const FILETIME, lpLocalFileTime: *mut FILETIME)
            -> i32;
    }

    let mut local = FILETIME::default();
    let local_ticks = unsafe {
        if FileTimeToLocalFileTime(utc as *const _, &mut local as *mut _) == 0 {
            // 转 local 失败时退回 utc，避免错误拒绝（最坏 case 是时区偏移落出容差被判不匹配）
            ((utc.dwHighDateTime as u64) << 32) | (utc.dwLowDateTime as u64)
        } else {
            ((local.dwHighDateTime as u64) << 32) | (local.dwLowDateTime as u64)
        }
    };
    local_ticks + NET_EPOCH_TO_WIN32_FILETIME_TICKS
}

#[cfg(not(windows))]
fn is_process_alive(_pid: u32, _expected_proc_start: Option<&str>) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_session_info() {
        let raw = r#"{"pid":35776,"sessionId":"5b67f422-52a9-453c-bd64-3288a78a24a0","cwd":"D:\\x","startedAt":1779157297377,"procStart":"639147828963703970","status":"busy"}"#;
        let info: SessionInfo = serde_json::from_str(raw).unwrap();
        assert_eq!(info.pid, 35776);
        assert_eq!(info.session_id, "5b67f422-52a9-453c-bd64-3288a78a24a0");
        assert_eq!(info.proc_start, "639147828963703970");
        assert_eq!(info.status.as_deref(), Some("busy"));
    }

    #[test]
    fn parse_session_info_no_status() {
        let raw = r#"{"pid":1,"sessionId":"s","cwd":"x","procStart":"100"}"#;
        let info: SessionInfo = serde_json::from_str(raw).unwrap();
        assert_eq!(info.status, None);
    }
}
