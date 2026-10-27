//! **monitor 手里「这条连接上那台后端说过的会话成品」** —— 纯缓存，不裁决。
//!
//! 活 / 可重连 / 已结束由那台机器的后端裁（`src/backend/observe/session_ledger.rs`，帧 `session_added` · `session_left` ·
//! `sessions_replayed`），本机远端同一形。monitor 这一侧只做三件事：
//! ① 收的那一头（`stream_source::consume`：本机那条流 · 本机后端转来的各台）把成品交进来（[`feed`]）；
//! ② 按原样转给前端（出口由 `lib.rs` 装：[`Out`] ⇒ 事件 ＋ 拉前那几样副作用）；
//! ③ 留一份最新成品，给 F5 / 开窗的重放用（[`Book::replay`]，就绪点在会话流里原位交）。
//!
//! 唯一一件 monitor 自己知道、后端不知道的事是**到那台的连接断了**（[`In::LinkLost`]）：那台的成品随之作废（整份摘掉），
//! 当时还活的 / 可重连的交出去说「说不清」（不许显示成已结束）。重连之后那台的新连接自己重报一遍。
//!
//! 裁决一条都不在这一侧。**每台那一本就是端核心的会话簿**（[`SessionBook`]，手机核心同一份）：会话帧原样交进来（[`In::Told`]），
//! 折法只调 [`SessionBook::fold`]，这一侧不另写一份。

use parking_lot::RwLock;
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use cmd_wire::frame::{Frame, Life, SessionAdded, SessionLeft, SessionRuns, SessionStatus};
use resident_client::book::{Seat, SessionBook};

/// 两条流交进来的一件事。`origin` 是线上串（本机 `<local>`）。
#[derive(Debug, Clone, PartialEq)]
pub enum In {
    /// 那台说的一帧会话帧（`session_added` · `session_status` · `session_left` · `session_file_reread` ·
    /// `sessions_replayed` · `session_runs`），原样交进来。
    Told {
        origin: String,
        frame: crate::stream_source::Frame,
    },
    /// 到那台的连接断了。
    LinkLost { origin: String },
    /// 本机后端握上手时 hello 带来的那一张「那台看不见」的脸（`Hello.unseen`）：此后每一格「说不清」都带上它。
    UnseenFace {
        face: Option<cmd_wire::frame::SessionFace>,
    },
    /// `session_branch`：那台后端给的一个会话的主线外清单的一截 ——「那份记录（`path`）的清单前 `at` 条照旧，其后换成 `off`」
    /// （记录 `id`，不解释）。账照它拼成整份，交出去的是整份。`sum` ＝ 那台铸的「到这一帧末尾为止」那一截的摘要（只记、在 `stream-watch` 里交回）。
    Branch {
        origin: crate::origin::Origin,
        sid: String,
        path: String,
        at: u64,
        off: Vec<String>,
        sum: String,
    },
}

/// 交给出口的一件事（顺序就是意义：同一条流上的先后原样保留）。
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    Live {
        origin: String,
        sid: String,
        meta: SessionAdded,
    },
    Status {
        origin: String,
        status: SessionStatus,
    },
    Left {
        origin: String,
        left: SessionLeft,
    },
    /// `all` = 这一刻机器表里的每一台都报完了（「各台都报完」那一刻：前端据此收空组、说记住的那一格没出现）。
    Listed {
        origin: String,
        all: bool,
    },
    /// 那台看不见了（**机器级**：线上只一格 `unseen {origin}`，前端按机器落说不清）。
    /// `sids` = 这一刻落说不清的那几条（排好序），只给 monitor 自己的旁路账用（本机那两份缓存），不上线。
    Unseen {
        origin: String,
        sids: Vec<String>,
        /// 那几条落成的那一张：本机后端交来的那一份（还没交 ⇒ `None`，界面不画点）。
        face: Option<cmd_wire::frame::SessionFace>,
    },
    /// 一个会话的运行表（原样转）。
    Runs {
        origin: String,
        runs: SessionRuns,
    },
    /// 一个会话的主线外清单（拼好的整份）。
    Branch {
        origin: crate::origin::Origin,
        sid: String,
        off: Vec<String>,
    },
}

