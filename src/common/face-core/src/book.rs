//! **一台的会话簿**：那台后端在这条连接上说过的会话成品，折成此刻的整份 —— 桌面壳（`session_book`，每台一本）与手机核心
//! （`mobcore::resident`，每台一本）都只调这里的 [`SessionBook::fold`]，各端不另写一份折法。
//!
//! 只记、不裁：活 / 可重连 / 已结束、脸、标题都是那台后端裁好的（帧 `session_added` · `session_status` · `session_left` …），
//! 这里只照帧把整份换到此刻。连接断了那一刻的说不清也是一帧 `session_left`（`face.life = unclear`），脸是 [`unseen`] 那一张：
//! 桌面那一路由本机后端照它那一本发（`control/feeds.rs`），手机核心换流那一下照同一处折（[`SessionBook::unseen_now`]）。

use std::collections::BTreeMap;

use cmd_wire::cells::{Tone, Words};
use cmd_wire::frame::{Allow, CanClose, CanSay, CanView, SessionCan, SessionDot};
use cmd_wire::frame::{Frame, Life, SessionAdded, SessionFace, SessionLeft, SessionRuns};
use copy_core::copy_text;
use serde::{Deserialize, Serialize};

/// **那台看不见（到它的连接断了）时那一张脸**，全仓唯一的构造口：点说不清 · 名「状态不明」· 那一句 ＝ `offline`（那台的离线那一句，
/// 与机器成品的 `offline_said` 同一处取字）· 各样一律不能，只有说照常可写（成待发，那台接上照原编号重交）。
pub fn unseen(offline: Words) -> SessionFace {
    let tip = offline;
    let no = |why: &Words| Allow::No { why: why.clone() };
    SessionFace {
        dot: SessionDot::Unknown,
        life: Life::Unclear,
        settled: false,
        name: Words(copy_text("runs.state.unknown", &[])),
        line: tip.clone(),
        tone: Tone::Plain,
        order: 3,
        account: None,
        clock: None,
        peek: None,
        can: SessionCan {
            close: CanClose::Menu,
            resume: Allow::No {
                why: Words(copy_text("sessionState.unseen.noResume", &[])),
            },
            front: no(&tip),
            attach: no(&tip),
            view: CanView::No { why: tip.clone() },
            reattach: no(&tip),
            model: no(&tip),
            permission: no(&tip),
            stop: no(&tip),
            end: no(&tip),
            say: CanSay::Yes {
                hint: Words(copy_text("composer.input.offline", &[])),
            },
        },
        tip,
    }
}

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
    /// 走了的：那台答的去向（离开那一帧：一生在 `face.life`）。
    pub left: BTreeMap<String, SessionLeft>,
    /// 活会话的最新运行表（会话走了 ⇒ 摘）。
    pub runs: BTreeMap<String, SessionRuns>,
    /// 被顶替的会话 ⇒ 顶替它的那一条（离开那一帧 `face.life = ended {by}`：清空之后同一个进程另起的那一份记录）。
    pub successors: BTreeMap<String, String>,
    /// 每条会话的记录被从头重读过几回（`session_file_reread`）：数变了 ⇒ 读过的那一代作废、整份重来。
    #[cfg_attr(test, ts(type = "{ [key in string]: number }"))]
    pub rereads: BTreeMap<String, u64>,
    /// 每条会话的记录文件最近怎么了，那台写好的那一句（`session_file_gone` · `session_file_reread` 带的 `said`）：后来的盖前面的，出口照摆。
    #[cfg_attr(test, ts(type = "{ [key in string]: string }"))]
    pub file_said: BTreeMap<String, Words>,
    /// 那台说「活会话清单报完了」（`sessions_replayed`）。
    pub listed: bool,
    /// 那台此刻看不见（[`SessionBook::unseen_now`] 记的那一张，[`unseen`]）：簿里没有、又不是「看不见时照旧作数」的会话一律照它画；
    /// 接上之后那台报完活会话清单（`sessions_replayed`）摘掉。看得见 ⇒ 不在。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub unseen: Option<SessionFace>,
}

/// 簿里一条会话此刻是哪一种（按 sid 的先后读整份用）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seat<'a> {
    Live(&'a SessionAdded),
    Left(&'a SessionLeft),
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
            Frame::SessionLeft(s) => {
                self.live.remove(&s.sid);
                self.runs.remove(&s.sid);
                if let Life::Ended { by: Some(by) } = &s.face.life {
                    self.successors.insert(s.sid.clone(), by.clone());
                }
                self.left.insert(s.sid.clone(), s.clone());
            }
            Frame::SessionFileReread(r) => {
                *self.rereads.entry(r.session_id.clone()).or_default() += 1;
                self.file_said.insert(r.session_id.clone(), r.said.clone());
            }
            Frame::SessionFileGone(g) => {
                self.file_said.insert(g.session_id.clone(), g.said.clone());
            }
            Frame::SessionsReplayed(_) => {
                self.listed = true;
                self.unseen = None;
            }
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

    /// **那台这一条连接断的那一刻**（全仓唯一的落法）：簿里活的 · 可重连的每一条落成说不清（[`unseen`]，`offline` 是那台的离线那一句），
    /// 自己先折进簿、记下那一张（[`SessionBook::unseen`]），交回那几帧（按 sid 排；要转出去的照它发）。已结束 · 记录已不在的不说。
    pub fn unseen_now(&mut self, offline: Words) -> Vec<SessionLeft> {
        let face = unseen(offline);
        let open: Vec<String> = self
            .seats()
            .filter(|(_, p)| match p {
                Seat::Live(_) => true,
                Seat::Left(s) => s.face.life == Life::Reconnectable,
            })
            .map(|(sid, _)| sid.clone())
            .collect();
        let left: Vec<SessionLeft> = open
            .into_iter()
            .map(|sid| SessionLeft {
                sid,
                face: face.clone(),
            })
            .collect();
        for l in &left {
            self.fold(&Frame::<serde_json::Value>::SessionLeft(l.clone()));
        }
        self.unseen = Some(face);
        left
    }

    /// 用户关掉一个已结束的 tab ⇒ 那一条不再记。
    pub fn forget(&mut self, sid: &str) {
        self.live.remove(sid);
        self.left.remove(sid);
        self.runs.remove(sid);
        self.file_said.remove(sid);
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/face-core/book_tests.rs"]
mod tests;
