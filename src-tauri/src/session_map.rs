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

/// session 集合变化 —— 由 watcher 线程每次重扫后比对旧表得出。
/// 当前 lib.rs 只关心 removed（推 session-ended 事件），added 由 watcher 通过
/// jsonl-line 事件自然覆盖，无需独立通道。
#[derive(Debug, Clone)]
pub struct SessionChange {
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
    // status 字段（Claude Code 写入 "busy"/"shell"）当前 monitor 不消费；serde 默认
    // 忽略 JSON 里的额外字段，无需显式声明
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
    /// 加载 sessions/ 目录的全部活跃 session，并启动 watcher 线程。
    /// 返回一个 channel 接收 session 集合变化（lib.rs 用它推送 session-ended 事件给前端）。
    pub fn load_with_changes(dir: PathBuf) -> (Arc<Self>, mpsc::Receiver<SessionChange>) {
        tracing::info!(
            "session_map scanning {} (exists={})",
            dir.display(),
            dir.exists()
        );
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
        if let Err(e) = std::thread::Builder::new()
            .name("session-map-watcher".into())
            .spawn(move || run_watcher(dir, by_id, change_tx))
        {
            tracing::error!(
                "spawn session-map-watcher failed: {e}; \
                 active session list will stay frozen at initial scan"
            );
        }
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
        tracing::debug!("active? {session_id} pid={} alive={alive}", info.pid);
        alive
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
        let info = self
            .by_id
            .read()
            .get(session_id)
            .cloned()
            .ok_or_else(|| format!("session {session_id} not in map"))?;
        let snap = process_info_snapshot().ok_or_else(|| "snapshot failed".to_string())?;
        let windows_list = enumerate_top_level_windows();

        let ancestors = build_ancestors(info.pid, &snap);
        let search_terms = build_search_terms(&info.cwd, info.name.as_deref());

        let best = select_best_window(&windows_list, &snap, &ancestors, &search_terms);
        let (window, tier) = match best {
            Some(v) => v,
            None => {
                // 完全没命中 —— 打全部终端类窗口的 (pid, title) 给诊断
                let candidates: Vec<String> = windows_list
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
                    candidates.join(" | ")
                ));
            }
        };

        tracing::info!(
            "bring_to_front sid={session_id} terms={:?} tier={} hwnd={:?}",
            search_terms,
            tier.label(),
            window.hwnd
        );

        // tier D 兜底意味着按项目名找不到精确窗口；输出所有终端类窗口的 title
        // 给诊断（仅 D 时打，避免 A/B/C 命中时刷屏）
        if tier == MatchTier::TerminalAny {
            let candidates: Vec<String> = windows_list
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
                candidates.len(),
                candidates.join(" | ")
            );
        }

        activate_window(window.hwnd)
    }

    #[cfg(not(windows))]
    pub fn bring_terminal_to_front(&self, _session_id: &str) -> Result<(), String> {
        Err("only supported on Windows".into())
    }
}

// === WindowMatcher：bring_terminal_to_front 的纯逻辑部分 ===
//
// 把"找哪个窗口"从 Win32 API 调用里拆出来，单元测试可直接喂构造好的
// HashMap / Vec 验证 tier 决策，不需要真实 OpenProcess / EnumWindows。

/// 沿 ToolHelp 快照里的 parent 链向上走，收集所有祖先 PID（含起点）。
/// 上限 32 层防 cyclic / 异常进程表导致死循环。
fn build_ancestors(start_pid: u32, snap: &HashMap<u32, ProcInfo>) -> HashSet<u32> {
    let mut acc = HashSet::new();
    let mut cur = start_pid;
    for _ in 0..32 {
        if !acc.insert(cur) {
            break; // 已访问过，链有环
        }
        let Some(p) = snap.get(&cur) else { break };
        if p.parent == 0 || p.parent == cur {
            break;
        }
        cur = p.parent;
    }
    acc
}

