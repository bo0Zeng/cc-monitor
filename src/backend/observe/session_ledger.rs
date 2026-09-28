//! 〔MIG-1 · `设计/99 §2.1 ⑬` · `01 §1.1`「一切判定都在后端」〕**会话账本**：这台机器上一条会话离开「活」之后是
//! **可重连**（claude 退了、tmux 会话还在）还是**已结束**，由这台后端自己裁、发成品帧 [`Frame::SessionState`]。
//!
//! 原先这一套住 monitor（`ssh_source` 的 tmux 原文账 · idle 账 · `classify_removed` · 两份收割器 · 重连后重新裁），
//! 本机远端各一份。搬进来之后本机远端同一份代码（`INVARIANTS §40`），客户端只收成品。
//!
//! # 形状：看着自己发出去的帧
//!
//! 挂在 watcher 发帧的唯一出口上（`watcher::FrameSink`，每条流一份）：每一帧发出去之前问 [`SessionLedger::on_frame`]，
//! 回「这一帧自己发不发」＋「紧跟它之后补发哪几帧」；每一份 tmux 观测交 [`SessionLedger::on_tmux`]（〔MIG-1 续 · V41〕tmux 快照
//! **不再上线**：原 `tmux_sessions` / `tmux_session_closed` 两帧删了，快照只喂这本账）。它不起线程、不读盘、不定时 —— 输入只有这几样：
//!
//! | 看见 | 做什么 |
//! |---|---|
//! | `session_added` | 记为活（从可重连里摘掉：又活了） |
//! | `session_removed` | `superseded` ⇒ 已结束（旧 sid 的 tmux 那一格已改挂新 sid，查快照必错）；`gone` ⇒ 最新快照里 `@ccm_sid` 还挂着它 ⇒ 可重连，否则已结束 |
//! | tmux 观测（可观测的） | 收割：会话名消失（确证关了）⇒ 当场已结束；活的 / 可重连的、曾挂在 tmux 里、连续 [`RETIRE_MISS_THRESHOLD`] 份不见 ⇒ 已结束（带外杀掉 tmux，#60-A）。每一份还宣告「挂着 `@ccm_sid`、却不在活会话里、没报过」的为可重连（〔MIG-1 续四〕原先只第一份） |
//! | `sessions_replayed` | 第一份 tmux 快照到之前**压住**，到了（不论可不可观测）先放上一格的宣告、再放它 ⇒ 客户端收到「清单报完了」时可重连的也已报过 |
//!
//! 不可观测的快照（没装 tmux · 观测失败）不收割、不推导；摘除时照它裁（它说的是「这一刻一个 `@ccm_sid` 都看不见」）。

use std::collections::{BTreeMap, BTreeSet};

use crate::wire::{Frame, RemovalCause, SessionFate};

/// 活的 / 可重连的会话在 tmux 里**连续几份快照**看不见才算没了（去抖：`/branch` 换 sid 那一拍新旧并存，
/// 等于 1 会把还活着、只是换了 sid 的会话误判）。编译期兜死不许小于 2。
pub(crate) const RETIRE_MISS_THRESHOLD: u32 = 2;
const _: () = assert!(RETIRE_MISS_THRESHOLD >= 2);

/// 一条流上的会话账本（见模块头注）。
#[derive(Debug, Default)]
pub(crate) struct SessionLedger {
    live: BTreeSet<String>,
    reconnectable: BTreeSet<String>,
    /// 最新一份 tmux 快照：会话名 → `@ccm_sid`（空串 = 没挂）。`None` = 还没见过任何一份。
    view: Option<BTreeMap<String, String>>,
    /// 曾在某份快照里挂着的 sid（只对这些收割：没进过 tmux 的会话永不因「tmux 里没它」而判死）。
    ever_bound: BTreeSet<String>,
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

impl SessionLedger {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// 某个 sid 此刻在最新快照里挂着吗。
    fn bound(&self, sid: &str) -> bool {
        self.view
            .as_ref()
            .is_some_and(|v| v.values().any(|s| s == sid))
    }

