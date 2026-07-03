//! 活跃 session 探测 —— 不用 hook，直接读 Claude Code 自己维护的 `~/.claude/sessions/<PID>.json`。
//!
//! 每个 PID.json 含：`{pid, sessionId, cwd, startedAt, procStart, status, ...}`
//! - `procStart` 是 **.NET DateTime.Ticks 字符串**（100ns 自 0001-01-01 **Local**，
//!   非 Win32 FILETIME UTC——比较时要用 `FileTime::to_net_local_ticks` 转换。
//!   详 `utils::NetTicks` / `FileTime` 模块文档）
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
use std::collections::HashMap;
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
    /// issue #23: 红绿灯——本次重扫中 status/waitingFor 发生变化（含新出现）的会话。
    /// lib.rs 据此 emit session-activity（变化才发，天然稀疏：CLI 仅在状态转换时
    /// 重写 sessions/<PID>.json）。
    pub status_changed: Vec<SessionActivity>,
}

/// issue #23: 单个会话的红绿灯状态快照（status 直接来自 Claude Code 官方字段）。
#[derive(Debug, Clone, PartialEq)]
pub struct SessionActivity {
    pub session_id: String,
    /// "busy"（运行中）/ "idle"/"shell"（等输入）/ "waiting"（等弹窗决定）。
    /// None = 旧版 CC 没写该字段。
    pub status: Option<String>,
    /// status=="waiting" 时的细分（"permission prompt" / "dialog open" / "input needed"…）
    pub waiting_for: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SessionInfo {
    pub pid: u32,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub cwd: String,
    /// .NET DateTime.ToFileTime() —— FILETIME 100ns 自 1601-01-01 UTC，**字符串** 形式
    ///
    /// v2.4.2 issue: 实测 Claude Code 某些启动路径（特定 /resume 流？）写出的
    /// `sessions/<PID>.json` 不含 `procStart` 字段。之前 `String` 必填导致 serde
    /// 解析失败 → 整个 session 被忽略 → monitor 漏 Tab。改 Option：缺失时
    /// `is_process_alive` 跳过 PID 复用校验仅看 STILL_ACTIVE（代价是 PID 短期
    /// 复用极小概率误判活跃，但比 "session 完全不出现" 强）。
    #[serde(rename = "procStart", default)]
    pub proc_start: Option<String>,
    /// issue #23: Claude Code 官方会话状态（"busy"/"idle"/"waiting"/"shell"），
    /// CLI **仅在状态转换时**重写本文件（实测与 jsonl turn_duration 同步，差 ~24ms）。
    /// 红绿灯主信号。Option 兜旧版 CC 无此字段。
    #[serde(default)]
    pub status: Option<String>,
    /// issue #23: status=="waiting" 时的细分原因（"permission prompt" / "dialog open"
    /// / "input needed" / "worker request" / "sandbox request"）。
    #[serde(rename = "waitingFor", default)]
    pub waiting_for: Option<String>,
    /// Claude 给会话起的语义名（aka ai-title）。保留字段以备未来 v1.7 注入式绑定使用。
    #[serde(default)]
    #[allow(dead_code)]
    pub name: Option<String>,
    /// Batch6-F21：会话类型。CC 2.1.x 起 daemon 后台任务（--fork-session）也写
    /// pidfile，标 `kind:"bg"`（另带 jobId）；交互会话为 `"interactive"`。
    /// Option 兜旧版 CC 无此字段（缺失视为交互，保守放行）。
    #[serde(default)]
    pub kind: Option<String>,
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
        let alive = is_process_alive(info.pid, info.proc_start.as_deref());
        tracing::debug!("active? {session_id} pid={} alive={alive}", info.pid);
        alive
    }

    /// 查 SessionInfo（v1.7 绑定逻辑用：拿 claude_pid → parent → BindRegistry）
    pub fn lookup(&self, session_id: &str) -> Option<SessionInfo> {
        self.by_id.read().get(session_id).cloned()
    }

    /// issue #23: 当前全部活跃会话的红绿灯快照。前端启动/F5 后拉一次做初始收敛
    /// （session-activity 事件不进 replay buffer，刷新会丢——同 get_session_tasks
    /// 的「快照 + 事件增量」双路收敛模式）。
    pub fn snapshot_activity(&self) -> Vec<SessionActivity> {
        self.by_id
            .read()
            .iter()
            .map(|(sid, info)| SessionActivity {
                session_id: sid.clone(),
                status: info.status.clone(),
                waiting_for: info.waiting_for.clone(),
            })
            .collect()
    }

