//! 会话账本：这台机器上一条会话离开「活」之后是
//! **可重连**（claude 退了、tmux 会话还在）还是**已结束**，由这台后端自己裁、发成品帧 [`Frame::SessionState`]。
//!
//! 判定在后端：本机远端同一份代码（`INVARIANTS §40`），客户端只收成品。
//!
//! # 形状：看着自己发出去的帧
//!
//! 挂在 watcher 发帧的唯一出口上（`watcher::FrameSink`，每条流一份）：每一帧发出去之前问 [`SessionLedger::on_frame`]，
//! 回「这一帧自己发不发」＋「紧跟它之后补发哪几帧」；每一份 tmux 观测交 [`SessionLedger::on_tmux`]（tmux 快照
//! 不上线，只喂这本账）。它不起线程、不读盘、不定时 —— 输入只有这几样：
//!
//! | 看见 | 做什么 |
//! |---|---|
//! | `session_added` | 记为活（从可重连里摘掉：又活了） |
//! | `session_removed` | `superseded` ⇒ 已结束（旧 sid 的 tmux 那一格已改挂新 sid，查快照必错）；`gone` ⇒ 去向**等一份摘除之后才起的 tmux 观测**来裁（[`SessionLedger::observing`]）：那一份里还挂在某个窗格上 ⇒ 可重连，否则已结束。拿摘除之前起的快照裁，会先报错一格、下一份再翻回来 |
//! | tmux 观测（可观测的） | 收割**只对可重连的**（活不活只认 pidfile / pidfd）：它挂的那个 tmux 会话没了（按句柄 `#{session_id}` 认，改名不算没了）⇒ 当场已结束；连续 [`RETIRE_MISS_THRESHOLD`] 份哪个窗格都不挂它 ⇒ 已结束。每一份还宣告「挂着 `@ccm_sid`、却不在活会话里、没报过、也不在等裁」的为可重连 |
//! | `sessions_replayed` | 第一份 tmux 快照到之前**压住**，到了（不论可不可观测）先放上一格的宣告、再放它 ⇒ 客户端收到「清单报完了」时可重连的也已报过 |
//!
//! `@ccm_sid` 打在**窗格**上：一个 tmux 会话里可以挂几个 sid，快照按会话句柄记它挂着的那一组。
//! 不可观测的快照（没装 tmux · 观测失败）不收割、不推导；等裁的照它裁（它说的是「这一刻一个 `@ccm_sid` 都看不见」）。

use std::collections::{BTreeMap, BTreeSet};

use crate::stream::wire::{Frame, RemovalCause, SessionFate};

/// 可重连的会话在 tmux 里**连续几份快照**看不见才算没了（去抖：同一个窗格里换 sid 那一拍新旧并存）。编译期兜死不许小于 2。
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
    /// 上一份快照的观测种类（只为日志只记变化）。
    last_kind: Option<&'static str>,
}

fn state(sid: &str, state: SessionFate) -> Frame {
    Frame::SessionState {
        sid: sid.to_string(),
        state,
    }
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
            Frame::SessionAdded { sid, .. } => {
                self.live.insert(sid.clone());
                self.reconnectable.remove(sid);
                self.pending.remove(sid);
                self.miss.remove(sid);
                (true, Vec::new())
            }
            Frame::SessionRemoved { sid, cause } => {
                if !self.live.remove(sid) {
                    return (true, Vec::new());
                }
                self.miss.remove(sid);
                match cause {
                    RemovalCause::Superseded => (true, vec![state(sid, SessionFate::Ended)]),
                    RemovalCause::Gone => {
                        self.pending.insert(sid.clone(), false);
                        (true, Vec::new())
                    }
                }
            }
            Frame::SessionsReplayed => {
                if self.tmux_seen {
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
                out.push(state(&sid, SessionFate::Ended));
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
                        out.push(state(&sid, SessionFate::Ended));
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
                out.push(state(&sid, SessionFate::Ended));
            }
            // ③ 挂着 `@ccm_sid`、却不在活会话里、没报过、也不在等裁的 ⇒ 可重连（从 tmux 自己推出来）。
            //   每一份可观测的都推：后端先起、tmux server 后起时第一份是「零会话」，之后那台 server 上挂着标签的会话也要报。
            let reported: BTreeSet<&String> = now.values().flatten().collect();
            for sid in reported {
                if !self.live.contains(sid)
                    && !self.pending.contains_key(sid)
                    && self.reconnectable.insert(sid.clone())
                {
                    out.push(state(sid, SessionFate::Reconnectable));
                }
            }
            self.view = Some(now);
        }
        self.tmux_seen = true;
        if std::mem::take(&mut self.held_listed) {
            out.push(Frame::SessionsReplayed);
        }
        out
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/session_ledger_tests.rs"]
mod tests;
