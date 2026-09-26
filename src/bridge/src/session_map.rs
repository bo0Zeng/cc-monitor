//! 本机活会话表 —— **由本机常驻后端那条流的帧喂出来**，monitor 自己不再看 `~/.claude/sessions/<PID>.json`。
//!
//! # 〔LOC1b · 第四波 4D〕它换掉的是什么
//!
//! 这个模块原来是 monitor 里的**第二份判活**：自己 `notify` 盯 `sessions/<PID>.json`、自己读 `/proc/<pid>/stat`
//! 第 22 字段（Windows 走 `GetProcessTimes`）核 `procStart`、再加一条 2 s 心跳扫死进程（`SessionMap`〔散文墓碑〕·
//! `run_watcher`〔散文墓碑〕· `is_process_alive`〔散文墓碑〕）。同一件事本机常驻后端早就在做（`observe/watcher.rs`：
//! inotify ＋ pidfd 看守 ＋ 冒名检查），它发的 `session_added` / `session_status` / `session_removed` / `sessions_replayed`
//! 本机那条流上**一直在来，被整个丢掉**。
//! 主会话 09-25 裁「迁」（按 `设计/00 §2.2` · `01 §1.1`「一切判定都在后端」· `INVARIANTS §40`「本地＝不走 ssh 的远端」；
//! ⚠ 偏离 `05 §15.1` 那句「本机会话的起停归 `session_map`」—— 那是 09-24 的迁移范围裁定，由 DD1 改）。
//! RT1 F9 的真机读数支持退役心跳：Windows 上后端 1 ms 就醒的那一帧早于 monitor 心跳 1 s。
//!
//! # 形状
//!
//! - 本机那条流的起停帧 ⇒ [`Lifecycle`]（`ssh_source::local_lifecycle` 从帧里摘，藏起来的 bg 会话不进）；
//! - [`LocalTable::step`] 是**纯**的：一件起停事实 × 这张表 ⇒ 表怎么变 ＋ 交给本机 emitter 的 [`Out`]；
//! - 本机 emitter（`lib.rs` 的 `session-changes-emitter`）吃 [`Out`]：与远端 emitter 同一个裁决
//!   （`ssh_source::classify_removed`）、同一组事件。
//!
//! # 流断：与远端同一形（主会话 09-25 补的那一件）
//!
//! 本机那条流结束 ⇒ 表里的活会话 ∪ 本机的可重连会话**一律 `Unseen`**（`ssh_source::disconnect_removals`，
//! 远端断连 flush 用的**同一个函数**）⇒ 前端「说不清」，不是「已结束」（`设计/30 §3.5.7a`）；
//! 「报完了清单」那本账（`ssh_source::note_listed` / `forget_listed`）本机也记，F5 对账按它分已结束 / 说不清。
//! 下一条流初扫重新宣告 ⇒ 回活；`sessions_replayed` 之后仍没被报的由前端落已结束。

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::OnceLock;

/// S0：一个 sid **为什么**从活跃集里出去。
///
/// 之所以要这个类型，而不是只传 sid：emitter 收到 removed 后要在「可重连（tmux 会话还在）」「已结束」
/// 「说不清」之间选一个，而 `/branch` 那一形（[`Self::Superseded`]）按 tmux 快照猜必错。
/// 三种 cause 都由**后端**在帧里明说（`session_removed.cause`），或由断连 flush 给（[`Self::Unseen`]）——
/// 〔LOC1b〕本机从此也一样：本机那份「按 `pid + procStart` diff 出 `Superseded`」的实现（`diff_sessions`〔散文墓碑〕）
/// 随本机判活一起删了，本机的 `Superseded` 由本机后端说（`observe/watcher.rs` 同 pidfile 换 sid 那一支）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RemovalCause {
    /// 真的没了：pidfile 被删 / 进程退出。**默认值。**
    /// 〔GP1 · 第四波〕从前这句还有「/ 连接断开时兜底归档」—— 断开那一形改成了下面的 [`Self::Unseen`]。
    #[default]
    Gone,
    /// 同一个 pidfile 原地换了 sid（`/branch`、`/clear`）：旧 sid **不是死了，是被顶替了**。
    ///
    /// 此时旧 sid 的 tmux 格子确实还在，但那一格现在挂的是**新** sid；判成灰点的话，
    /// 用户会看到一个永远消不掉、也 attach 不上的灰点（按旧 sid 去匹配 `@ccm_sid` 恒失败）。
    Superseded,
    /// 〔GP1 · 第四波〕**那台机器看不见了**（到它的那条连接断了）：说不清这条会话还在不在。
    ///
    /// 产出者是两条流的断连 flush，同一个函数（`ssh_source::disconnect_removals`）：远端 `ssh_source::run` ·
    /// 〔LOC1b〕本机 [`LocalTable::step`] 收 [`Lifecycle::StreamEnded`] 那一臂。
    /// 它说的是**机器**，不是会话 ⇒ 裁决不看 tmux 快照，
    /// 落到前端是「说不清」而不是「已结束」（`设计/30 §3.5.7a`：`Unseen` 不许被显示成已结束）。
    Unseen,
}