    /// Batch5-F18：活跃会话清单（sid + cwd），供 `list_active_sessions` IPC——
    /// 前端启动时先建全部骨架 Tab，不等首条内容行。
    ///
    /// 按 (cwd, sid) 排序：HashMap 迭代序每进程随机，不排序则 tab 栏顺序每次
    /// 启动洗牌（F18 审计发现）。cwd 优先 → 同项目的会话相邻，跨启动稳定。
    pub fn snapshot_active(&self) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = self
            .by_id
            .read()
            .iter()
            .map(|(sid, info)| (sid.clone(), info.cwd.clone()))
            .collect();
        v.sort_by(|a, b| (&a.1, &a.0).cmp(&(&b.1, &b.0)));
        v
    }
}

fn scan_dir(dir: &Path) -> HashMap<String, SessionInfo> {
    // P3 归并：走 utils::scan_dir_jsons。
    let mut map = crate::utils::scan_dir_jsons(dir, |info: &SessionInfo| info.session_id.clone());
    // Batch6-F21：交互性过滤。CC 2.1.x 的 daemon 后台任务（--fork-session）也写
    // pidfile（kind:"bg" + jobId）——是自己文件的真作者，但不是交互会话，不该成
    // Tab / 进红绿灯 / 进骨架清单。保守规则（与远端 daemon 一字一致）：kind 存在
    // 且非 "interactive" 才排除，旧 CC 无该字段 → 保留。在 by_id 源头纯净化，
    // 下游（diff/snapshot/activity/list_active_sessions）自动干净。
    map.retain(is_interactive);
    map
}

/// Batch6-F21：交互性谓词——`scan_dir` 过滤与单测共用（测产线谓词而非测试内
/// 副本，审计 S2）。签名匹配 `HashMap::retain`。
fn is_interactive(_sid: &String, info: &mut SessionInfo) -> bool {
    info.kind.as_deref().map_or(true, |k| k == "interactive")
}

