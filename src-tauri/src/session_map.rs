//! 活跃 session 探测 —— 不用 hook，直接读 Claude Code 自己维护的 `~/.claude/sessions/<PID>.json`。
//!
//! 每个 PID.json 含：`{pid, sessionId, cwd, startedAt, procStart, status, ...}`
//!
//! ## `procStart` 是**平台原生**的（U7d 实测订正，2026-08-02）
//!
//! 这里原先只写了 Windows 那一种，读起来像是跨平台统一格式 —— **不是**：
//!
//! | 平台 | `procStart` 的量纲 | 拿什么比 |
//! |---|---|---|
//! | Windows | **.NET DateTime.Ticks 字符串**（100ns 自 0001-01-01 **Local**，非 Win32 FILETIME UTC；比较要过 `FileTime::to_net_local_ticks`，详 `utils::NetTicks`） | `GetProcessTimes` 的 FILETIME，直接 u64 等值（容差几毫秒） |
//! | Linux | **`/proc/<pid>/stat` 第 22 字段**（starttime，时钟滴答自 boot） | 同一字段，**逐字符相等** |
//!
//! Linux 那行是实测出来的：本机 6 个真实会话的 `procStart` 与 `/proc` 第 22 字段
//! **6/6 完全相等**，且它们的量级（~10^6）一眼不是 .NET Ticks（~6.4e17）。
//! ⇒ 两个平台各自与本平台的查询口径同源，**PID 复用防御两边都是满精度**，不需要启发式。
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
/// S0：一个 sid **为什么**从活跃集里出去。
///
/// 之所以要这个类型，而不是继续只传 sid：monitor 收到 removed 后要在「灰点（tmux 会话
/// 还在，用户可以回去 attach）」和「归档」之间二选一，原先靠**查自己缓存的那份
/// `tmux ls` 原文里 `@ccm_sid` 还在不在**来猜。`/branch` 场景这个猜法必错——见
/// [`Superseded`](RemovalCause::Superseded)。
///
/// ⚠ **F01b 订正**：原文写「远端由 daemon 在帧里明说，**本地由 diff 得出**」——
/// **后半句是假的**。实测本地那条 diff（`session_map.rs` 的两处）**全部产 `Gone`**，
/// 一处 `Superseded` 都没有；`Superseded` 今天**只从远端帧来**
/// （`ssh_source.rs` 解析 `"cause":"superseded"` 后直接构造）。
///
/// 那么本地 `/branch` 为什么没出「永远消不掉的灰点」那个 bug？
/// **理由不是文档说的那个** —— 是本地 sid **根本不进 `tmux_raw_registry`**
/// （那张表只在 SSH 连接路径按 `host_label` 写）⇒ `find_tmux_origin_for_sid` 恒 `None`
/// ⇒ `classify_removed(None, Gone)` = `Archive`。**结论对、理由是个巧合。**
/// 由 `ssh_source::the_local_path_is_safe_only_because_local_sids_never_enter_the_tmux_cache`
/// 钉住那个巧合 —— 哪天本地会话进了那张表，bug 就回来了。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RemovalCause {
    /// 真的没了：pidfile 被删 / 进程退出 / 连接断开时兜底归档。**默认值。**
    #[default]
    Gone,
    /// 同一个 pidfile 原地换了 sid（`/branch`、`/clear`）：旧 sid **不是死了，是被顶替了**。
    ///
    /// 此时旧 sid 的 tmux 格子确实还在，但那一格现在挂的是**新** sid；判成灰点的话，
    /// 用户会看到一个永远消不掉、也 attach 不上的灰点（按旧 sid 去匹配 `@ccm_sid` 恒失败）。
    Superseded,
}

/// S0：removed 列表的元素——sid + 它为什么走。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedSid {
    pub sid: String,
    pub cause: RemovalCause,
}

impl RemovedSid {
    /// 真死。绝大多数调用点用这个。
    pub fn gone(sid: impl Into<String>) -> Self {
        Self {
            sid: sid.into(),
            cause: RemovalCause::Gone,
        }
    }
    // F01b：`superseded()` 构造器**已删** —— 它生产段零调用方，而它的存在会让人以为
    // 「本地也会产 Superseded」（那正是上面订正掉的那句假陈述）。
    // `Superseded` 今天只从远端帧来，`ssh_source.rs` 直接 `RemovedSid { sid, cause }` 构造。
}