/// 构造按优先级排序的 search term 列表（用于 WT 窗口 title 匹配）。
///
/// 优先级（从精确到模糊）：
///   1. ai-title 全字（Claude Code 实际写到 console title 的字符串）
///   2. ai-title 前 12 字符前缀（应对 WT title 长度截断）
///   3. cwd 最后一段（项目名）
///   4. 项目名前 8 字符前缀
///
/// 短于 4 字符的 term 跳过（避免"src" 这种短串误匹配很多窗口）。
fn build_search_terms(cwd: &str, ai_title: Option<&str>) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    let ai = ai_title.unwrap_or("").trim();
    if ai.len() >= 4 {
        terms.push(ai.to_string());
        let prefix: String = ai.chars().take(12).collect();
        if prefix.len() >= 4 && prefix != ai {
            terms.push(prefix);
        }
    }
    let project = Path::new(cwd)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if project.len() >= 4 {
        terms.push(project.to_string());
        let prefix: String = project.chars().take(8).collect();
        if prefix.len() >= 4 && prefix != project {
            terms.push(prefix);
        }
    }
    terms
}

/// 4 阶段窗口匹配优先级（从最精确到最宽松）。
#[cfg(windows)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum MatchTier {
    /// 祖先链 PID 的窗口 + title 含 search term —— 最精确
    AncestorWithTitle,
    /// 祖先链 PID 的窗口（任意 title）
    AncestorAny,
    /// 终端类进程窗口 + title 含 search term
    TerminalWithTitle,
    /// 终端类进程的任一窗口 —— 兜底，WT 多窗口时所有 session 落这里
    TerminalAny,
}

#[cfg(windows)]
impl MatchTier {
    fn label(self) -> &'static str {
        match self {
            Self::AncestorWithTitle => "A:ancestor+title",
            Self::AncestorAny => "B:ancestor",
            Self::TerminalWithTitle => "C:terminal+title",
            Self::TerminalAny => "D:terminal-any",
        }
    }
}

/// 给单个窗口分类。None 表示既不在祖先链也不是终端类（与本 session 无关）。
#[cfg(windows)]
fn classify_window(
    w: &WindowSnap,
    snap: &HashMap<u32, ProcInfo>,
    ancestors: &HashSet<u32>,
    search_terms: &[String],
) -> Option<MatchTier> {
    let proc_name = snap.get(&w.pid).map(|p| p.name.as_str()).unwrap_or("");
    let is_system = is_system_shell_process(proc_name);
    let in_ancestors = !is_system && ancestors.contains(&w.pid);
    let is_terminal = is_terminal_process(proc_name);
    let title_match = search_terms.iter().any(|t| w.title.contains(t));

    if in_ancestors && title_match {
        return Some(MatchTier::AncestorWithTitle);
    }
    if in_ancestors {
        return Some(MatchTier::AncestorAny);
    }
    if is_terminal && title_match {
        return Some(MatchTier::TerminalWithTitle);
    }
    if is_terminal {
        return Some(MatchTier::TerminalAny);
    }
    None
}