/// 手上一个会话的主线外清单：哪份记录的 · 拼好的整份 · 那台给的摘要（`None` ＝ 拼的时候中间缺过一截，说不准 ⇒ 下次要整份）。
#[derive(Debug, Default, Clone)]
struct BranchCopy {
    path: String,
    off: Vec<String>,
    sum: Option<String>,
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
    /// 每台那一本（端核心的会话簿；运行表也在里面，F5 重放跟在宣告后面再说一次）。
    origins: HashMap<String, SessionBook>,
    /// 每台每个会话手上那份主线外清单（F5 重放跟在宣告后面再说一次 · 报名单时交回手上有什么）。
    /// **断线不摘**（不跟 `origins` 那一台一起走）：重接之后交回它，那台只补缺的那一截。会话离开 · 关 tab · 那台报完清单时不在了 ⇒ 摘。
    branch: HashMap<crate::origin::Origin, BTreeMap<String, BranchCopy>>,
    /// 订了会话流的那几台（机器表：本机 ＋ 注册着的远端）；`None` = 还不知道 ⇒ 不立「各台都报完」。
    machines: Option<Vec<String>>,
    /// 本机后端交来的那一张「那台看不见」的脸（[`In::UnseenFace`]）。
    unseen: Option<cmd_wire::frame::SessionFace>,
}

impl Book {
    /// **纯**：一件事 × 这本账 ⇒ 账怎么变 ＋ 交给出口什么。
    pub fn step(&mut self, ev: In) -> Vec<Out> {
        match ev {
            In::Told { origin, frame } => {
                if !self.origins.entry(origin.clone()).or_default().fold(&frame) {
                    return Vec::new();
                }
                self.told(origin, frame)
            }
            In::Branch {
                origin,
                sid,
                path,
                at,
                off,
                sum,
            } => {
                let held = self
                    .branch
                    .entry(origin.clone())
                    .or_default()
                    .entry(sid.clone())
                    .or_default();
                // 换了一份记录 ⇒ 从空的起拼（那一份从 `at = 0` 发起）。
                if held.path != path {
                    *held = BranchCopy {
                        path,
                        ..BranchCopy::default()
                    };
                }
                let at = usize::try_from(at).unwrap_or(usize::MAX);
                // 「前 `at` 条照旧」而手上没有那么多 ⇒ 中间缺过一截：拼出来的不是那台那份，摘要不记（下次交 `at: 0` 要整份）。
                let whole = at <= held.off.len();
                held.off.truncate(at);
                held.off.extend(off);
                held.sum = whole.then_some(sum);
                vec![Out::Branch {
                    origin,
                    sid,
                    off: held.off.clone(),
                }]
            }
            In::UnseenFace { face } => {
                self.unseen = face;
                Vec::new()
            }
            In::LinkLost { origin } => {
                let Some(b) = self.origins.remove(&origin) else {
                    return Vec::new();
                };
                let sids: Vec<String> = b
                    .seats()
                    .filter(|(_, p)| !matches!(p, Seat::Left(s) if !reconnectable(s)))
                    .map(|(s, _)| s.clone())
                    .collect();
                if sids.is_empty() {
                    return Vec::new();
                }
                vec![Out::Unseen {
                    origin,
                    sids,
                    face: self.unseen.clone(),
                }]
            }
        }
    }

    /// 那台那一本刚折进一帧会话帧 ⇒ 交给出口什么（帧原样；清单报完了带「各台都报完」）。
    fn told(&mut self, origin: String, frame: crate::stream_source::Frame) -> Vec<Out> {
        match frame {
            Frame::SessionAdded(meta) => vec![Out::Live {
                origin,
                sid: meta.sid.clone(),
                meta,
            }],
            Frame::SessionStatus(status) => vec![Out::Status { origin, status }],
            Frame::SessionLeft(left) => {
                if let Some(m) = self
                    .branch
                    .get_mut(&crate::origin::Origin::named(origin.clone()))
                {
                    m.remove(&left.sid);
                }
                vec![Out::Left { origin, left }]
            }
            Frame::SessionRuns(runs) => vec![Out::Runs { origin, runs }],
            Frame::SessionsReplayed(_) => {
                // 清单报完了：那台此刻不在的会话，手上那份清单没用了。
                let live = &self.origins[&origin].live;
                if let Some(m) = self
                    .branch
                    .get_mut(&crate::origin::Origin::named(origin.clone()))
                {
                    m.retain(|sid, _| live.contains_key(sid));
                }
                let all = self.all_listed();
                vec![Out::Listed { origin, all }]
            }
            // 顶替 · 重读只记进簿（内容那一路由收口说）。
            _ => Vec::new(),
        }
    }