/// S0：removed 列表的元素——sid + 它为什么走。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedSid {
    pub sid: String,
    pub cause: RemovalCause,
}

impl RemovedSid {
    /// 真死。
    pub fn gone(sid: impl Into<String>) -> Self {
        Self {
            sid: sid.into(),
            cause: RemovalCause::Gone,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionChange {
    pub added: Vec<String>,
    pub removed: Vec<RemovedSid>,
    /// issue #23: 红绿灯——status/waitingFor 变了（含新宣告的初始值）的会话。
    pub status_changed: Vec<SessionActivity>,
}

/// issue #23: 单个会话的红绿灯状态快照（status 直接来自 Claude Code 官方字段，经后端帧转交）。
#[derive(Debug, Clone, PartialEq)]
pub struct SessionActivity {
    pub session_id: String,
    /// "busy"（运行中）/ "idle"/"shell"（等输入）/ "waiting"（等弹窗决定）。
    /// None = 旧版 CC 没写该字段。
    pub status: Option<String>,
    /// status=="waiting" 时的细分（"permission prompt" / "dialog open" / "input needed"…）
    pub waiting_for: Option<String>,
}

/// Batch7-F24：`list_active_sessions` 的条目（骨架 tab 清单 —— kind/name 供 ⚙ 标识与 bg 标题）。
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveSession {
    pub session_id: String,
    pub cwd: String,
    pub kind: Option<String>,
    pub name: Option<String>,
}

/// 表里的一条：本机后端宣告这条会话时带来的元信息（`session_added` 的那几格），之后按 `session_status` 更新灯。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LiveEntry {
    pub cwd: Option<String>,
    /// `session_kind`（"interactive" / "bg"；旧 CC 缺）。
    pub kind: Option<String>,
    pub name: Option<String>,
    pub status: Option<String>,
    pub waiting_for: Option<String>,
    /// 那个 claude 进程的 pid（`session_added.pid`，〔LOC1b〕additive）。本机 ↗ 按它找父 PowerShell 绑窗口
    /// （`bind::SidHwndCache::record`，只在 Windows 上有）；老后端不带 ⇒ 那一格不绑，不猜。
    pub pid: Option<u32>,
}

/// 本机那条流上的一件起停事实（已经滤掉被藏起来的 bg 会话）。
#[derive(Debug, Clone, PartialEq)]
pub enum Lifecycle {
    Added {
        sid: String,
        entry: LiveEntry,
    },
    Status {
        sid: String,
        status: Option<String>,
        waiting_for: Option<String>,
    },
    Removed {
        sid: String,
        cause: RemovalCause,
    },
    /// `sessions_replayed`：这台的活会话清单报完了。
    Listed,
    /// 这条流结束了。`idle` = 此刻本机的可重连会话（与远端断连 flush 并进来的同一摞）。
    StreamEnded {
        idle: Vec<String>,
    },
}

/// 交给本机 emitter 的东西。**顺序就是意义**：`Listed` 排在它前面那些 `Change` 之后发，
/// 前端收到它时本机全部活会话都已宣告过。
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    Change(SessionChange),
    Listed,
}

/// 本机活会话表。
#[derive(Debug, Default)]
pub struct LocalTable {
    by_id: HashMap<String, LiveEntry>,
    /// 这条流上收到过 `sessions_replayed`（流一断就不算数）。
    listed: bool,
}

impl LocalTable {
    /// **纯**：一件起停事实 ⇒ 表怎么变 ＋ 交给 emitter 什么。
    pub fn step(&mut self, ev: Lifecycle) -> Vec<Out> {
        match ev {
            Lifecycle::Added { sid, entry } => {
                let activity = SessionActivity {
                    session_id: sid.clone(),
                    status: entry.status.clone(),
                    waiting_for: entry.waiting_for.clone(),
                };
                self.by_id.insert(sid.clone(), entry);
                vec![Out::Change(SessionChange {
                    added: vec![sid],
                    removed: vec![],
                    status_changed: vec![activity],
                })]
            }
            Lifecycle::Status {
                sid,
                status,
                waiting_for,
            } => {
                let Some(e) = self.by_id.get_mut(&sid) else {
                    return vec![];
                };
                e.status = status.clone();
                e.waiting_for = waiting_for.clone();
                vec![Out::Change(SessionChange {
                    status_changed: vec![SessionActivity {
                        session_id: sid,
                        status,
                        waiting_for,
                    }],
                    ..Default::default()
                })]
            }
            Lifecycle::Removed { sid, cause } => {
                if self.by_id.remove(&sid).is_none() {
                    return vec![];
                }
                vec![Out::Change(SessionChange {
                    removed: vec![RemovedSid { sid, cause }],
                    ..Default::default()
                })]
            }
            Lifecycle::Listed => {
                self.listed = true;
                vec![Out::Listed]
            }
            Lifecycle::StreamEnded { idle } => {
                self.listed = false;
                let mut announced: Vec<String> = self.by_id.drain().map(|(sid, _)| sid).collect();
                announced.sort();
                let removed = crate::ssh_source::disconnect_removals(announced, idle);
                if removed.is_empty() {
                    return vec![];
                }
                vec![Out::Change(SessionChange {
                    removed,
                    ..Default::default()
                })]
            }
        }
    }

