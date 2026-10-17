//! 会话账本：这台机器上一条会话离开「活」之后是
//! **可重连**（claude 退了、tmux 会话还在）还是**已结束**，由这台后端自己裁、发成品帧 [`Frame::SessionState`]。
//!
//! 判定在后端：本机远端同一份代码（`INVARIANTS §40`），客户端只收成品。
//!
//! # 形状：看着自己发出去的帧
//!
//! 挂在 watcher 发帧的唯一出口上（`watcher::FrameSink`，整台一份）：每一帧发出去之前问 [`SessionLedger::on_frame`]，
//! 回「这一帧自己发不发」＋「紧跟它之后补发哪几帧」；每一份 tmux 观测交 [`SessionLedger::on_tmux`]（tmux 快照
//! 不上线，只喂这本账）。它不起线程、不读盘、不定时 —— 输入只有这几样：
//!
//! | 看见 | 做什么 |
//! |---|---|
//! | `session_added` | 记为活（从可重连里摘掉：又活了） |
//! | `session_removed` | `superseded` ⇒ 已结束（旧 sid 的 tmux 那一格已改挂新 sid，查快照必错）；`gone` ⇒ 去向**等一份摘除之后才起的 tmux 观测**来裁（[`SessionLedger::observing`]）：那一份里还挂在某个窗格上 ⇒ 可重连，否则已结束。拿摘除之前起的快照裁，会先报错一格、下一份再翻回来 |
//! | tmux 观测（可观测的） | 收割**只对可重连的**（活不活只认 pidfile / pidfd）：它挂的那个 tmux 会话没了（按句柄 `#{session_id}` 认，改名不算没了）⇒ 当场已结束；连续 [`RETIRE_MISS_THRESHOLD`] 份哪个窗格都不挂它 ⇒ 已结束。每一份还宣告「挂着 `@ccm_sid`、却不在活会话里、没报过、也不在等裁」的为可重连 |
//! | `sessions_replayed` | 第一份 tmux 快照到之前**压住**，到了（不论可不可观测）先放上一格的宣告、再放它 ⇒ 客户端收到「清单报完了」时可重连的也已报过 |
//! | 一条连接半路接上来 | 账本整台一份（read8a）：给它补 [`SessionLedger::snapshot`]（可重连的 · 清单报完了没有） |
//! | 一条连接报了手上留着的会话 | 清单报完了、账上没有它（[`SessionLedger::knows`]）⇒ 只给那条连接一帧已结束（`watcher::FrameSink::settle_kept`），账上记它已结束 |
//! | `session_status` | 那条不在活的里 ⇒ 不发（进程状态文件残留、被重扫推来的陈旧活动改不回一条已结束的会话） |
//! | 问过记录在不在（[`SessionLedger::on_record`]） | 说过已结束的那条：不在 ⇒ 记录已不在 · 又在了 ⇒ 回已结束（给每条连接重发一帧）；活的 · 可重连的不动 |
//!
//! `@ccm_sid` 打在**窗格**上：一个 tmux 会话里可以挂几个 sid，快照按会话句柄记它挂着的那一组。
//! 不可观测的快照（没装 tmux · 观测失败）不收割、不推导；等裁的照它裁（它说的是「这一刻一个 `@ccm_sid` 都看不见」）。
//! 盘只碰一处：出已结束那一刻，账上记着那条的记录路径 ⇒ 看一次它还在不在，不在就直接说记录已不在。

use crate::wire::FrameMake as _;
use std::collections::{BTreeMap, BTreeSet};

use crate::wire::{Frame, Life, RemovalCause, SessionFate};

/// 可重连的会话在 tmux 里**连续几份快照**看不见才算没了（去抖：同一个窗格里换 sid 那一刻新旧并存）。编译期兜死不许小于 2。
pub(crate) const RETIRE_MISS_THRESHOLD: u32 = 2;
const _: () = assert!(RETIRE_MISS_THRESHOLD >= 2);

/// 一条流上的会话账本（见模块头注）。
#[derive(Debug, Default)]
pub(crate) struct SessionLedger {
    live: BTreeSet<String>,
    reconnectable: BTreeSet<String>,
    /// 摘除了（`gone`）、等裁的：sid → 摘除之后有没有起过一份观测（起过 ⇒ 它回来就裁）。
    pending: BTreeMap<String, bool>,
    /// 最新一份可观测的 tmux 快照：会话句柄 → 它各窗格挂着的 sid。`None` = 还没见过任何一份。
    view: Option<BTreeMap<String, BTreeSet<String>>>,
    miss: BTreeMap<String, u32>,
    /// 见过至少一份 tmux 观测（可观测与否都算）。
    tmux_seen: bool,
    /// `sessions_replayed` 被压着没发。
    held_listed: bool,
    /// `sessions_replayed` 已经放出去过（之后接上来的连接，它那一份里带一帧）。
    listed: bool,
    /// 上一份快照的观测种类（只为日志只记变化）。
    last_kind: Option<&'static str>,
    /// 说过已结束的：sid → 记录还在不在（`false` ＝ 说过记录已不在）。又活了 ⇒ 摘掉。
    ended: BTreeMap<String, bool>,
    /// 活会话的记录路径（帧上带过的最新那一份）。
    paths: BTreeMap<String, String>,
}

