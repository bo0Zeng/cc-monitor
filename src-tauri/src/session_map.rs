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

    /// 焦点窗口 PID → session_id。
    ///
    /// session_map 存的是 claude CLI 的 PID（在进程树最深一层），而前台窗口的 PID
    /// 是终端（WindowsTerminal / ConHost 等），通常是 claude 的祖先。
    /// 所以从每个 session.pid 出发沿 parent 链向上 walk，匹配到 fg_pid 即命中。
    ///
    /// 取一次进程快照后在内存里 walk，避免对每个 session 重复 syscall。
    pub fn lookup_by_foreground_pid(&self, fg_pid: u32) -> Option<String> {
        if fg_pid == 0 {
            return None;
        }
        let parents = parent_map()?;
        let sessions = self.by_id.read();
        for (sid, info) in sessions.iter() {
            let mut cur = info.pid;
            // 最多 walk 32 层，防御循环或异常深的进程树
            for _ in 0..32 {
                if cur == fg_pid {
                    return Some(sid.clone());
                }
                let Some(&parent) = parents.get(&cur) else { break };
                if parent == 0 || parent == cur {
                    break;
                }
                cur = parent;
            }
        }
        None
    }
}

/// 当前所有进程的 pid → parent_pid 映射。一次 ToolHelp 快照。
#[cfg(windows)]
fn parent_map() -> Option<HashMap<u32, u32>> {
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
                map.insert(entry.th32ProcessID, entry.th32ParentProcessID);
                if Process32Next(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
        Some(map)
    }
}

#[cfg(not(windows))]
fn parent_map() -> Option<HashMap<u32, u32>> {
    None
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

        // 2) procStart 校验（如果有期望值）
        if let Some(expected_str) = expected_proc_start {
            let expected: Option<u64> = expected_str.parse().ok();
            let mut creation = FILETIME::default();
            let mut exit_t = FILETIME::default();
            let mut kernel = FILETIME::default();
            let mut user = FILETIME::default();
            let got_times =
                GetProcessTimes(handle, &mut creation, &mut exit_t, &mut kernel, &mut user)
                    .is_ok();
            if let (true, Some(exp)) = (got_times, expected) {
                let actual =
                    ((creation.dwHighDateTime as u64) << 32) | (creation.dwLowDateTime as u64);
                let diff = actual.abs_diff(exp);
                if diff > PROC_START_TOLERANCE_TICKS {
                    tracing::debug!(
                        "pid {pid} proc_start mismatch: expected={exp} actual={actual} diff={diff} — PID reused"
                    );
                    let _ = CloseHandle(handle);
                    return false;
                }
            }
        }

        let _ = CloseHandle(handle);
        true
    }
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