#[derive(Debug, Clone)]
pub struct SessionChange {
    pub added: Vec<String>,
    pub removed: Vec<RemovedSid>,
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

/// Batch7-F24：`snapshot_active` 的富化条目（骨架 tab 清单——kind/name 供
/// ⚙ 标识与树状归属）。
#[derive(Debug, Clone)]
pub struct ActiveSession {
    pub session_id: String,
    pub cwd: String,
    pub kind: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SessionInfo {
    pub pid: u32,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub cwd: String,
    /// 进程启动时刻，**平台原生格式的字符串**（见模块头注那张表）：
    /// Windows = .NET DateTime.ToFileTime()；Linux = `/proc/<pid>/stat` 第 22 字段。
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
    /// Claude 给会话起的语义名（aka ai-title；bg 任务的任务名）。Batch7-F24 起
    /// 由骨架清单/树状标题消费。
    #[serde(default)]
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
    /// Batch7-F24：显示 bg 会话（config.json showBgSessions，默认 true；重启生效）。
    show_bg: bool,
}

impl SessionMap {
    /// 加载 sessions/ 目录的全部活跃 session，并启动 watcher 线程。
    /// 返回一个 channel 接收 session 集合变化（lib.rs 用它推送 session-ended 事件给前端）。
    pub fn load_with_changes(
        dir: PathBuf,
        show_bg: bool,
    ) -> (Arc<Self>, mpsc::Receiver<SessionChange>) {
        tracing::info!(
            "session_map scanning {} (exists={})",
            dir.display(),
            dir.exists()
        );
        let initial = scan_dir(&dir, show_bg);
        tracing::info!("session_map loaded {} entries", initial.len());
        for (sid, info) in &initial {
            tracing::info!("  session: {} pid={} cwd={}", sid, info.pid, info.cwd);
        }
        let (tx, rx) = mpsc::channel::<SessionChange>();
        let me = Arc::new(Self {
            dir: dir.clone(),
            by_id: Arc::new(RwLock::new(initial)),
            show_bg,
        });
        Self::spawn_watcher(&me, Some(tx));
        (me, rx)
    }

