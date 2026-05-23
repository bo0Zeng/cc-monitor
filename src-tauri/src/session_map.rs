//! 活跃 session 探测 —— 不用 hook，直接读 Claude Code 自己维护的 `~/.claude/sessions/<PID>.json`。
//!
//! 每个 PID.json 含：`{pid, sessionId, cwd, startedAt, procStart, status, ...}`
//! - `procStart` 是 .NET DateTime.ToFileTime() 字符串（100ns 自 1601 UTC = Win32 FILETIME 整数）
//! - 跟 `GetProcessTimes` 返回的 FILETIME 直接 u64 等值比对（容差几毫秒）
//!
//! **v1.6.7 撤回了 bring_terminal_to_front 整条链路**（4-tier WindowMatcher /
//! WT title 匹配 / SetForegroundWindow 等）。在 explorer 启 PowerShell + WT
//! DefTerm 接管 console 的常见架构下，claude 进程的祖先链与 WT 窗口完全脱节，
//! 无法可靠定位"哪个 WT 窗口跑了这个 session"。新方案以 `cc` 命令注入式绑定
//! 实现，待 v1.7 引入。

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult};
use parking_lot::RwLock;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

/// session 集合变化 —— 由 watcher 线程每次重扫 / 心跳后比对旧表得出。
///
/// - `removed`：sessions/<PID>.json 被删 / 心跳探活失败 → lib.rs 推 session-ended 事件
/// - `added`：sessions/<PID>.json 新增 → lib.rs 触发 jsonl-watcher 强制重扫该 session
///   的 jsonl（修 Bug：若 jsonl 行先于 PID.json 到达，active() 被拒后 process_file
///   early return 但不更新 offset，且不会再被自动重扫——导致 /resume 起的新 session
///   在某些竞态下永远不出现 Tab。这里加 added → rescan 通道作为安全网）
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
    // status 字段（Claude Code 写入 "busy"/"shell"）当前 monitor 不消费；serde 默认
    // 忽略 JSON 里的额外字段，无需显式声明
    /// Claude 给会话起的语义名（aka ai-title）。保留字段以备未来 v1.7 注入式绑定使用。
    #[serde(default)]
    #[allow(dead_code)]
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

    // 双触发：文件事件（即时） + 心跳（每 2s）。心跳是为修 Bug —— 用户关闭终端
    // 窗口导致 claude.exe 被强杀时，sessions/<PID>.json **不会被删**（Claude Code 的
    // 退出 hook 没跑）→ 文件事件永不触发 → 死 session 的 Tab 永远 live。心跳主动调
    // is_process_alive 清理这种残留。
    use std::sync::mpsc::RecvTimeoutError;
    loop {
        let evt = rx.recv_timeout(Duration::from_secs(2));
        let scan = match evt {
            Ok(_) => true,                           // 文件事件 → 全量重扫
            Err(RecvTimeoutError::Timeout) => false, // 心跳 → 只探活
            Err(RecvTimeoutError::Disconnected) => break,
        };

        if scan {
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
        } else {
            // 心跳：探活所有当前条目。死的算 removed（PID.json 还在但进程没了）。
            let snapshot: Vec<(String, SessionInfo)> = by_id
                .read()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let dead: Vec<String> = snapshot
                .into_iter()
                .filter(|(_, info)| !is_process_alive(info.pid, Some(&info.proc_start)))
                .map(|(sid, _)| sid)
                .collect();
            if !dead.is_empty() {
                {
                    let mut w = by_id.write();
                    for sid in &dead {
                        w.remove(sid);
                    }
                }
                tracing::info!(
                    "session_map heartbeat: {} dead session(s) removed: {:?}",
                    dead.len(),
                    dead
                );
                if let Some(tx) = &change_tx {
                    let _ = tx.send(SessionChange {
                        added: vec![],
                        removed: dead,
                    });
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
}