/// issue #23: 重扫 diff（纯函数，供单测）。
///
/// - removed/added：sid 集合差（与历史 HashSet difference 逻辑等价）。
/// - status_changed：next 中 status/waitingFor 与 prev 不同的会话；**新出现的也算**
///   （让前端立即拿到初始灯色）。CLI 状态转换 = 重写 PID.json = 文件事件 = 必走
///   scan → 本函数，是状态变化的唯一检出点（心跳分支不重读文件、无状态可比）。
fn diff_sessions(
    prev: &HashMap<String, SessionInfo>,
    next: &HashMap<String, SessionInfo>,
) -> SessionChange {
    let removed: Vec<String> = prev
        .keys()
        .filter(|k| !next.contains_key(*k))
        .cloned()
        .collect();
    let added: Vec<String> = next
        .keys()
        .filter(|k| !prev.contains_key(*k))
        .cloned()
        .collect();
    let mut status_changed: Vec<SessionActivity> = Vec::new();
    for (sid, info) in next {
        let changed = match prev.get(sid) {
            Some(p) => p.status != info.status || p.waiting_for != info.waiting_for,
            None => true,
        };
        if changed {
            status_changed.push(SessionActivity {
                session_id: sid.clone(),
                status: info.status.clone(),
                waiting_for: info.waiting_for.clone(),
            });
        }
    }
    SessionChange {
        added,
        removed,
        status_changed,
    }
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
            // issue #23: diff 抽纯函数（可单测，"变化才发"契约的唯一实现点）。
            // 块作用域确保 read guard 在 write 前释放（parking_lot 同线程 read→write 死锁）。
            let change = {
                let prev = by_id.read();
                diff_sessions(&prev, &next)
            };
            *by_id.write() = next;
            if !change.removed.is_empty()
                || !change.added.is_empty()
                || !change.status_changed.is_empty()
            {
                if !change.removed.is_empty() || !change.added.is_empty() {
                    tracing::info!(
                        "session_map: {n} active (+{} -{})",
                        change.added.len(),
                        change.removed.len()
                    );
                }
                if let Some(tx) = &change_tx {
                    let _ = tx.send(change);
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
                .filter(|(_, info)| !is_process_alive(info.pid, info.proc_start.as_deref()))
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
                        status_changed: vec![],
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
//   2) GetProcessTimes 返回的 creation FILETIME 与 sessions/<PID>.json 里记录的
//      procStart（NetTicks 字符串）在 100ms 容差内吻合（FileTime → NetTicks 转换
//      在 utils::FileTime::to_net_local_ticks 中处理）
//
// 没有第二关，残留的死 session PID.json 会因为 PID 被另一个进程复用而被误判为活跃
// （Windows PID 短期内复用很常见）。早期注释里"代价仅是多显示一个 Tab"的判断不成立，
// 用户实际看到的是 4 个僵尸 Tab。

#[cfg(windows)]
fn is_process_alive(pid: u32, expected_proc_start: Option<&str>) -> bool {
    use crate::utils::{FileTime, NetTicks};
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
        //   Claude Code 写的 procStart = NetTicks（.NET Local Ticks）
        //   GetProcessTimes 给的 = FILETIME UTC
        // FileTime → NetTicks 经 utils::FileTime::to_net_local_ticks 转换后比较。
        if let Some(expected) = expected_proc_start.and_then(NetTicks::parse_str) {
            let mut creation = FILETIME::default();
            let mut exit_t = FILETIME::default();
            let mut kernel = FILETIME::default();
            let mut user = FILETIME::default();
            if GetProcessTimes(handle, &mut creation, &mut exit_t, &mut kernel, &mut user).is_ok() {
                let actual = FileTime::from_win32(&creation).to_net_local_ticks();
                let diff = actual.abs_diff(expected);
                if diff > PROC_START_TOLERANCE_TICKS {
                    tracing::debug!(
                        "pid {pid} proc_start mismatch: expected={} actual_net={} diff={} — PID reused",
                        expected.0, actual.0, diff
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

    /// Batch6-F21：kind 字段解析 + 交互性过滤契约。
    #[test]
    fn kind_field_parses_and_bg_is_filtered() {
        // 真实 bg 样本形态（本机 732685.json）：kind:"bg" + jobId
        let bg = r#"{"pid":732685,"sessionId":"6d2d9a38-55a0-4a46-a04e-18cadb0fc9af","cwd":"/x","kind":"bg","jobId":"6d2d9a38"}"#;
        let info: SessionInfo = serde_json::from_str(bg).unwrap();
        assert_eq!(info.kind.as_deref(), Some("bg"));

        let interactive = r#"{"pid":1,"sessionId":"s1","cwd":"/x","kind":"interactive"}"#;
        let legacy = r#"{"pid":2,"sessionId":"s2","cwd":"/x"}"#; // 旧 CC 无 kind
        let i2: SessionInfo = serde_json::from_str(interactive).unwrap();
        let i3: SessionInfo = serde_json::from_str(legacy).unwrap();

        // scan_dir 的过滤规则（产线谓词直测，审计 S2）：bg 拒、interactive 放、缺失放
        let mut info = info;
        let mut i2 = i2;
        let mut i3 = i3;
        assert!(
            !is_interactive(&String::new(), &mut info),
            "kind:bg must be filtered"
        );
        assert!(is_interactive(&String::new(), &mut i2));
        assert!(
            is_interactive(&String::new(), &mut i3),
            "legacy CC without kind must be kept"
        );
    }

    #[test]
    fn parse_session_info() {
        // 来自 Claude Code 实际写入的 sessions/<PID>.json 的最小代表样本；
        // startedAt 等 monitor 不消费的字段也带上，确认 serde 默认能忽略未声明字段。
        // issue #23 起 status 被消费（红绿灯主信号）。
        let raw = r#"{"pid":35776,"sessionId":"5b67f422-52a9-453c-bd64-3288a78a24a0","cwd":"D:\\x","startedAt":1779157297377,"procStart":"639147828963703970","status":"busy"}"#;
        let info: SessionInfo = serde_json::from_str(raw).unwrap();
        assert_eq!(info.pid, 35776);
        assert_eq!(info.session_id, "5b67f422-52a9-453c-bd64-3288a78a24a0");
        assert_eq!(info.proc_start.as_deref(), Some("639147828963703970"));
        assert_eq!(info.status.as_deref(), Some("busy"));
        assert_eq!(info.waiting_for, None);
        assert_eq!(info.name, None);
    }

    // === issue #23: diff_sessions 行为测试（"变化才发"契约的唯一实现点） ===

    /// Batch5-F18：骨架清单排序契约——HashMap 迭代序随机，(cwd, sid) 排序保证
    /// tab 栏跨启动稳定且同项目相邻。
    #[test]
    fn snapshot_active_sorted_by_cwd_then_sid() {
        let mut a = mk("sid-b", None, None);
        a.cwd = "/proj/alpha".into();
        let mut b = mk("sid-a", None, None);
        b.cwd = "/proj/alpha".into();
        let mut c = mk("sid-c", None, None);
        c.cwd = "/proj/beta".into();
        let map = SessionMap {
            dir: std::path::PathBuf::new(),
            by_id: Arc::new(RwLock::new(as_map(vec![a, b, c]))),
        };
        assert_eq!(
            map.snapshot_active(),
            vec![
                ("sid-a".to_string(), "/proj/alpha".to_string()),
                ("sid-b".to_string(), "/proj/alpha".to_string()),
                ("sid-c".to_string(), "/proj/beta".to_string()),
            ]
        );
    }

    fn mk(sid: &str, status: Option<&str>, waiting: Option<&str>) -> SessionInfo {
        SessionInfo {
            pid: 1,
            session_id: sid.to_string(),
            cwd: "x".into(),
            proc_start: None,
            status: status.map(String::from),
            waiting_for: waiting.map(String::from),
            name: None,
            kind: None,
        }
    }
    fn as_map(items: Vec<SessionInfo>) -> HashMap<String, SessionInfo> {
        items
            .into_iter()
            .map(|i| (i.session_id.clone(), i))
            .collect()
    }

    #[test]
    fn diff_new_session_counts_as_added_and_status_changed() {
        // 新会话 → added + status_changed（前端立即拿初始灯色）
        let prev = as_map(vec![]);
        let next = as_map(vec![mk("s1", Some("busy"), None)]);
        let c = diff_sessions(&prev, &next);
        assert_eq!(c.added, vec!["s1".to_string()]);
        assert!(c.removed.is_empty());
        assert_eq!(c.status_changed.len(), 1);
        assert_eq!(c.status_changed[0].status.as_deref(), Some("busy"));
    }

    #[test]
    fn diff_status_flip_detected() {
        // busy → idle 翻转检出，且不误报 added/removed
        let prev = as_map(vec![mk("s1", Some("busy"), None)]);
        let next = as_map(vec![mk("s1", Some("idle"), None)]);
        let c = diff_sessions(&prev, &next);
        assert!(c.added.is_empty() && c.removed.is_empty());
        assert_eq!(c.status_changed.len(), 1);
        assert_eq!(c.status_changed[0].status.as_deref(), Some("idle"));
    }

    #[test]
    fn diff_waiting_for_only_change_detected() {
        // status 同为 waiting、仅 waitingFor 变 → 也算变化（tooltip 细分要跟）
        let prev = as_map(vec![mk("s1", Some("waiting"), Some("dialog open"))]);
        let next = as_map(vec![mk("s1", Some("waiting"), Some("permission prompt"))]);
        let c = diff_sessions(&prev, &next);
        assert_eq!(c.status_changed.len(), 1);
        assert_eq!(
            c.status_changed[0].waiting_for.as_deref(),
            Some("permission prompt")
        );
    }

    #[test]
    fn diff_no_change_is_all_empty() {
        // 无变化 → 三个集合全空（watcher 据此不 send，保持稀疏）
        let prev = as_map(vec![mk("s1", Some("busy"), None), mk("s2", None, None)]);
        let next = as_map(vec![mk("s1", Some("busy"), None), mk("s2", None, None)]);
        let c = diff_sessions(&prev, &next);
        assert!(c.added.is_empty() && c.removed.is_empty() && c.status_changed.is_empty());
    }

    #[test]
    fn diff_removed_session_not_in_status_changed() {
        // 消失的会话只进 removed（灯由 session-ended → archiveTab 收尾）
        let prev = as_map(vec![mk("s1", Some("busy"), None)]);
        let next = as_map(vec![]);
        let c = diff_sessions(&prev, &next);
        assert_eq!(c.removed, vec!["s1".to_string()]);
        assert!(c.status_changed.is_empty());
    }

    /// issue #23：waiting 状态带 waitingFor 细分（CLI v2.1.175 实测字段）。
    /// 旧版 CC 无 status 字段 → None（parse_session_info_minimal 已覆盖缺省路径）。
    #[test]
    fn parse_session_info_waiting_with_reason() {
        let raw = r#"{"pid":1,"sessionId":"s","cwd":"x","procStart":"100","status":"waiting","waitingFor":"permission prompt","updatedAt":1781280404074,"statusUpdatedAt":1781280404074}"#;
        let info: SessionInfo = serde_json::from_str(raw).unwrap();
        assert_eq!(info.status.as_deref(), Some("waiting"));
        assert_eq!(info.waiting_for.as_deref(), Some("permission prompt"));
    }

    #[test]
    fn parse_session_info_minimal() {
        // 最小必需字段（无 status / name / startedAt 等）也能解析
        let raw = r#"{"pid":1,"sessionId":"s","cwd":"x","procStart":"100"}"#;
        let info: SessionInfo = serde_json::from_str(raw).unwrap();
        assert_eq!(info.session_id, "s");
        assert_eq!(info.name, None);
    }

    /// v2.4.2 issue：Claude Code 某些启动路径不写 procStart。
    /// 之前 SessionInfo.proc_start: String 必填导致这种 session 被静默忽略
    /// → monitor Tab 漏。Option 化后能正常解析，proc_start = None。
    #[test]
    fn parse_session_info_without_proc_start() {
        let raw = r#"{"pid":22832,"sessionId":"2bb6394f-xx","cwd":"D:\\x"}"#;
        let info: SessionInfo = serde_json::from_str(raw).unwrap();
        assert_eq!(info.pid, 22832);
        assert_eq!(info.session_id, "2bb6394f-xx");
        assert!(info.proc_start.is_none());
    }
}