    fn spawn_watcher(this: &Arc<Self>, change_tx: Option<mpsc::Sender<SessionChange>>) {
        let dir = this.dir.clone();
        let by_id = this.by_id.clone();
        let show_bg = this.show_bg;
        if let Err(e) = std::thread::Builder::new()
            .name("session-map-watcher".into())
            .spawn(move || run_watcher(dir, by_id, change_tx, show_bg))
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
    pub fn snapshot_active(&self) -> Vec<ActiveSession> {
        let mut v: Vec<ActiveSession> = self
            .by_id
            .read()
            .iter()
            .map(|(sid, info)| ActiveSession {
                session_id: sid.clone(),
                cwd: info.cwd.clone(),
                kind: info.kind.clone(),
                name: info.name.clone(),
            })
            .collect();
        v.sort_by(|a, b| (&a.cwd, &a.session_id).cmp(&(&b.cwd, &b.session_id)));
        v
    }
}

fn scan_dir(dir: &Path, show_bg: bool) -> HashMap<String, SessionInfo> {
    // v2.22.2:先按 pid(文件名,天然唯一)收全量,再按 sid 归并。同 sid 多份
    // pidfile(实证:cc-daemon 的 bg-spare 备用进程复用父会话 sid、标 kind=bg)
    // 时 **interactive 恒压过 bg**——此前直接按 sid 建 map = 目录序先到先得,
    // bg 先扫到会把真交互会话降格成 ⚙、树状挂错宿主(用户截图实锤)。
    // 同 rank 取更新的(procStart 数值比较,缺失回退 pid 大者),消除任意性。
    let by_pid = crate::utils::scan_dir_jsons(dir, |info: &SessionInfo| info.pid);
    let mut map: HashMap<String, SessionInfo> = HashMap::new();
    for (_, info) in by_pid {
        let replace = match map.get(&info.session_id) {
            None => true,
            Some(prev) => {
                let (rp, rn) = (kind_rank(prev), kind_rank(&info));
                rn > rp || (rn == rp && newer_than(&info, prev))
            }
        };
        if replace {
            map.insert(info.session_id.clone(), info);
        }
    }
    // Batch7-F24：showBgSessions 开（默认）→ 保留 bg（kind 字段随 info 透传给下游
    // 做 ⚙ 标识/树状）；关 → 回到 Batch6-F21 行为（bg 不算会话）。
    if show_bg {
        return map;
    }
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
    let ok = info.kind.as_deref().map_or(true, |k| k == "interactive");
    // U-CC1：**排他 ≠ 无声**。这条白名单是刻意的（`kind` 是授权型判据，把 bg 当交互
    // 会让用户对着一个不能打字的东西敲键），但「不在白名单里就隐藏」这件事本身
    // 今天一声不吭 ⇒ CC 加了新 kind 时没有任何信号。只记账，**不改行为**。
    if let Some(k) = info.kind.as_deref() {
        if k != "interactive" && k != "bg" {
            crate::drift_ledger::record(
                crate::drift_ledger::DriftFace::UnknownSessionKind,
                k,
                None,
            );
        }
    }
    ok
}

/// v2.22.2:kind 优先级——interactive(或缺失,旧 CC 视为交互)= 1,bg 等 = 0。
fn kind_rank(info: &SessionInfo) -> u8 {
    if info.kind.as_deref().map_or(true, |k| k == "interactive") {
        1
    } else {
        0
    }
}

/// v2.22.2:同 rank 平局判新——procStart(FILETIME 数值)大者新;缺失回退 pid。
fn newer_than(a: &SessionInfo, b: &SessionInfo) -> bool {
    let ps = |i: &SessionInfo| i.proc_start.as_deref().and_then(|s| s.parse::<u64>().ok());
    match (ps(a), ps(b)) {
        (Some(x), Some(y)) if x != y => x > y,
        _ => a.pid > b.pid,
    }
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
    // S0：本地 diff 只会得出「sid 集合里少了一个」= 真死。本地路径没有远端那条
    // 「同 pidfile 原地换 sid」的信息（那是 daemon 才看得见的 per-pidfile 视角），
    // 也不需要——本地没有 idle-tmux 灰点（`SESSION_IDLE` 是远端专有，见 bridge.rs）。
    let removed: Vec<RemovedSid> = prev
        .keys()
        .filter(|k| !next.contains_key(*k))
        .map(RemovedSid::gone)
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
    show_bg: bool,
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
            let next = scan_dir(&dir, show_bg);
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
                        removed: dead.into_iter().map(RemovedSid::gone).collect(),
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

/// Linux：`/proc/<pid>` 存在性 + `procStart` 精确比对（**同样是双重校验，不是降级**）。
///
/// # 为什么这里能做到与 Windows 同等强度（U7d 实测，2026-08-02）
///
/// 计划原本担心「monitor 侧 `procStart` 是 .NET DateTime.Ticks，而 Linux 的
/// `/proc/<pid>/stat` 第 22 字段是自 boot 的时钟滴答，**量纲不同、不能硬套**」，
/// 并准备降级成「只查存在性 + 标注置信度」。
///
/// **实测推翻了这条前提**：Claude Code 在 Linux 上写进 pidfile 的 `procStart`
/// **就是 `/proc/<pid>/stat` 第 22 字段本身**。本机 6 个真实会话逐个比对，**6/6 完全相等**
/// （`3169940` / `12892607` / `5500689` / `6027532` / `1069089` / `1196681`）——
/// 那些值也一眼不是 .NET Ticks（后者是 ~6.4e17 量级）。
///
/// 也就是说 `procStart` 是**平台原生**的：Windows 上是 FILETIME 系，Linux 上是 jiffies 系，
/// 各自与本平台的查询口径同源。⇒ PID 复用防御在这里是**满精度**的，不需要任何启发式。
///
/// # `procStart` 缺失 ⇒ 只查存在性
///
/// 与 Windows 分支同语义（v2.4.2 实测某些启动路径下 Claude Code 不写这个字段）。
#[cfg(target_os = "linux")]
fn is_process_alive(pid: u32, expected_proc_start: Option<&str>) -> bool {
    let Ok(raw) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false; // 进程不在（或读不到）⇒ 判死。fail-safe：宁可少显示，不显示僵尸
    };
    let Some(want) = expected_proc_start else {
        return true; // 缺 procStart ⇒ 退到存在性，同 Windows 侧
    };
    proc_stat_starttime(&raw).is_some_and(|got| got == want)
}

/// 从 `/proc/<pid>/stat` 原文里取第 22 字段（starttime）。
///
/// # ★ 不能用朴素 `split_whitespace()`
///
/// 第 2 字段 `comm` 是**括号包起来的可执行名，允许含空格与括号**。
/// 实测本机 400 个进程里就有一个踩中：**`comm = "tmux: server"`** ——
/// 朴素切法读到 `0`，正确值是 `1042`。而 tmux server 正是本仓的核心依赖。
///
/// 稳健解法：找**最后一个** `)`（comm 内部的括号不会是最后一个），其后即第 3 字段起，
/// 于是 starttime = 其后第 `22 - 3 = 19` 项（0 基）。
#[cfg(target_os = "linux")]
fn proc_stat_starttime(raw: &str) -> Option<&str> {
    let close = raw.rfind(')')?;
    raw.get(close + 1..)?.split_whitespace().nth(19)
}

/// 其余 unix（**主要是 macOS**）：仍然恒 `false` —— 本机会话不会被监听。
///
/// **这是如实的未实现，不是判据**。macOS 没有 `/proc`，要做得走 `sysctl KERN_PROC`
/// 的 FFI；本仓没有 macOS CI，我也无法在这里实测 —— 按本仓纪律**不写没验过的实现**。
///
/// 为什么返回 `false` 而不是像 daemon 侧那样 `unimplemented!()`：
/// 那边是 CLI，panic 是「没人能忽略的信号」；这边是 GUI 常驻进程，panic 会直接崩掉窗口。
/// `false` 在这里是 **fail-safe**（少显示，而不是显示永不消失的僵尸会话），
/// 且这条限制已写进 `doc/ARCHITECTURE.md` 与双语 README —— **不是静默的谎**。
#[cfg(all(unix, not(target_os = "linux")))]
fn is_process_alive(_pid: u32, _expected_proc_start: Option<&str>) -> bool {
    false
}

/// U7d：Linux 判活的测试。**跑在真进程上**，不是只喂夹具字符串。
#[cfg(all(test, target_os = "linux"))]
mod linux_liveness {
    use super::{is_process_alive, proc_stat_starttime};

