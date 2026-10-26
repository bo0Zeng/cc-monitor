//! **一台的会话簿**：那台后端在这条连接上说过的会话成品，折成此刻的整份 —— 桌面壳（`session_book`，每台一本）与手机核心
//! （`mobcore::resident`，每台一本）都只调这里的 [`SessionBook::fold`]，各端不另写一份折法。
//!
//! 只记、不裁：活 / 可重连 / 已结束、脸、标题都是那台后端裁好的（帧 `session_added` · `session_status` · `session_state` …），
//! 这里只照帧把整份换到此刻。连接断了那一刻（说不清）由各端自己说：簿随连接走，断了各端丢掉这一本。

use std::collections::BTreeMap;

use cmd_wire::frame::{Frame, SessionAdded, SessionFace, SessionRuns, SessionState};
use serde::{Deserialize, Serialize};

/// 一台的会话簿（整份）：交给界面 / Kotlin 的就是它。一条会话在 `live` 与 `left` 里至多出现一处。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct SessionBook {
    /// 活着的：宣告那一帧，之后的状态帧换上脸 · 标题 · 路径。
    pub live: BTreeMap<String, SessionAdded>,
    /// 走了的：那台答的去向（可重连 · 已结束，带那一张脸）。
    pub left: BTreeMap<String, SessionState>,
    /// 活会话的最新运行表（会话走了 ⇒ 摘）。
    pub runs: BTreeMap<String, SessionRuns>,
    /// 被顶替的会话 ⇒ 顶替它的那一条（`session_removed.by`：清空之后同一个进程另起的那一份记录）。
    pub successors: BTreeMap<String, String>,
    /// 每条会话的记录被从头重读过几回（`session_file_reread`）：数变了 ⇒ 读过的那一代作废、整份重来。
    #[cfg_attr(test, ts(type = "{ [key in string]: number }"))]
    pub rereads: BTreeMap<String, u64>,
    /// 那台说「活会话清单报完了」（`sessions_replayed`）。
    pub listed: bool,
}

/// 簿里一条会话此刻是哪一种（按 sid 的先后读整份用）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seat<'a> {
    Live(&'a SessionAdded),
    Left(&'a SessionState),
}

impl SessionBook {
    /// **一帧一步**（全仓唯一的折法）：会话那几种帧把整份换到此刻，回 `true`；别的帧不动、回 `false`。
    pub fn fold<R>(&mut self, frame: &Frame<R>) -> bool {
        match frame {
            Frame::SessionAdded(m) => {
                self.left.remove(&m.sid);
                self.live.insert(m.sid.clone(), m.clone());
            }
            Frame::SessionStatus(s) => {
                if let Some(m) = self.live.get_mut(&s.sid) {
                    m.face = s.face.clone();
                    m.name_face = s.name_face.clone();
                    // 宣告以后记录才出现 · 换了一份：那台在那一行的 line 帧之前先发带路径的状态帧。
                    if let Some(p) = &s.path {
                        m.path = Some(p.clone());
                    }
                }
            }
            Frame::SessionState(s) => {
                self.live.remove(&s.sid);
                self.runs.remove(&s.sid);
                self.left.insert(s.sid.clone(), s.clone());
            }
            // 只剩内容流的边界：去向由紧跟着的 `session_state` 说；这里只记顶替它的那一条。
            Frame::SessionRemoved(r) => match &r.by {
                Some(by) => {
                    self.successors.insert(r.sid.clone(), by.clone());
                }
                None => return false,
            },
            Frame::SessionFileReread(r) => {
                *self.rereads.entry(r.session_id.clone()).or_default() += 1;
            }
            Frame::SessionsReplayed(_) => self.listed = true,
            Frame::SessionRuns(r) => {
                self.runs.insert(r.sid.clone(), r.clone());
            }
            _ => return false,
        }
        true
    }

    /// 那一条此刻的整份脸（活的 ⇒ 最新状态；走了的 ⇒ 去向那一张；不在 ⇒ `None`）。
    pub fn face(&self, sid: &str) -> Option<&SessionFace> {
        self.live
            .get(sid)
            .map(|m| &m.face)
            .or_else(|| self.left.get(sid).map(|s| &s.face))
    }

    /// 那一条此刻在簿里是哪一种。
    pub fn seat(&self, sid: &str) -> Option<Seat<'_>> {
        self.live
            .get(sid)
            .map(Seat::Live)
            .or_else(|| self.left.get(sid).map(Seat::Left))
    }

    /// 整份按 sid 的先后（活的与走了的合在一起）。
    pub fn seats(&self) -> impl Iterator<Item = (&String, Seat<'_>)> {
        let mut all: Vec<(&String, Seat<'_>)> = self
            .live
            .iter()
            .map(|(k, m)| (k, Seat::Live(m)))
            .chain(self.left.iter().map(|(k, s)| (k, Seat::Left(s))))
            .collect();
        all.sort_by(|a, b| a.0.cmp(b.0));
        all.into_iter()
    }

    /// 用户关掉一个已结束的 tab ⇒ 那一条不再记。
    pub fn forget(&mut self, sid: &str) {
        self.live.remove(sid);
        self.left.remove(sid);
        self.runs.remove(sid);
    }
}

#[cfg(test)]
#[path = "../../../tests/comms/resident-client/book_tests.rs"]
mod tests;
