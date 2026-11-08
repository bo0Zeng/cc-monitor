//! **monitor 手里「这条连接上那台后端说过的会话成品」** —— 纯缓存，不裁决。
//!
//! 活 / 可重连 / 已结束由那台机器的后端裁（`src/backend/observe/session_ledger.rs`，帧 `session_added` · `session_left` ·
//! `sessions_replayed`），本机远端同一形。monitor 这一侧只做三件事：
//! ① 收的那一头（`stream_source::consume`：本机那条流 · 本机后端转来的各台）把成品交进来（[`feed`]）；
//! ② 按原样转给前端（出口由 `lib.rs` 装：[`Out`] ⇒ 事件 ＋ 拉前那几样副作用）；
//! ③ 留一份最新成品，给 F5 / 开窗的重放用（[`Book::replay`]，就绪点在会话流里原位交）。
//!
//! 连接断了那一刻也不在这一侧说：本机后端给那台活的 / 可重连的每一条发一帧 `session_left`（说不清），原样经 [`In::Told`] 进来；
//! 这一侧只在那条连接收了时把那一本「清单报完了」作废（[`In::Closed`]），下一条连接自己再报。
//!
//! 裁决一条都不在这一侧。**每台那一本就是共享核心的会话簿**（[`SessionBook`]，手机核心同一份）：会话帧原样交进来（[`In::Told`]），
//! 折法只调 [`SessionBook::fold`]，这一侧不另写一份。

use parking_lot::RwLock;
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use cmd_wire::frame::{
    BranchRun, Frame, Life, SessionAdded, SessionLeft, SessionRuns, SessionStatus,
};
use face_core::book::{Seat, SessionBook};

/// 两条流交进来的一件事。`origin` 是线上串（本机 `<local>`）。
#[derive(Debug, Clone, PartialEq)]
pub enum In {
    /// 那台说的一帧会话帧（`session_added` · `session_status` · `session_left` · `session_file_gone` · `session_file_reread` ·
    /// `sessions_replayed` · `session_runs`），原样交进来。
    Told {
        origin: String,
        frame: crate::stream_source::Frame,
    },
    /// 到那台的这一条连接收了：那一本「清单报完了」作废（下一条连接自己再报）；会话一条都不动 —— 说不清由本机后端发。
    Closed { origin: String },
    /// `session_branch`：那台后端给的一个会话的主线外清单的一截 ——「那份记录（`path`）的清单前 `at` 条照旧，其后换成 `off`」
    /// （记录 `id`，不解释）。账照它拼成整份，交出去的是整份。`sum` ＝ 那台铸的「到这一帧末尾为止」那一截的摘要（只记、在 `stream-watch` 里交回）。
    /// `runs` ＝ 段首落在这一帧里的段（那台分好的；`at` 恒是段首 ⇒ 起在它之前的段照旧，其后的换成这些，账不并段）。
    Branch {
        origin: crate::origin::Origin,
        sid: String,
        path: String,
        at: u64,
        off: Vec<String>,
        runs: Vec<BranchRun>,
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
    /// 那台的活会话清单报完了（「各台都报完」那一刻是本机后端算的 `FleetLines.allListed`，不在这一侧）。
    Listed {
        origin: String,
    },
    /// 一个会话的运行表（原样转）。
    Runs {
        origin: String,
        runs: SessionRuns,
    },
    /// 一个会话的主线外清单（拼好的整份）与它分成的段。
    Branch {
        origin: crate::origin::Origin,
        sid: String,
        off: Vec<String>,
        runs: Vec<BranchRun>,
    },
}

/// 手上一个会话的主线外清单：哪份记录的 · 拼好的整份与段 · 那台给的摘要（`None` ＝ 拼的时候中间缺过一截，说不准 ⇒ 下次要整份）。
#[derive(Debug, Default, Clone)]
struct BranchCopy {
    path: String,
    off: Vec<String>,
    runs: Vec<BranchRun>,
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
                runs,
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
                held.runs.retain(|r| r.at < at as u64);
                held.runs.extend(runs);
                held.sum = whole.then_some(sum);
                vec![Out::Branch {
                    origin,
                    sid,
                    off: held.off.clone(),
                    runs: held.runs.clone(),
                }]
            }
            In::Closed { origin } => {
                if let Some(b) = self.origins.get_mut(&origin) {
                    b.listed = false;
                }
                Vec::new()
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
                // 说不清（断线那一刻本机后端说的）不是走了：手上那份主线外清单留着，重接之后交回、那台只补缺的那一截。
                if left.face.life != Life::Unclear {
                    if let Some(m) = self
                        .branch
                        .get_mut(&crate::origin::Origin::named(origin.clone()))
                    {
                        m.remove(&left.sid);
                    }
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
                vec![Out::Listed { origin }]
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
                    ..
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
    /// - `after`：全部可重连的 · 手上留着却不活的（说过已结束 / 说不清 ⇒ 照那一帧；没说过 ⇒ 不说，那台答）·
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
                                runs: c.runs.clone(),
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
        for (sid, origin) in kept {
            // 走了的（已结束 · 说不清 …）照那一帧再说一次；没被说过 ⇒ 不说：那台后端照手上留着的答它的去向（答过就在账里）。
            if let Some(Seat::Left(s)) = self.origins.get(origin).and_then(|b| b.seat(sid)) {
                if !reconnectable(s) {
                    r.after.push(Out::Left {
                        origin: origin.clone(),
                        left: s.clone(),
                    });
                }
            }
        }
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
    pub fn origin(&self) -> String {
        match self {
            Out::Live { origin, .. }
            | Out::Status { origin, .. }
            | Out::Left { origin, .. }
            | Out::Listed { origin, .. }
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
            Out::Listed { origin } => F::Listed(b::OriginSessionsListedPayload {
                origin: crate::origin::Origin::named(origin.clone()),
            }),
            Out::Runs { runs, .. } => F::Runs(runs.clone()),
            Out::Branch { sid, off, runs, .. } => F::Branch(b::SessionBranchPayload {
                sid: sid.clone(),
                off: off.clone(),
                runs: runs.clone(),
            }),
        }
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
