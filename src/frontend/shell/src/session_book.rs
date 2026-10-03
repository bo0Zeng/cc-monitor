//! **monitor 手里「这条连接上那台后端说过的会话成品」** —— 纯缓存，不裁决。
//!
//! 活 / 可重连 / 已结束由那台机器的后端裁（`src/backend/observe/session_ledger.rs`，帧 `session_added` · `session_state` ·
//! `sessions_replayed`），本机远端同一形。monitor 这一侧只做三件事：
//! ① 两条流（远端 `ssh_source::stream_loop` · 本机 `ssh_source::consume_local`）把成品交进来（[`feed`]）；
//! ② 按原样转给前端（出口由 `lib.rs` 装：[`Out`] ⇒ 事件 ＋ 拉前那几样副作用）；
//! ③ 留一份最新成品，给 F5 / 开窗的重放用（[`Book::replay`]，就绪点在会话流里原位交）。
//!
//! 唯一一件 monitor 自己知道、后端不知道的事是**到那台的连接断了**（[`In::LinkLost`]）：那台的成品随之作废（整份摘掉），
//! 当时还活的 / 可重连的交出去说「说不清」（不许显示成已结束）。重连之后那台的新连接自己重报一遍。
//!
//! 原先住在这一侧的那一套裁决（tmux 原文账 · idle 账 · `classify_removed` · 两份收割 · 重连后重新裁 · 本机活会话表）已删。

use parking_lot::RwLock;
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

/// 一条活会话住在什么容器里（后端 `session_added.container`；判不了的不在这里，缺席 = 不知道）。
///
/// 线上两个字面量与后端 `wire::SessionContainer` 逐字一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Container {
    Tmux,
    None,
}

pub const CONTAINER_TMUX: &str = "tmux";
pub const CONTAINER_NONE: &str = "none";

impl Container {
    /// 不认识的取值当不知道（`None`）—— 宁可说「没报」，不许凭一个不认识的词说「不在 tmux 里」。
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            CONTAINER_TMUX => Some(Container::Tmux),
            CONTAINER_NONE => Some(Container::None),
            _ => None,
        }
    }

    pub fn as_wire(self) -> &'static str {
        match self {
            Container::Tmux => CONTAINER_TMUX,
            Container::None => CONTAINER_NONE,
        }
    }
}

/// 后端 `session_state.state`：离开「活」之后是什么。线上两个字面量与后端 `wire::SessionFate` 逐字一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fate {
    Reconnectable,
    Ended,
}

pub const FATE_RECONNECTABLE: &str = "reconnectable";
pub const FATE_ENDED: &str = "ended";

impl Fate {
    /// 不认识的取值 ⇒ `None`（那一帧当坏帧跳过，不猜）。
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            FATE_RECONNECTABLE => Some(Fate::Reconnectable),
            FATE_ENDED => Some(Fate::Ended),
            _ => None,
        }
    }
}

/// 一条活会话宣告时带来的那几格（后端 `session_added`）。之后按 `session_status` 更新灯。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LiveMeta {
    /// `session_kind`（"interactive" / "bg"；旧 CC 缺）。
    pub kind: Option<String>,
    pub attachable: Option<bool>,
    /// pidfile 记的起会话目录（认「我刚起的那条」用）。
    pub cwd: Option<String>,
    /// 会话的项目目录（那台后端给的；tab 标题用它）。
    pub project_dir: Option<String>,
    pub name: Option<String>,
    pub status: Option<String>,
    pub waiting_for: Option<String>,
    pub container: Option<Container>,
    /// 那个 claude 进程的 pid（本机 ↗ 绑窗口用；老后端 / 没索要 ⇒ `None`）。
    pub pid: Option<u32>,
}

/// 一条会话此刻的最新成品。
#[derive(Debug, Clone, PartialEq)]
pub enum Product {
    Live(LiveMeta),
    Left(Fate),
}

/// 两条流交进来的一件事。`origin` 是线上串（本机 `<local>`）。
#[derive(Debug, Clone, PartialEq)]
pub enum In {
    Live {
        origin: String,
        sid: String,
        meta: LiveMeta,
    },
    Status {
        origin: String,
        sid: String,
        status: Option<String>,
        waiting_for: Option<String>,
    },
    Left {
        origin: String,
        sid: String,
        fate: Fate,
    },
    /// `sessions_replayed`：那台的活会话清单报完了（可重连的也已报过）。
    Listed { origin: String },
    /// 到那台的连接断了。
    LinkLost { origin: String },
    /// `session_runs`：那台后端给的一个会话的运行表 ＋ 被挤出表的已收场那几个（JSON 数组原文，不解释）。
    Runs {
        origin: String,
        sid: String,
        runs: crate::ui_contract::RecordBody,
        ended: crate::ui_contract::RecordBody,
    },
}