    pub fn is_active(&self, sid: &str) -> bool {
        self.by_id.contains_key(sid)
    }

    pub fn lookup(&self, sid: &str) -> Option<LiveEntry> {
        self.by_id.get(sid).cloned()
    }

    /// issue #23: 当前全部活会话的红绿灯快照（前端启动 / F5 后拉一次做初始收敛）。
    pub fn snapshot_activity(&self) -> Vec<SessionActivity> {
        let mut v: Vec<SessionActivity> = self
            .by_id
            .iter()
            .map(|(sid, e)| SessionActivity {
                session_id: sid.clone(),
                status: e.status.clone(),
                waiting_for: e.waiting_for.clone(),
            })
            .collect();
        v.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        v
    }

    /// `list_active_sessions` 的答案：清单**报完了**才给（`Some`），没报完 ⇒ `None`（不交半截的清单 ——
    /// 前端拿它把固定复活的本机 tab 落成「活 / 已结束」，半截的会把还没宣告到的活会话说成已结束，`设计/30 §3.5.7a`）。
    pub fn listed_active(&self) -> Option<Vec<ActiveSession>> {
        self.listed.then(|| self.snapshot_active())
    }

    /// Batch5-F18：活会话清单，按 (cwd, sid) 排（跨启动稳定，同项目的会话相邻）。
    pub fn snapshot_active(&self) -> Vec<ActiveSession> {
        let mut v: Vec<ActiveSession> = self
            .by_id
            .iter()
            .map(|(sid, e)| ActiveSession {
                session_id: sid.clone(),
                cwd: e.cwd.clone().unwrap_or_default(),
                kind: e.kind.clone(),
                name: e.name.clone(),
            })
            .collect();
        v.sort_by(|a, b| (&a.cwd, &a.session_id).cmp(&(&b.cwd, &b.session_id)));
        v
    }
}

static TABLE: OnceLock<RwLock<LocalTable>> = OnceLock::new();
static SINK: OnceLock<std::sync::mpsc::Sender<Out>> = OnceLock::new();

/// 本机那一张（进程里只有一条本机流 ⇒ 只有一张）。
pub fn local() -> &'static RwLock<LocalTable> {
    TABLE.get_or_init(Default::default)
}

/// 装本机 emitter 的入口（`lib.rs` 起步段调一次）。第二次调是用法错：大声说，不换。
pub fn install_sink(tx: std::sync::mpsc::Sender<Out>) {
    if SINK.set(tx).is_err() {
        tracing::error!(
            "session_map::install_sink 被调了第二次 —— 本机 emitter 只许有一个，这次不换"
        );
    }
}

/// 本机那条流交来一件起停事实：改表，把结果按序交给 emitter。
///
/// 「报完了清单」那本账与远端同一本（`ssh_source::note_listed` / `forget_listed`，键 `<local>`）——
/// F5 对账按它分「已结束 / 说不清」，本机远端一条规则。
pub fn feed(ev: Lifecycle) {
    let origin = crate::backend::control::inbound_client::LOCAL_ORIGIN;
    match &ev {
        Lifecycle::Listed => crate::ssh_source::note_listed(origin),
        Lifecycle::StreamEnded { .. } => crate::ssh_source::forget_listed(origin),
        _ => {}
    }
    let outs = local().write().step(ev);
    let Some(tx) = SINK.get() else {
        if !outs.is_empty() {
            tracing::warn!(
                "本机 emitter 还没装上（session_map::install_sink 没调过）—— 这 {} 件丢掉",
                outs.len()
            );
        }
        return;
    };
    for o in outs {
        if let Err(e) = tx.send(o) {
            tracing::warn!("本机 emitter 收不了了：{e}");
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/bridge/session_map_tests.rs"]
mod tests;