    /// 那台那一本（判据 · 两端对拍读）。
    #[cfg(test)]
    pub(crate) fn of(&self, origin: &str) -> Option<&SessionBook> {
        self.origins.get(origin)
    }

    /// 报名单时交回的那一格（`stream-watch.branch`）：那台这几个会话的主线外清单手上各有什么（没有 / 说不准 ⇒ `at: 0`）。
    pub fn branch_held(
        &self,
        origin: &crate::origin::Origin,
        sids: &std::collections::BTreeSet<String>,
    ) -> Vec<cmd_wire::watch::BranchHeld> {
        let mine = self.branch.get(origin);
        sids.iter()
            .map(|sid| match mine.and_then(|m| m.get(sid)) {
                Some(BranchCopy {
                    path,
                    off,
                    sum: Some(sum),
                }) => cmd_wire::watch::BranchHeld {
                    sid: sid.clone(),
                    path: Some(path.clone()),
                    at: off.len() as u64,
                    sum: Some(sum.clone()),
                },
                _ => cmd_wire::watch::BranchHeld {
                    sid: sid.to_string(),
                    ..Default::default()
                },
            })
            .collect()
    }

    /// 机器表里的每一台都报完了活会话清单（表还不知道 / 空 ⇒ 否）。
    fn all_listed(&self) -> bool {
        self.machines.as_ref().is_some_and(|m| {
            !m.is_empty()
                && m.iter()
                    .all(|o| self.origins.get(o).is_some_and(|b| b.listed))
        })
    }

    /// 机器表换了（启动 · 热加载）：记下；此刻每一台都报完了 ⇒ 再说一次「报完了」带 `all`（那一刻不因为表变了而错过，
    /// 例如摘掉的正是唯一没报完的那台）。
    pub fn set_machines(&mut self, machines: Vec<String>) -> Vec<Out> {
        self.machines = Some(machines);
        if !self.all_listed() {
            return Vec::new();
        }
        let first = self.machines.as_ref().and_then(|m| m.first()).cloned();
        first
            .map(|origin| vec![Out::Listed { origin, all: true }])
            .unwrap_or_default()
    }

    /// 那台那个会话此刻是活的（它最近的成品是 `Live`）。
    pub fn is_live(&self, origin: &crate::origin::Origin, sid: &str) -> bool {
        self.origins
            .get(&origin.to_string())
            .is_some_and(|b| b.live.contains_key(sid))
    }

    /// 用户关掉一个已结束的 tab（`EventReplay::forget` 同一刻）⇒ 它的成品也忘掉（不再重放）。
    pub fn forget(&mut self, sid: &str) {
        for b in self.origins.values_mut() {
            b.forget(sid);
        }
        for m in self.branch.values_mut() {
            m.remove(sid);
        }
    }