/// 交给出口的一件事（顺序就是意义：同一条流上的先后原样保留）。
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    Live {
        origin: String,
        sid: String,
        meta: LiveMeta,
    },
    Status {
        origin: String,
        sid: String,
        status: Option<String>,
        waiting_for: Option<String>,
    },
    Left {
        origin: String,
        sid: String,
        fate: Fate,
    },
    Listed {
        origin: String,
    },
    /// 那台看不见了（**机器级**：线上只一格 `unseen {origin}`，前端按机器落说不清）。
    /// `sids` = 这一刻落说不清的那几条（排好序），只给 monitor 自己的旁路账用（本机那两份缓存），不上线。
    Unseen {
        origin: String,
        sids: Vec<String>,
    },
    /// 一个会话的运行表（原样转）。
    Runs {
        origin: String,
        sid: String,
        runs: crate::ui_contract::RecordBody,
        ended: crate::ui_contract::RecordBody,
    },
}

#[derive(Debug, Default)]
struct OriginBook {
    sessions: BTreeMap<String, Product>,
    listed: bool,
    /// 活会话的最新运行表（F5 重放跟在它的宣告后面再说一次）；会话离开 ⇒ 摘。
    runs: BTreeMap<
        String,
        (
            crate::ui_contract::RecordBody,
            crate::ui_contract::RecordBody,
        ),
    >,
}

/// F5 重放那一趟要发的东西：骨架先（`before`，在留存行之前），终局后（`after`，在留存行之后 —— 否则远端行会把刚归档的 tab 翻活）。
#[derive(Debug, Default, PartialEq)]
pub struct Replay {
    pub before: Vec<Out>,
    pub after: Vec<Out>,
}

/// 全部机器的成品账（见模块头注）。
#[derive(Debug, Default)]
pub struct Book {
    origins: HashMap<String, OriginBook>,
}

impl Book {
    /// **纯**：一件事 × 这本账 ⇒ 账怎么变 ＋ 交给出口什么。
    pub fn step(&mut self, ev: In) -> Vec<Out> {
        match ev {
            In::Live { origin, sid, meta } => {
                self.origins
                    .entry(origin.clone())
                    .or_default()
                    .sessions
                    .insert(sid.clone(), Product::Live(meta.clone()));
                vec![Out::Live { origin, sid, meta }]
            }
            In::Status {
                origin,
                sid,
                status,
                waiting_for,
            } => {
                if let Some(Product::Live(m)) = self
                    .origins
                    .get_mut(&origin)
                    .and_then(|b| b.sessions.get_mut(&sid))
                {
                    m.status = status.clone();
                    m.waiting_for = waiting_for.clone();
                }
                vec![Out::Status {
                    origin,
                    sid,
                    status,
                    waiting_for,
                }]
            }
            In::Left { origin, sid, fate } => {
                let b = self.origins.entry(origin.clone()).or_default();
                b.sessions.insert(sid.clone(), Product::Left(fate));
                b.runs.remove(&sid);
                vec![Out::Left { origin, sid, fate }]
            }
            In::Runs {
                origin,
                sid,
                runs,
                ended,
            } => {
                self.origins
                    .entry(origin.clone())
                    .or_default()
                    .runs
                    .insert(sid.clone(), (runs.clone(), ended.clone()));
                vec![Out::Runs {
                    origin,
                    sid,
                    runs,
                    ended,
                }]
            }
            In::Listed { origin } => {
                self.origins.entry(origin.clone()).or_default().listed = true;
                vec![Out::Listed { origin }]
            }
            In::LinkLost { origin } => {
                let Some(b) = self.origins.remove(&origin) else {
                    return Vec::new();
                };
                let sids: Vec<String> = b
                    .sessions
                    .into_iter()
                    .filter(|(_, p)| !matches!(p, Product::Left(Fate::Ended)))
                    .map(|(s, _)| s)
                    .collect();
                if sids.is_empty() {
                    return Vec::new();
                }
                vec![Out::Unseen { origin, sids }]
            }
        }
    }

    /// 用户关掉一个已结束的 tab（`EventReplay::forget` 同一刻）⇒ 它的成品也忘掉（不再重放）。
    pub fn forget(&mut self, sid: &str) {
        for b in self.origins.values_mut() {
            b.sessions.remove(sid);
            b.runs.remove(sid);
        }
    }