/// 从窗口列表里挑出"按 tier 优先级最高且最早出现"的那个。
///
/// Iterate 一次，分别记下 4 个 tier 的首个命中。返回时按 tier 优先级输出。
#[cfg(windows)]
fn select_best_window<'a>(
    windows: &'a [WindowSnap],
    snap: &HashMap<u32, ProcInfo>,
    ancestors: &HashSet<u32>,
    search_terms: &[String],
) -> Option<(&'a WindowSnap, MatchTier)> {
    let mut best: [Option<&'a WindowSnap>; 4] = [None; 4];
    for w in windows {
        let Some(tier) = classify_window(w, snap, ancestors, search_terms) else {
            continue;
        };
        let idx = tier as usize;
        if best[idx].is_none() {
            best[idx] = Some(w);
        }
    }
    for (idx, slot) in best.iter().enumerate() {
        if let Some(w) = slot {
            let tier = match idx {
                0 => MatchTier::AncestorWithTitle,
                1 => MatchTier::AncestorAny,
                2 => MatchTier::TerminalWithTitle,
                _ => MatchTier::TerminalAny,
            };
            return Some((w, tier));
        }
    }
    None
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
        CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32, TH32CS_SNAPPROCESS,
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
        let added_count = next_keys.difference(&prev_keys).count();
        *by_id.write() = next;
        if !removed.is_empty() || added_count > 0 {
            tracing::info!(
                "session_map: {n} active (+{added_count} -{})",
                removed.len()
            );
            if !removed.is_empty() {
                if let Some(tx) = &change_tx {
                    let _ = tx.send(SessionChange { removed });
                }
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
        fn FileTimeToLocalFileTime(
            lpFileTime: *const FILETIME,
            lpLocalFileTime: *mut FILETIME,
        ) -> i32;
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
        // 来自 Claude Code 实际写入的 sessions/<PID>.json 的最小代表样本；
        // status / startedAt 等 monitor 不消费的字段也带上，确认 serde 默认能
        // 忽略未声明的字段
        let raw = r#"{"pid":35776,"sessionId":"5b67f422-52a9-453c-bd64-3288a78a24a0","cwd":"D:\\x","startedAt":1779157297377,"procStart":"639147828963703970","status":"busy"}"#;
        let info: SessionInfo = serde_json::from_str(raw).unwrap();
        assert_eq!(info.pid, 35776);
        assert_eq!(info.session_id, "5b67f422-52a9-453c-bd64-3288a78a24a0");
        assert_eq!(info.proc_start, "639147828963703970");
        assert_eq!(info.name, None);
    }

    #[test]
    fn parse_session_info_minimal() {
        // 最小必需字段（无 status / name / startedAt 等）也能解析
        let raw = r#"{"pid":1,"sessionId":"s","cwd":"x","procStart":"100"}"#;
        let info: SessionInfo = serde_json::from_str(raw).unwrap();
        assert_eq!(info.session_id, "s");
        assert_eq!(info.name, None);
    }

    // === build_ancestors（跨平台纯函数）===

    fn proc(parent: u32, name: &str) -> ProcInfo {
        ProcInfo {
            parent,
            name: name.into(),
        }
    }

    #[test]
    fn ancestors_walks_parent_chain() {
        let mut snap = HashMap::new();
        snap.insert(100, proc(50, "claude.exe"));
        snap.insert(50, proc(20, "pwsh.exe"));
        snap.insert(20, proc(4, "WindowsTerminal.exe"));
        snap.insert(4, proc(0, "System")); // 走到根
        let acc = build_ancestors(100, &snap);
        assert!(acc.contains(&100));
        assert!(acc.contains(&50));
        assert!(acc.contains(&20));
        assert!(acc.contains(&4));
    }

    #[test]
    fn ancestors_breaks_on_cycle() {
        // 异常进程表：100 -> 50 -> 100（环）
        let mut snap = HashMap::new();
        snap.insert(100, proc(50, "a.exe"));
        snap.insert(50, proc(100, "b.exe"));
        let acc = build_ancestors(100, &snap);
        assert_eq!(acc.len(), 2); // 不死循环，最多 32 层但环检测先 break
    }

    #[test]
    fn ancestors_breaks_on_missing_parent() {
        // 走到一个 PID snap 里没记的（进程已退）
        let mut snap = HashMap::new();
        snap.insert(100, proc(99999, "a.exe"));
        let acc = build_ancestors(100, &snap);
        assert!(acc.contains(&100));
        assert!(acc.contains(&99999));
        assert_eq!(acc.len(), 2);
    }

    // === build_search_terms ===

    #[test]
    fn search_terms_full_ai_title_and_project() {
        let terms = build_search_terms(r"D:\Sync\文档\claudecode-frontend", Some("filter-active"));
        // ai-title 全字 + 前缀 + 项目名（短于 12 字 → 无前缀）；项目名 16 字 → 前 8
        assert!(terms.iter().any(|t| t == "filter-active"));
        assert!(terms.iter().any(|t| t == "claudecode-frontend"));
        // 项目名长 > 8 → 加前 8 字前缀
        assert!(terms.iter().any(|t| t == "claudeco"));
    }

    #[test]
    fn search_terms_skip_short_strings() {
        // ai-title 3 字符（< 4）跳过；项目名 3 字符跳过 → 空
        let terms = build_search_terms(r"D:\a\b", Some("hi"));
        assert!(terms.is_empty(), "expected empty, got {:?}", terms);
    }

    #[test]
    fn search_terms_ai_title_prefix_only_when_long() {
        // ai-title 13 字符 → 前 12 字前缀加入
        let terms = build_search_terms("/tmp", Some("abcdefghijklm")); // 13
        assert!(terms.iter().any(|t| t == "abcdefghijklm"));
        assert!(terms.iter().any(|t| t == "abcdefghijkl"));
    }

    // === classify_window + select_best_window（Win32 类型仅在 windows 编译）===
    #[cfg(windows)]
    mod matcher_win {
        use super::*;
        use windows::Win32::Foundation::HWND;

        fn ws(hwnd_raw: isize, pid: u32, title: &str) -> WindowSnap {
            WindowSnap {
                hwnd: HWND(hwnd_raw),
                pid,
                title: title.to_string(),
            }
        }

        fn build_snap(entries: &[(u32, u32, &str)]) -> HashMap<u32, ProcInfo> {
            let mut m = HashMap::new();
            for (pid, parent, name) in entries {
                m.insert(*pid, proc(*parent, name));
            }
            m
        }

        #[test]
        fn classify_ancestor_with_title() {
            let snap = build_snap(&[(100, 0, "claude.exe")]);
            let ancestors: HashSet<u32> = [100].into_iter().collect();
            let terms = vec!["proj".to_string()];
            let w = ws(1, 100, "myproject in foo");
            assert_eq!(
                classify_window(&w, &snap, &ancestors, &terms),
                Some(MatchTier::AncestorWithTitle)
            );
        }

        #[test]
        fn classify_ancestor_any() {
            let snap = build_snap(&[(100, 0, "claude.exe")]);
            let ancestors: HashSet<u32> = [100].into_iter().collect();
            let terms = vec!["proj".to_string()];
            let w = ws(1, 100, "irrelevant title");
            assert_eq!(
                classify_window(&w, &snap, &ancestors, &terms),
                Some(MatchTier::AncestorAny)
            );
        }

        #[test]
        fn classify_terminal_with_title() {
            let snap = build_snap(&[(200, 0, "WindowsTerminal.exe")]);
            let ancestors: HashSet<u32> = HashSet::new();
            let terms = vec!["foobar".to_string()];
            let w = ws(2, 200, "WT - foobar tab");
            assert_eq!(
                classify_window(&w, &snap, &ancestors, &terms),
                Some(MatchTier::TerminalWithTitle)
            );
        }

        #[test]
        fn classify_terminal_any() {
            let snap = build_snap(&[(200, 0, "WindowsTerminal.exe")]);
            let ancestors: HashSet<u32> = HashSet::new();
            let terms = vec!["foobar".to_string()];
            let w = ws(2, 200, "no match here");
            assert_eq!(
                classify_window(&w, &snap, &ancestors, &terms),
                Some(MatchTier::TerminalAny)
            );
        }

        #[test]
        fn classify_explorer_in_ancestors_not_matched_as_ancestor() {
            // explorer.exe 是系统 shell，即使在祖先链也不算 ancestor 命中
            let snap = build_snap(&[(50, 0, "explorer.exe")]);
            let ancestors: HashSet<u32> = [50].into_iter().collect();
            let terms = vec!["proj".to_string()];
            let w = ws(3, 50, "proj");
            assert_eq!(classify_window(&w, &snap, &ancestors, &terms), None);
        }

        #[test]
        fn classify_unrelated_returns_none() {
            let snap = build_snap(&[(300, 0, "code.exe")]);
            let ancestors: HashSet<u32> = HashSet::new();
            let terms: Vec<String> = vec![];
            let w = ws(4, 300, "vscode");
            assert_eq!(classify_window(&w, &snap, &ancestors, &terms), None);
        }

        #[test]
        fn select_best_picks_highest_tier() {
            let snap = build_snap(&[(100, 0, "claude.exe"), (200, 0, "WindowsTerminal.exe")]);
            let ancestors: HashSet<u32> = [100].into_iter().collect();
            let terms = vec!["proj".to_string()];
            let windows = vec![
                ws(1, 200, "WT - proj"),         // C: TerminalWithTitle
                ws(2, 100, "ancestor no title"), // B: AncestorAny
                ws(3, 200, "WT - random"),       // D: TerminalAny
                ws(4, 100, "ancestor proj"),     // A: AncestorWithTitle ← 应选这个
            ];
            let (best, tier) = select_best_window(&windows, &snap, &ancestors, &terms).unwrap();
            assert_eq!(tier, MatchTier::AncestorWithTitle);
            assert_eq!(best.title, "ancestor proj");
        }

        #[test]
        fn select_best_returns_none_when_no_match() {
            let snap = build_snap(&[(300, 0, "code.exe")]);
            let ancestors: HashSet<u32> = HashSet::new();
            let terms: Vec<String> = vec![];
            let windows = vec![ws(1, 300, "vscode")];
            assert!(select_best_window(&windows, &snap, &ancestors, &terms).is_none());
        }
    }
}