fn state(sid: &str, state: SessionFate) -> Frame {
    Frame::session_state(sid.to_string(), state)
}

/// 已结束那一帧：记录在 ⇒ 已结束，不在 ⇒ 记录已不在。
fn left(sid: &str, present: bool) -> Frame {
    Frame::session_left(
        sid.to_string(),
        if present { Life::Ended } else { Life::Gone },
    )
}

fn bound_in(view: &BTreeMap<String, BTreeSet<String>>, sid: &str) -> bool {
    view.values().any(|s| s.contains(sid))
}

impl SessionLedger {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// 一份新的 tmux 观测从这一刻起了：此前摘除、等裁的，等它回来裁。
    pub(crate) fn observing(&mut self) {
        for armed in self.pending.values_mut() {
            *armed = true;
        }
    }

    /// 有摘除了、却还没有一份「摘除之后才起」的观测在路上的 ⇒ 调用方该起一份。
    pub(crate) fn awaits_observation(&self) -> bool {
        self.pending.values().any(|armed| !armed)
    }

    /// 一帧要发出去之前：`(它自己发不发, 紧跟它之后补发的帧)`。
    pub(crate) fn on_frame(&mut self, f: &Frame) -> (bool, Vec<Frame>) {
        match f {
            Frame::SessionAdded(crate::wire::SessionAdded { sid, path, .. }) => {
                self.live.insert(sid.clone());
                self.ended.remove(sid);
                if let Some(p) = path {
                    self.paths.insert(sid.clone(), p.clone());
                }
                self.reconnectable.remove(sid);
                self.pending.remove(sid);
                self.miss.remove(sid);
                (true, Vec::new())
            }
            Frame::SessionStatus(crate::wire::SessionStatus { sid, path, .. }) => {
                if !self.live.contains(sid) {
                    return (false, Vec::new());
                }
                if let Some(p) = path {
                    self.paths.insert(sid.clone(), p.clone());
                }
                (true, Vec::new())
            }
            Frame::SessionRemoved(crate::wire::SessionRemoved { sid, cause, .. }) => {
                if !self.live.remove(sid) {
                    return (true, Vec::new());
                }
                self.miss.remove(sid);
                match cause {
                    RemovalCause::Superseded => (true, vec![self.end(sid)]),
                    RemovalCause::Gone => {
                        self.pending.insert(sid.clone(), false);
                        (true, Vec::new())
                    }
                }
            }
            Frame::SessionsReplayed(_) => {
                if self.tmux_seen {
                    self.listed = true;
                    (true, Vec::new())
                } else {
                    self.held_listed = true;
                    (false, Vec::new())
                }
            }
            _ => (true, Vec::new()),
        }
    }