    /// **F5 重放计划**（纯）：`buffered` = 重放缓冲里有行的 `(sid, 它所在的那台)`。
    ///
    /// - `before`：活会话重宣告（骨架 ＋ 初始灯 ＋ 容器，本机远端同一形）；
    /// - `after`：全部可重连的 · 有行却不活的（这条连接上说过已结束 ⇒ 已结束；没说过 ⇒ 那台报完了清单 ⇒ 已结束，没报完 ⇒ 说不清）·
    ///   报完了清单的那几台再说一次「报完了」（排在最后：前端处理它时活着的已经翻回活）。
    pub fn replay(&self, buffered: &[(String, String)]) -> Replay {
        let mut r = Replay::default();
        let mut origins: Vec<&String> = self.origins.keys().collect();
        origins.sort();
        for o in &origins {
            let b = &self.origins[*o];
            for (sid, p) in &b.sessions {
                match p {
                    Product::Live(m) => {
                        r.before.push(Out::Live {
                            origin: (*o).clone(),
                            sid: sid.clone(),
                            meta: m.clone(),
                        });
                        if let Some((runs, ended)) = b.runs.get(sid) {
                            r.before.push(Out::Runs {
                                origin: (*o).clone(),
                                sid: sid.clone(),
                                runs: runs.clone(),
                                ended: ended.clone(),
                            });
                        }
                    }
                    Product::Left(Fate::Reconnectable) => r.after.push(Out::Left {
                        origin: (*o).clone(),
                        sid: sid.clone(),
                        fate: Fate::Reconnectable,
                    }),
                    _ => {}
                }
            }
        }
        let mut unseen: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut settled = Vec::new();
        for (sid, origin) in buffered {
            let b = self.origins.get(origin);
            match b.and_then(|b| b.sessions.get(sid)) {
                Some(Product::Live(_)) | Some(Product::Left(Fate::Reconnectable)) => {}
                Some(Product::Left(Fate::Ended)) => settled.push(Out::Left {
                    origin: origin.clone(),
                    sid: sid.clone(),
                    fate: Fate::Ended,
                }),
                None if b.is_some_and(|b| b.listed) => settled.push(Out::Left {
                    origin: origin.clone(),
                    sid: sid.clone(),
                    fate: Fate::Ended,
                }),
                None => unseen.entry(origin.clone()).or_default().push(sid.clone()),
            }
        }
        // 机器级「说不清」排在终局最前：它把那台上活的 / 可重连的一并落说不清（前端按机器落），
        //   紧跟着把那台说过的活会话再宣告一次 ⇒ 真活着的翻回活；可重连的就在下面、照常落回可重连。
        let mut front = Vec::new();
        for (origin, sids) in unseen {
            front.extend(self.unseen_block(&origin, sids));
        }
        r.after.splice(0..0, front);
        r.after.extend(settled);
        for o in origins {
            if self.origins[o].listed {
                r.after.push(Out::Listed { origin: o.clone() });
            }
        }
        r
    }
}

impl Out {
    /// 这件事说的是哪台机器。
    pub fn origin(&self) -> &str {
        match self {
            Out::Live { origin, .. }
            | Out::Status { origin, .. }
            | Out::Left { origin, .. }
            | Out::Listed { origin }
            | Out::Unseen { origin, .. }
            | Out::Runs { origin, .. } => origin,
        }
    }

