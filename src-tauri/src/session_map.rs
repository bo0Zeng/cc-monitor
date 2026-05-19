//! 活跃 session 探测 —— 不用 hook，直接读 Claude Code 自己维护的 `~/.claude/sessions/<PID>.json`。
//!
//! 每个 PID.json 含：`{pid, sessionId, cwd, startedAt, procStart, status, ...}`
//! - `procStart` 是 .NET DateTime.ToFileTime() 字符串（100ns 自 1601 UTC = Win32 FILETIME 整数）
//! - 跟 `GetProcessTimes` 返回的 FILETIME 直接 u64 等值比对（容差几毫秒）

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult};
use parking_lot::RwLock;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

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
        tracing::info!("session_map scanning {} (exists={})", dir.display(), dir.exists());
        let initial = scan_dir(&dir);
        tracing::info!("session_map loaded {} entries", initial.len());
        for (sid, info) in &initial {
            tracing::info!("  session: {} pid={} cwd={}", sid, info.pid, info.cwd);
        }
        let me = Arc::new(Self {
            dir: dir.clone(),
            by_id: Arc::new(RwLock::new(initial)),
        });
        Self::spawn_watcher(&me);
        me
    }

    fn spawn_watcher(this: &Arc<Self>) {
        let dir = this.dir.clone();
        let by_id = this.by_id.clone();
        std::thread::Builder::new()
            .name("session-map-watcher".into())
            .spawn(move || run_watcher(dir, by_id))
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
        let alive = is_process_alive(info.pid);
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

fn run_watcher(dir: PathBuf, by_id: Arc<RwLock<HashMap<String, SessionInfo>>>) {
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
        // 任何变动都全量重扫（数量很小，<100 个文件）
        let next = scan_dir(&dir);
        let n = next.len();
        *by_id.write() = next;
        tracing::debug!("session_map reloaded ({n} entries)");
    }
}

// === 进程探活（Windows） ===
//
// 只判断 PID 是否还活着（OpenProcess 成功 + GetExitCodeProcess == STILL_ACTIVE）。
// 不再校验 procStart：.NET 的 procStart 是 DateTime Ticks（自 0001-01-01）且可能携带本地时区，
// 跟 Win32 GetProcessTimes 的 FILETIME（自 1601-01-01 UTC）需要复杂的归一化才能比对，
// 而本应用是只读 monitor，PID 复用造成的代价仅是多显示一个 Tab，不值得增加这层复杂度。

#[cfg(windows)]
fn is_process_alive(pid: u32) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    const STILL_ACTIVE: u32 = 259;

    if pid == 0 {
        return false;
    }

    unsafe {
        let handle = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) if !h.is_invalid() => h,
            _ => return false,
        };
        let mut code: u32 = 0;
        let queried = GetExitCodeProcess(handle, &mut code).is_ok();
        let _ = CloseHandle(handle);
        queried && code == STILL_ACTIVE
    }
}

#[cfg(not(windows))]
fn is_process_alive(_pid: u32) -> bool {
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