    /// ★ 自己这个进程必须被判活，且 `procStart` 要与 `/proc` 对得上。
    ///
    /// 这条同时验了两件事：读得到、比得对。用**真进程**是刻意的 ——
    /// 只喂夹具字符串的话，`/proc` 路径拼错、字段序错位都测不出来。
    #[test]
    fn the_current_process_is_alive_and_its_starttime_matches() {
        let me = std::process::id();
        assert!(
            is_process_alive(me, None),
            "自己这个进程都判成死的 —— /proc 读路径不对"
        );
        let raw = std::fs::read_to_string(format!("/proc/{me}/stat")).expect("读自己的 stat");
        let st = proc_stat_starttime(&raw).expect("抽不到 starttime");
        assert!(
            st.parse::<u64>().is_ok() && st != "0",
            "starttime 抽成了 {st:?} —— 字段序错位（`comm` 含空格时朴素切法就会得到 0）"
        );
        assert!(
            is_process_alive(me, Some(st)),
            "procStart 与 /proc 一致却判成死的"
        );
    }

    /// ★ **PID 复用防御**：pid 对、`procStart` 不对 ⇒ 判死。
    ///
    /// 这条是双重校验的全部意义 —— 少了它，pid 被复用后僵尸条目会一直显示成活跃。
    #[test]
    fn a_mismatched_starttime_means_dead_even_though_the_pid_exists() {
        let me = std::process::id();
        assert!(
            !is_process_alive(me, Some("1")),
            "pid 存在但 procStart 对不上，仍被判活 —— PID 复用防御没生效"
        );
    }

    /// 不存在的 pid ⇒ 判死。用一个**刚退出**的真子进程，不是凭空编号。
    #[test]
    fn a_process_that_has_exited_is_dead() {
        let mut child = std::process::Command::new("true")
            .spawn()
            .expect("起不了子进程");
        let pid = child.id();
        child.wait().expect("wait");
        // 子进程已 reap，`/proc/<pid>` 应当没了。
        assert!(!is_process_alive(pid, None), "已退出并 reap 的进程仍被判活");
    }

    /// ★ `comm` 含空格时字段序不许错位 —— 实测本机 `tmux: server` 就是这种。
    ///
    /// 朴素 `split_whitespace()` 在这条上读到 `0`，正确值是 `1042`。
    #[test]
    fn a_comm_containing_spaces_does_not_shift_the_field_index() {
        // 真实形状（截自 /proc/<tmux-server>/stat，starttime = 1042）
        let raw = "123 (tmux: server) S 1 123 123 0 -1 4194560 900 0 0 0 5 2 0 0 20 \
                   0 1 0 1042 12345678 900 18446744073709551615";
        let raw = raw.replace('\\', "").replace('\n', " ");
        assert_eq!(
            proc_stat_starttime(&raw),
            Some("1042"),
            "`comm` 里的空格让字段序错位了"
        );
        // 反向：朴素切法在同一条输入上会得到别的东西 —— 证明本测试不是空转。
        let naive = raw.split_whitespace().nth(21);
        assert_ne!(
            naive,
            Some("1042"),
            "朴素切法居然也对 —— 那这条测试没有区分力，换一条更刁的输入"
        );
    }