    /// 这件事 ⇒ 会话流里的几格（前端照原样收）。活会话 = `live` ＋ 初始灯 ＋ 容器（判不了的不发）。
    pub fn frames(&self) -> Vec<crate::ui_contract::SessionStreamFrame> {
        use crate::ui_contract::{self as b, SessionStreamFrame as F};
        match self {
            Out::Live { origin, sid, meta } => {
                let mut v = vec![
                    F::Live(b::SessionLivePayload {
                        session_id: sid.clone(),
                        origin: origin.clone(),
                        kind: meta.kind.clone(),
                        attachable: meta.attachable,
                        cwd: meta.cwd.clone(),
                        project_dir: meta.project_dir.clone(),
                        name: meta.name.clone(),
                    }),
                    F::Activity(b::SessionActivityPayload {
                        session_id: sid.clone(),
                        status: meta.status.clone(),
                        waiting_for: meta.waiting_for.clone(),
                    }),
                ];
                if let Some(c) = meta.container {
                    v.push(F::Container(b::SessionContainerPayload {
                        session_id: sid.clone(),
                        container: c.as_wire().to_string(),
                    }));
                }
                v
            }
            Out::Status {
                sid,
                status,
                waiting_for,
                ..
            } => vec![F::Activity(b::SessionActivityPayload {
                session_id: sid.clone(),
                status: status.clone(),
                waiting_for: waiting_for.clone(),
            })],
            Out::Left { sid, fate, .. } => vec![match fate {
                Fate::Reconnectable => F::Idle(b::SessionIdlePayload {
                    session_id: sid.clone(),
                }),
                Fate::Ended => F::Ended(b::SessionEndedPayload {
                    session_id: sid.clone(),
                }),
            }],
            Out::Listed { origin } => vec![F::Listed(b::OriginSessionsListedPayload {
                origin: crate::origin::Origin(origin.clone()),
            })],
            Out::Unseen { origin, .. } => vec![F::Unseen(b::SessionUnseenPayload {
                origin: crate::origin::Origin(origin.clone()),
            })],
            Out::Runs {
                sid, runs, ended, ..
            } => vec![F::Runs(b::SessionRunsPayload {
                session_id: sid.clone(),
                runs: runs.clone(),
                ended: ended.clone(),
            })],
        }
    }
}

impl Book {
    /// 旁路快照被取消（会话离开了 / 连接断了）时那条会话**此刻**的终局 —— 快照行可能已经把刚落定的 tab 翻活，
    /// 原样再说一次它现在是什么（原先这里恒补「已结束」，那是 monitor 自己在裁）。活着 ⇒ 不说；这条连接上没说过 ⇒ 说不清。
    /// 没说过它、而那台已报完清单 ⇒ 不在清单里 = 已结束；还没报完 ⇒ 机器级说不清（见 [`Self::unseen_block`]）。
    pub fn settle_again(&self, origin: &str, sid: &str) -> Vec<Out> {
        let b = self.origins.get(origin);
        match b.and_then(|b| b.sessions.get(sid)) {
            Some(Product::Live(_)) => Vec::new(),
            Some(Product::Left(fate)) => vec![Out::Left {
                origin: origin.to_string(),
                sid: sid.to_string(),
                fate: *fate,
            }],
            None if b.is_some_and(|b| b.listed) => vec![Out::Left {
                origin: origin.to_string(),
                sid: sid.to_string(),
                fate: Fate::Ended,
            }],
            None => self.unseen_block(origin, vec![sid.to_string()]),
        }
    }

    /// 连接还在、却说不清某几条（那台还没报完清单）时的那一段：机器级 `unseen` ＋ 那台说过的活会话 / 可重连的再说一次
    /// （前端按机器落说不清会把它们一并落下，紧跟着翻回来）。
    fn unseen_block(&self, origin: &str, sids: Vec<String>) -> Vec<Out> {
        let mut v = vec![Out::Unseen {
            origin: origin.to_string(),
            sids,
        }];
        if let Some(b) = self.origins.get(origin) {
            for (sid, p) in &b.sessions {
                match p {
                    Product::Live(m) => v.push(Out::Live {
                        origin: origin.to_string(),
                        sid: sid.clone(),
                        meta: m.clone(),
                    }),
                    Product::Left(Fate::Reconnectable) => v.push(Out::Left {
                        origin: origin.to_string(),
                        sid: sid.clone(),
                        fate: Fate::Reconnectable,
                    }),
                    Product::Left(Fate::Ended) => {}
                }
            }
        }
        v
    }
}

static BOOK: OnceLock<RwLock<Book>> = OnceLock::new();
static SINK: OnceLock<std::sync::mpsc::Sender<Out>> = OnceLock::new();

/// 进程里那一本。
pub fn book() -> &'static RwLock<Book> {
    BOOK.get_or_init(Default::default)
}

/// 装出口（`lib.rs` 起步段调一次）。第二次调是用法错：大声说，不换。
pub fn install_sink(tx: std::sync::mpsc::Sender<Out>) {
    if SINK.set(tx).is_err() {
        tracing::error!("session_book::install_sink 被调了第二次 —— 出口只许有一个，这次不换");
    }
}

/// 两条流交来一件事：记账，按序交出口。出口还没装 ⇒ 只记账（前端那时还没在听；F5 / 起步那一问会从账里重放）。
pub fn feed(ev: In) {
    let outs = book().write().step(ev);
    let Some(tx) = SINK.get() else {
        return;
    };
    for o in outs {
        if let Err(e) = tx.send(o) {
            tracing::warn!("会话成品的出口收不了了：{e}");
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/session_book_tests.rs"]
mod tests;