    /// **F5 重放计划**（纯）：`kept` = 手上留着的 `(sid, 它所在的那台)`（留存里有行的 · 主窗口的 tab）。
    ///
    /// - `before`：活会话重宣告（骨架 ＋ 初始灯 ＋ 容器，本机远端同一形）；
    /// - `after`：全部可重连的 · 手上留着却不活的（这条连接上说过已结束 ⇒ 已结束；没说过 ⇒ 那台没报完清单 ⇒ 说不清，报完了 ⇒ 不说，那台答）·
    ///   报完了清单的那几台再说一次「报完了」（排在最后：前端处理它时活着的已经翻回活）。
    pub fn replay(&self, kept: &[(String, String)]) -> Replay {
        let mut r = Replay::default();
        let mut origins: Vec<&String> = self.origins.keys().collect();
        origins.sort();
        for o in &origins {
            let b = &self.origins[*o];
            for (sid, p) in b.seats() {
                match p {
                    Seat::Live(m) => {
                        r.before.push(Out::Live {
                            origin: (*o).clone(),
                            sid: sid.clone(),
                            meta: m.clone(),
                        });
                        if let Some(runs) = b.runs.get(sid) {
                            r.before.push(Out::Runs {
                                origin: (*o).clone(),
                                runs: runs.clone(),
                            });
                        }
                        let at = crate::origin::Origin::named((*o).clone());
                        if let Some(c) = self.branch.get(&at).and_then(|m| m.get(sid)) {
                            r.before.push(Out::Branch {
                                origin: at,
                                sid: sid.clone(),
                                off: c.off.clone(),
                            });
                        }
                    }
                    Seat::Left(s) if reconnectable(s) => r.after.push(Out::Left {
                        origin: (*o).clone(),
                        left: s.clone(),
                    }),
                    Seat::Left(_) => {}
                }
            }
        }
        let mut unseen: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut settled = Vec::new();
        for (sid, origin) in kept {
            let b = self.origins.get(origin);
            match b.and_then(|b| b.seat(sid)) {
                Some(Seat::Live(_)) => {}
                Some(Seat::Left(s)) if reconnectable(s) => {}
                Some(Seat::Left(s)) => settled.push(Out::Left {
                    origin: origin.clone(),
                    left: s.clone(),
                }),
                // 那台报完了清单、这条没被说过 ⇒ 不说：那台后端照手上留着的答它的去向（答过就在账里）。
                None if b.is_some_and(|b| b.listed) => {}
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
        let all = self.all_listed();
        for o in origins {
            if self.origins[o].listed {
                r.after.push(Out::Listed {
                    origin: o.clone(),
                    all,
                });
            }
        }
        r
    }
}

impl Out {
    /// 这件事说的是哪台机器。
    pub fn origin(&self) -> String {
        match self {
            Out::Live { origin, .. }
            | Out::Status { origin, .. }
            | Out::Left { origin, .. }
            | Out::Listed { origin, .. }
            | Out::Unseen { origin, .. }
            | Out::Runs { origin, .. } => origin.clone(),
            Out::Branch { origin, .. } => origin.to_string(),
        }
    }

    /// 这件事 ⇒ 会话流里的一格（前端照原样收；起停那几种就是那台的帧）。
    pub fn frame(&self) -> crate::ui_contract::SessionStreamFrame {
        use crate::ui_contract::{self as b, SessionStreamFrame as F};
        match self {
            Out::Live { meta, .. } => F::Added(meta.clone()),
            Out::Status { status, .. } => F::Status(status.clone()),
            Out::Left { left, .. } => F::Left(left.clone()),
            Out::Listed { origin, all } => F::Listed(b::OriginSessionsListedPayload {
                origin: crate::origin::Origin::named(origin.clone()),
                all: *all,
            }),
            Out::Unseen { origin, face, .. } => F::Unseen(b::SessionUnseenPayload {
                origin: crate::origin::Origin::named(origin.clone()),
                face: face.clone(),
            }),
            Out::Runs { runs, .. } => F::Runs(runs.clone()),
            Out::Branch { sid, off, .. } => F::Branch(b::SessionBranchPayload {
                sid: sid.clone(),
                off: off.clone(),
            }),
        }
    }
}

impl Book {
    /// 连接还在、却说不清某几条（那台还没报完清单）时的那一段：机器级 `unseen` ＋ 那台说过的活会话 / 可重连的再说一次
    /// （前端按机器落说不清会把它们一并落下，紧跟着翻回来）。
    fn unseen_block(&self, origin: &str, sids: Vec<String>) -> Vec<Out> {
        let mut v = vec![Out::Unseen {
            origin: origin.to_string(),
            sids,
            face: self.unseen.clone(),
        }];
        if let Some(b) = self.origins.get(origin) {
            for (sid, p) in b.seats() {
                match p {
                    Seat::Live(m) => v.push(Out::Live {
                        origin: origin.to_string(),
                        sid: sid.clone(),
                        meta: m.clone(),
                    }),
                    Seat::Left(s) if reconnectable(s) => v.push(Out::Left {
                        origin: origin.to_string(),
                        left: s.clone(),
                    }),
                    Seat::Left(_) => {}
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

/// 机器表换了（`machine_table::refresh` 那一处调）：记账，有话就交出口。
pub fn machines_changed(machines: Vec<String>) {
    let outs = book().write().set_machines(machines);
    send(outs);
}

/// 两条流交来一件事：记账，按序交出口。出口还没装 ⇒ 只记账（前端那时还没在听；F5 / 起步那一问会从账里重放）。
pub fn feed(ev: In) {
    let outs = book().write().step(ev);
    send(outs);
}

fn send(outs: Vec<Out>) {
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

/// 走了的那一条接得回去（那台说可重连）。
fn reconnectable(s: &SessionLeft) -> bool {
    s.face.life == Life::Reconnectable
}