    /// `comm` 里带右括号（如 `(sd-pam)`）也不许错位 —— 靠的是找**最后一个** `)`。
    #[test]
    fn a_comm_containing_a_closing_paren_still_parses() {
        let raw = "7 ((sd-pam)) S 1 7 7 0 -1 4194368 100 0 0 0 0 0 0 0 20 0 1 0 1023 5000 1 1";
        assert_eq!(proc_stat_starttime(raw), Some("1023"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Batch7-F24：scan_dir 的开关双分支——开（默认）保留 bg 且 kind/name 透传；
    /// 关 = F21 行为（bg 不算会话）。
    #[test]
    fn scan_dir_show_bg_switch() {
        let dir = std::env::temp_dir().join(format!("ccm-scanbg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("1.json"),
            r#"{"pid":1,"sessionId":"sid-int","cwd":"/p","kind":"interactive"}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("2.json"),
            r#"{"pid":2,"sessionId":"sid-bg","cwd":"/p","kind":"bg","name":"评估"}"#,
        )
        .unwrap();
        let on = scan_dir(&dir, true);
        assert_eq!(on.len(), 2, "开 = bg 保留");
        assert_eq!(on["sid-bg"].kind.as_deref(), Some("bg"), "kind 透传下游");
        assert_eq!(on["sid-bg"].name.as_deref(), Some("评估"), "name 透传下游");
        let off = scan_dir(&dir, false);
        assert_eq!(off.len(), 1, "关 = F21 行为");
        assert!(off.contains_key("sid-int"));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// v2.22.2:同 sid 多 pidfile 的 kind 冲突消解——interactive 恒压过 bg,
    /// 与目录扫描顺序无关(实证形态:cc-daemon bg-spare 复用父会话 sid)。
    #[test]
    fn scan_dir_same_sid_interactive_wins_over_bg() {
        let dir = std::env::temp_dir().join(format!("ccm-kindrace-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // bg 文件名排前(1.json),interactive 排后(2.json)——旧实现目录序先到先得会输
        std::fs::write(
            dir.join("1.json"),
            r#"{"pid":3051720,"sessionId":"sid-parent","cwd":"/p","kind":"bg","name":"迁移服务","jobId":"sid-pare"}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("2.json"),
            r#"{"pid":16609,"sessionId":"sid-parent","cwd":"/p","kind":"interactive","name":"迁移服务"}"#,
        )
        .unwrap();
        let map = scan_dir(&dir, true);
        assert_eq!(map.len(), 1, "同 sid 归并成一条");
        assert_eq!(
            map["sid-parent"].kind.as_deref(),
            Some("interactive"),
            "interactive 压过 bg(不论扫描顺序)"
        );
        assert_eq!(
            map["sid-parent"].pid, 16609,
            "保留的是 interactive 那份 pidfile"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// v2.22.2:同 rank 平局判新——procStart 数值大者胜,缺失回退 pid。
    #[test]
    fn scan_dir_same_sid_same_kind_newer_wins() {
        let dir = std::env::temp_dir().join(format!("ccm-kindtie-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("1.json"),
            r#"{"pid":100,"sessionId":"s","cwd":"/p","kind":"interactive","procStart":"200"}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("2.json"),
            r#"{"pid":999,"sessionId":"s","cwd":"/p","kind":"interactive","procStart":"100"}"#,
        )
        .unwrap();
        let map = scan_dir(&dir, true);
        assert_eq!(
            map["s"].pid, 100,
            "procStart 更大(更新)者胜,与 pid 大小无关"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Batch6-F21/Batch7-F24：kind 解析 + 关开关时的过滤契约（开 = 保留带标注）。
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
            show_bg: true,
        };
        let out: Vec<(String, String)> = map
            .snapshot_active()
            .into_iter()
            .map(|e| (e.session_id, e.cwd))
            .collect();
        assert_eq!(
            out,
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
        assert_eq!(c.removed, vec![RemovedSid::gone("s1")]);
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
