//! 〔MIG-1 · `设计/99 §2.1 ⑬` · `01 §1.1`〕**monitor 手里「这条连接上那台后端说过的会话成品」** —— 纯缓存，不裁决。
//!
//! 活 / 可重连 / 已结束由那台机器的后端裁（`src/backend/observe/session_ledger.rs`，帧 `session_added` · `session_state` ·
//! `sessions_replayed`），本机远端同一形。monitor 这一侧只做三件事：
//! ① 两条流（远端 `ssh_source::stream_loop` · 本机 `ssh_source::consume_local`）把成品交进来（[`feed`]）；
//! ② 按原样转给前端（出口由 `lib.rs` 装：[`Out`] ⇒ 事件 ＋ 拉前那几样副作用）；
//! ③ 留一份最新成品，给 F5 重放（[`Book::replay`]）与本机骨架清单（[`Book::local_listed`]）用。
//!
//! 唯一一件 monitor 自己知道、后端不知道的事是**到那台的连接断了**（[`In::LinkLost`]）：那台的成品随之作废（整份摘掉），
//! 当时还活的 / 可重连的交出去说「说不清」（`设计/30 §3.5.7a`：不许显示成已结束）。重连之后那台的新连接自己重报一遍。
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
    pub cwd: Option<String>,
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
    /// 那台看不见了：当时还活的 / 可重连的那几条（排好序）。
    Unseen {
        origin: String,
        sids: Vec<String>,
    },
}

#[derive(Debug, Default)]
struct OriginBook {
    sessions: BTreeMap<String, Product>,
    listed: bool,
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
                    sid,
                    status,
                    waiting_for,
                }]
            }
            In::Left { origin, sid, fate } => {
                self.origins
                    .entry(origin.clone())
                    .or_default()
                    .sessions
                    .insert(sid.clone(), Product::Left(fate));
                vec![Out::Left { origin, sid, fate }]
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
        }
    }

    /// 本机的活会话清单 —— 报完了才给（`None` = 还没报完；不交半截的，`设计/30 §3.5.7a`）。按 (cwd, sid) 排。
    pub fn local_listed(&self) -> Option<Vec<(String, LiveMeta)>> {
        let b = self.origins.get(crate::origin::LOCAL)?;
        if !b.listed {
            return None;
        }
        let mut v: Vec<(String, LiveMeta)> = b
            .sessions
            .iter()
            .filter_map(|(s, p)| match p {
                Product::Live(m) => Some((s.clone(), m.clone())),
                Product::Left(_) => None,
            })
            .collect();
        v.sort_by(|a, b| (&a.1.cwd, &a.0).cmp(&(&b.1.cwd, &b.0)));
        Some(v)
    }

    /// 本机活会话的红绿灯（前端起步 / F5 拉一次做初始收敛）。
    pub fn local_activity(&self) -> Vec<(String, Option<String>, Option<String>)> {
        self.origins
            .get(crate::origin::LOCAL)
            .map(|b| {
                b.sessions
                    .iter()
                    .filter_map(|(s, p)| match p {
                        Product::Live(m) => {
                            Some((s.clone(), m.status.clone(), m.waiting_for.clone()))
                        }
                        Product::Left(_) => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// **F5 重放计划**（纯）：`buffered` = 重放缓冲里有行的 `(sid, 它所在的那台)`。
    ///
    /// - `before`：活会话重宣告（本机骨架由 [`Self::local_listed`] 那一问建，出口对本机那几条只重发容器）；
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
                    Product::Live(m) => r.before.push(Out::Live {
                        origin: (*o).clone(),
                        sid: sid.clone(),
                        meta: m.clone(),
                    }),
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
        for (sid, origin) in buffered {
            let b = self.origins.get(origin);
            match b.and_then(|b| b.sessions.get(sid)) {
                Some(Product::Live(_)) | Some(Product::Left(Fate::Reconnectable)) => {}
                Some(Product::Left(Fate::Ended)) => r.after.push(Out::Left {
                    origin: origin.clone(),
                    sid: sid.clone(),
                    fate: Fate::Ended,
                }),
                None if b.is_some_and(|b| b.listed) => r.after.push(Out::Left {
                    origin: origin.clone(),
                    sid: sid.clone(),
                    fate: Fate::Ended,
                }),
                None => unseen.entry(origin.clone()).or_default().push(sid.clone()),
            }
        }
        for (origin, sids) in unseen {
            r.after.push(Out::Unseen { origin, sids });
        }
        for o in origins {
            if self.origins[o].listed {
                r.after.push(Out::Listed { origin: o.clone() });
            }
        }
        r
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
#[path = "../../../tests/bridge/session_book_tests.rs"]
mod tests;