    /// 一份 tmux 观测（`raw` = `tmux ls -F` 原文，`observation` = `watcher` 的四态取值）：裁等裁的 ＋ 收割 ＋ 推导 ＋
    /// 放出压着的清单。回紧跟着要发的帧。
    pub(crate) fn on_tmux(&mut self, raw: &str, observation: Option<&str>) -> Vec<Frame> {
        let observable = crate::observe::tmux_observe::tmux_view_is_observable(raw, observation);
        let kind = if observable {
            "observable"
        } else {
            observation.map_or("legacy_ambiguous_empty", |o| match o {
                "no_tmux" => "no_tmux",
                "unobservable" => "unobservable",
                _ => "other",
            })
        };
        if self.last_kind != Some(kind) {
            tracing::info!(
                "tmux-observation: {} → {kind}",
                self.last_kind.unwrap_or("<首帧>")
            );
            self.last_kind = Some(kind);
        }
        let now = crate::observe::tmux_observe::ledger_view(raw);
        let mut out = Vec::new();
        // ⓪ 摘除之后才起的那份回来了 ⇒ 裁：还挂在某个窗格上 ⇒ 可重连，否则已结束。
        let due: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, armed)| **armed)
            .map(|(s, _)| s.clone())
            .collect();
        for sid in due {
            self.pending.remove(&sid);
            if bound_in(&now, &sid) {
                self.reconnectable.insert(sid.clone());
                out.push(state(&sid, SessionFate::Reconnectable));
            } else {
                out.push(self.end(&sid));
            }
        }
        if observable {
            // ① 可重连的那个 tmux 会话没了（句柄消失 = 确证关了）⇒ 当场已结束（不等去抖）。
            if let Some(prev) = &self.view {
                let closed: BTreeSet<String> = prev
                    .iter()
                    .filter(|(id, _)| !now.contains_key(*id))
                    .flat_map(|(_, sids)| sids.iter().cloned())
                    .filter(|sid| !bound_in(&now, sid))
                    .collect();
                for sid in closed {
                    if self.reconnectable.remove(&sid) {
                        self.miss.remove(&sid);
                        out.push(self.end(&sid));
                    }
                }
            }
            // ② 去抖收割：可重连的、这一份哪个窗格都不挂它 ⇒ 累计；够阈值 ⇒ 已结束。
            self.miss.retain(|s, _| self.reconnectable.contains(s));
            let mut retire = Vec::new();
            for sid in &self.reconnectable {
                if bound_in(&now, sid) {
                    self.miss.remove(sid);
                } else {
                    let n = self.miss.entry(sid.clone()).or_insert(0);
                    *n += 1;
                    if *n >= RETIRE_MISS_THRESHOLD {
                        retire.push(sid.clone());
                    }
                }
            }
            for sid in retire {
                self.reconnectable.remove(&sid);
                self.miss.remove(&sid);
                out.push(self.end(&sid));
            }
            // ③ 挂着 `@ccm_sid`、却不在活会话里、没报过、也不在等裁的 ⇒ 可重连（从 tmux 自己推出来）。
            //   每一份可观测的都推：后端先起、tmux server 后起时第一份是「零会话」，之后那台 server 上挂着标签的会话也要报。
            let reported: BTreeSet<&String> = now.values().flatten().collect();
            for sid in reported {
                if !self.live.contains(sid)
                    && !self.pending.contains_key(sid)
                    && self.reconnectable.insert(sid.clone())
                {
                    self.ended.remove(sid);
                    out.push(state(sid, SessionFate::Reconnectable));
                }
            }
            self.view = Some(now);
        }
        self.tmux_seen = true;
        if std::mem::take(&mut self.held_listed) {
            self.listed = true;
            out.push(Frame::SessionsReplayed(crate::wire::SessionsReplayed {}));
        }
        out
    }

    /// 这一条离开活、已结束：记下；记录路径知道而那份记录已经不在了 ⇒ 说记录已不在。
    fn end(&mut self, sid: &str) -> Frame {
        let present = self
            .paths
            .remove(sid)
            .is_none_or(|p| std::path::Path::new(&p).exists());
        self.ended.insert(sid.to_string(), present);
        left(sid, present)
    }

    /// 一条连接手上留着、账上没有的那条，只给它答过已结束：账上也记着（之后问到记录在不在照样翻）。
    pub(crate) fn said_ended(&mut self, sid: &str) {
        self.ended.entry(sid.to_string()).or_insert(true);
    }

    /// 问过这条的记录在不在（`history-record` · `sessions-start` 答完递来的）：说过已结束的那条翻了 ⇒ 一帧给每条连接。
    pub(crate) fn on_record(&mut self, sid: &str, present: bool) -> Vec<Frame> {
        match self.ended.get_mut(sid) {
            Some(was) if *was != present => {
                *was = present;
                vec![left(sid, present)]
            }
            _ => Vec::new(),
        }
    }

    /// 此刻可重连的那几条（历史清单那一行照它分出 `reconnectable`，[`crate::history::live::set_reconnectable`]）。
    pub(crate) fn reconnectable(&self) -> &BTreeSet<String> {
        &self.reconnectable
    }

    /// 清单报完了（`sessions_replayed` 已放出去）。
    pub(crate) fn listed(&self) -> bool {
        self.listed
    }

    /// 这条会话账上有：活的 · 可重连的 · 等裁的之一。
    pub(crate) fn knows(&self, sid: &str) -> bool {
        self.live.contains(sid)
            || self.reconnectable.contains(sid)
            || self.pending.contains_key(sid)
    }

    /// 一条连接半路接上来时它该补的那一份账（账本整台一份，它没看见之前那些帧）：可重连的各一帧 ·
    /// 清单已经报完了 ⇒ 一帧 `sessions_replayed`（排在最后，同起步那一趟的次序）。等裁的不补：裁了照常发给每条连接。
    pub(crate) fn snapshot(&self) -> Vec<Frame> {
        let mut out: Vec<Frame> = self
            .reconnectable
            .iter()
            .map(|sid| state(sid, SessionFate::Reconnectable))
            .collect();
        if self.listed {
            out.push(Frame::SessionsReplayed(crate::wire::SessionsReplayed {}));
        }
        out
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/session_ledger_tests.rs"]
mod tests;