    /// 一帧要发出去之前：`(它自己发不发, 紧跟它之后补发的帧)`。
    pub(crate) fn on_frame(&mut self, f: &Frame) -> (bool, Vec<Frame>) {
        match f {
            Frame::SessionAdded { sid, .. } => {
                self.live.insert(sid.clone());
                self.reconnectable.remove(sid);
                self.miss.remove(sid);
                (true, Vec::new())
            }
            Frame::SessionRemoved { sid, cause } => {
                if !self.live.remove(sid) {
                    return (true, Vec::new());
                }
                self.miss.remove(sid);
                let fate = match cause {
                    RemovalCause::Superseded => SessionFate::Ended,
                    RemovalCause::Gone if self.bound(sid) => SessionFate::Reconnectable,
                    RemovalCause::Gone => SessionFate::Ended,
                };
                if fate == SessionFate::Reconnectable {
                    self.reconnectable.insert(sid.clone());
                    self.ever_bound.insert(sid.clone());
                } else {
                    self.ever_bound.remove(sid);
                }
                (true, vec![state(sid, fate)])
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

    /// 一份 tmux 观测（`raw` = `tmux ls -F` 原文，`observation` = `watcher` 的四态取值）：收割 ＋（第一份可观测的）推导 ＋
    /// 放出压着的清单。回紧跟着要发的帧。
    pub(crate) fn on_tmux(&mut self, raw: &str, observation: Option<&str>) -> Vec<Frame> {
        let observable = crate::observe::watcher::tmux_view_is_observable(raw, observation);
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
        let now: BTreeMap<String, String> = crate::observe::watcher::session_rows(raw)
            .into_iter()
            .map(|r| (r.name, r.ccm_sid))
            .collect();
        let mut out = Vec::new();
        if observable {
            // ① 会话名消失 = 确证关了：挂在它上面的那条当场已结束（不等去抖）。
            if let Some(prev) = &self.view {
                let closed: Vec<String> = prev
                    .iter()
                    .filter(|(name, sid)| !sid.is_empty() && !now.contains_key(*name))
                    .map(|(_, sid)| sid.clone())
                    .collect();
                for sid in closed {
                    if self.reconnectable.remove(&sid) || self.live.remove(&sid) {
                        self.ever_bound.remove(&sid);
                        self.miss.remove(&sid);
                        out.push(state(&sid, SessionFate::Ended));
                    }
                }
            }
            // ② 去抖收割：活的 ∪ 可重连的，曾挂过、这一份不见 ⇒ 累计；够阈值 ⇒ 已结束。
            let reported: BTreeSet<&str> = now
                .values()
                .filter(|s| !s.is_empty())
                .map(String::as_str)
                .collect();
            let tracked: BTreeSet<String> = self.live.union(&self.reconnectable).cloned().collect();
            self.miss.retain(|s, _| tracked.contains(s));
            self.ever_bound.retain(|s| tracked.contains(s));
            for sid in &self.reconnectable {
                self.ever_bound.insert(sid.clone());
            }
            let mut retire = Vec::new();
            for sid in &tracked {
                if reported.contains(sid.as_str()) {
                    self.ever_bound.insert(sid.clone());
                    self.miss.remove(sid);
                } else if self.ever_bound.contains(sid) {
                    let n = self.miss.entry(sid.clone()).or_insert(0);
                    *n += 1;
                    if *n >= RETIRE_MISS_THRESHOLD {
                        retire.push(sid.clone());
                    }
                }
            }
            for sid in retire {
                self.reconnectable.remove(&sid);
                self.live.remove(&sid);
                self.ever_bound.remove(&sid);
                self.miss.remove(&sid);
                out.push(state(&sid, SessionFate::Ended));
            }
            // ③ 每一份可观测的快照：挂着 `@ccm_sid`、却不在活会话里、还没报过可重连的 ⇒ 可重连（从 tmux 自己推出来）。
            //   〔MIG-1 续四 · `backend-tmux-late-server` 那一格〕原先只在**第一份**推：后端先起、tmux server 后起时第一份是
            //   「零会话」，之后那台 server 上挂着标签的会话就再也没人报 ⇒ 灰灯永不出现（#60 同形，换了一层）。
            for sid in reported {
                if !self.live.contains(sid) && self.reconnectable.insert(sid.to_string()) {
                    self.ever_bound.insert(sid.to_string());
                    out.push(state(sid, SessionFate::Reconnectable));
                }
            }
        }
        self.view = Some(now);
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
