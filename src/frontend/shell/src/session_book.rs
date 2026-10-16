//! **monitor 手里「这条连接上那台后端说过的会话成品」** —— 纯缓存，不裁决。
//!
//! 活 / 可重连 / 已结束由那台机器的后端裁（`src/backend/observe/session_ledger.rs`，帧 `session_added` · `session_state` ·
//! `sessions_replayed`），本机远端同一形。monitor 这一侧只做三件事：
//! ① 两条流（远端 `stream_source::consume_remote` · 本机 `stream_source::consume_local`）把成品交进来（[`feed`]）；
//! ② 按原样转给前端（出口由 `lib.rs` 装：[`Out`] ⇒ 事件 ＋ 拉前那几样副作用）；
//! ③ 留一份最新成品，给 F5 / 开窗的重放用（[`Book::replay`]，就绪点在会话流里原位交）。
//!
//! 唯一一件 monitor 自己知道、后端不知道的事是**到那台的连接断了**（[`In::LinkLost`]）：那台的成品随之作废（整份摘掉），
//! 当时还活的 / 可重连的交出去说「说不清」（不许显示成已结束）。重连之后那台的新连接自己重报一遍。
//!
//! 裁决一条都不在这一侧。

use parking_lot::RwLock;
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use cmd_wire::frame::{SessionAdded, SessionFate, SessionState, SessionStatus};

/// 去向那一种写好的字与语气（那台核心写的 `session_state.state_text` · `state_hint` · `state_tone` · `switch`，原样转交）。
/// 去向只由那台裁（手上留着、那台不在说的，那台也答一句已结束），每一份都带这几格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FateWords {
    pub text: cmd_wire::cells::Words,
    pub hint: cmd_wire::cells::Words,
    pub tone: cmd_wire::cells::Tone,
    /// 监控板组内的序（与活会话同一把尺）。
    pub order: u8,
    pub switch: cmd_wire::frame::Switch,
}

impl FateWords {
    /// `session_state` 带来的那几格。
    pub fn of(s: SessionState) -> Self {
        FateWords {
            text: s.state_text,
            hint: s.state_hint,
            tone: s.state_tone,
            order: s.state_order,
            switch: s.switch,
        }
    }
}

/// 一条会话此刻的最新成品。
#[derive(Debug, Clone, PartialEq)]
pub enum Product {
    Live(SessionAdded),
    Left(SessionFate, FateWords),
}

/// 两条流交进来的一件事。`origin` 是线上串（本机 `<local>`）。
#[derive(Debug, Clone, PartialEq)]
pub enum In {
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
        sid: String,
        fate: SessionFate,
        words: FateWords,
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
        sid: String,
        fate: SessionFate,
        words: FateWords,
    },
    /// `all` = 这一刻机器表里的每一台都报完了（「各台都报完」那一刻：前端据此收空组、说记住的那一格没出现）。
    Listed { origin: String, all: bool },
    /// 那台看不见了（**机器级**：线上只一格 `unseen {origin}`，前端按机器落说不清）。
    /// `sids` = 这一刻落说不清的那几条（排好序），只给 monitor 自己的旁路账用（本机那两份缓存），不上线。
    Unseen { origin: String, sids: Vec<String> },
    /// 一个会话的运行表（原样转）。
    Runs {
        origin: String,
        sid: String,
        runs: crate::ui_contract::RecordBody,
        ended: crate::ui_contract::RecordBody,
    },
    /// 一个会话的主线外清单（原样转）。
    Branch {
        origin: crate::origin::Origin,
        sid: String,
        off: crate::ui_contract::RecordBody,
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

/// 拼好的整份清单写成交给界面的那一格（JSON 数组原文）。
fn branch_body(off: &[String]) -> crate::ui_contract::RecordBody {
    crate::ui_contract::RecordBody::from_json(serde_json::to_string(off).unwrap_or_default())
        .unwrap_or_else(|| {
            crate::ui_contract::RecordBody::from_json("[]".to_string()).expect("[] is JSON")
        })
}

/// 全部机器的成品账（见模块头注）。
#[derive(Debug, Default)]
pub struct Book {
    origins: HashMap<String, OriginBook>,
    /// 每台每个会话手上那份主线外清单（F5 重放跟在宣告后面再说一次 · 报名单时交回手上有什么）。
    /// **断线不摘**（不跟 `origins` 那一台一起走）：重接之后交回它，那台只补缺的那一截。会话离开 · 关 tab · 那台报完清单时不在了 ⇒ 摘。
    branch: HashMap<String, BTreeMap<String, BranchCopy>>,
    /// 订了会话流的那几台（机器表：本机 ＋ 注册着的远端）；`None` = 还不知道 ⇒ 不立「各台都报完」。
    machines: Option<Vec<String>>,
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
            In::Status { origin, status } => {
                if let Some(Product::Live(m)) = self
                    .origins
                    .get_mut(&origin)
                    .and_then(|b| b.sessions.get_mut(&status.sid))
                {
                    m.face = status.face.clone();
                    m.name_face = status.name_face.clone();
                    m.waiting_for = status.waiting_for.clone();
                }
                vec![Out::Status { origin, status }]
            }
            In::Left {
                origin,
                sid,
                fate,
                words,
            } => {
                let b = self.origins.entry(origin.clone()).or_default();
                b.sessions
                    .insert(sid.clone(), Product::Left(fate, words.clone()));
                b.runs.remove(&sid);
                if let Some(m) = self.branch.get_mut(&origin) {
                    m.remove(&sid);
                }
                vec![Out::Left {
                    origin,
                    sid,
                    fate,
                    words,
                }]
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
                    .entry(origin.as_wire_str().to_string())
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
                    off: branch_body(&held.off),
                }]
            }
            In::Listed { origin } => {
                self.origins.entry(origin.clone()).or_default().listed = true;
                // 清单报完了：那台此刻不在的会话，手上那份清单没用了。
                let live = &self.origins[&origin].sessions;
                if let Some(m) = self.branch.get_mut(&origin) {
                    m.retain(|sid, _| matches!(live.get(sid), Some(Product::Live(_))));
                }
                let all = self.all_listed();
                vec![Out::Listed { origin, all }]
            }
            In::LinkLost { origin } => {
                let Some(b) = self.origins.remove(&origin) else {
                    return Vec::new();
                };
                let sids: Vec<String> = b
                    .sessions
                    .into_iter()
                    .filter(|(_, p)| !matches!(p, Product::Left(SessionFate::Ended, _)))
                    .map(|(s, _)| s)
                    .collect();
                if sids.is_empty() {
                    return Vec::new();
                }
                vec![Out::Unseen { origin, sids }]
            }
        }
    }

    /// 报名单时交回的那一格（`stream-watch.branch`）：那台这几个会话的主线外清单手上各有什么（没有 / 说不准 ⇒ `at: 0`）。
    pub fn branch_held(
        &self,
        origin: &crate::origin::Origin,
        sids: &std::collections::BTreeSet<String>,
    ) -> Vec<cmd_wire::watch::BranchHeld> {
        let mine = self.branch.get(origin.as_wire_str());
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
        matches!(
            self.origins
                .get(origin.as_wire_str())
                .and_then(|b| b.sessions.get(sid)),
            Some(Product::Live(_))
        )
    }

    /// 那台那个会话此刻活着时，宣告它的那一帧带来的 agent 进程号（↗ 点那一刻从它往上走进程链；没带 / 不活 ⇒ `None`）。
    pub fn live_pid(&self, origin: &crate::origin::Origin, sid: &str) -> Option<u32> {
        match self
            .origins
            .get(origin.as_wire_str())
            .and_then(|b| b.sessions.get(sid))
        {
            Some(Product::Live(meta)) => Some(meta.pid),
            _ => None,
        }
    }

    /// 用户关掉一个已结束的 tab（`EventReplay::forget` 同一刻）⇒ 它的成品也忘掉（不再重放）。
    pub fn forget(&mut self, sid: &str) {
        for b in self.origins.values_mut() {
            b.sessions.remove(sid);
            b.runs.remove(sid);
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
                        if let Some(c) = self.branch.get(*o).and_then(|m| m.get(sid)) {
                            r.before.push(Out::Branch {
                                origin: crate::origin::Origin((*o).clone()),
                                sid: sid.clone(),
                                off: branch_body(&c.off),
                            });
                        }
                    }
                    Product::Left(SessionFate::Reconnectable, words) => r.after.push(Out::Left {
                        origin: (*o).clone(),
                        sid: sid.clone(),
                        fate: SessionFate::Reconnectable,
                        words: words.clone(),
                    }),
                    _ => {}
                }
            }
        }
        let mut unseen: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut settled = Vec::new();
        for (sid, origin) in kept {
            let b = self.origins.get(origin);
            match b.and_then(|b| b.sessions.get(sid)) {
                Some(Product::Live(_)) | Some(Product::Left(SessionFate::Reconnectable, _)) => {}
                Some(Product::Left(SessionFate::Ended, words)) => settled.push(Out::Left {
                    origin: origin.clone(),
                    sid: sid.clone(),
                    fate: SessionFate::Ended,
                    words: words.clone(),
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
    pub fn origin(&self) -> &str {
        match self {
            Out::Live { origin, .. }
            | Out::Status { origin, .. }
            | Out::Left { origin, .. }
            | Out::Listed { origin, .. }
            | Out::Unseen { origin, .. }
            | Out::Runs { origin, .. } => origin,
            Out::Branch { origin, .. } => origin.as_wire_str(),
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
                        background: meta.background,
                        attachable: meta.attachable,
                        cwd: meta.cwd.clone(),
                        project_dir: meta.project_dir.clone(),
                        name: meta.name.clone(),
                        hidden: meta.hidden.clone(),
                    }),
                    F::Activity(b::SessionActivityPayload {
                        session_id: sid.clone(),
                        activity: b::ActivityCells::of(&meta.face),
                        naming: b::NameCells::of(&meta.name_face),
                        waiting_for: meta.waiting_for.clone(),
                    }),
                ];
                if let Some(c) = &meta.container {
                    v.push(F::Container(b::SessionContainerPayload {
                        session_id: sid.clone(),
                        container: c.clone(),
                    }));
                }
                v
            }
            Out::Status { status: s, .. } => vec![F::Activity(b::SessionActivityPayload {
                session_id: s.sid.clone(),
                activity: b::ActivityCells::of(&s.face),
                naming: b::NameCells::of(&s.name_face),
                waiting_for: s.waiting_for.clone(),
            })],
            Out::Left {
                sid, fate, words, ..
            } => {
                let (text, hint, tone, order, switch) = (
                    words.text.0.clone(),
                    words.hint.0.clone(),
                    b::tone_word(words.tone),
                    words.order,
                    words.switch.clone(),
                );
                vec![match fate {
                    SessionFate::Reconnectable => F::Idle(b::SessionIdlePayload {
                        session_id: sid.clone(),
                        text,
                        hint,
                        tone,
                        order,
                        switch,
                    }),
                    SessionFate::Ended => F::Ended(b::SessionEndedPayload {
                        session_id: sid.clone(),
                        text,
                        hint,
                        tone,
                        order,
                        switch,
                    }),
                }]
            }
            Out::Listed { origin, all } => vec![F::Listed(b::OriginSessionsListedPayload {
                origin: crate::origin::Origin(origin.clone()),
                all: *all,
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
            Out::Branch { sid, off, .. } => vec![F::Branch(b::SessionBranchPayload {
                session_id: sid.clone(),
                off: off.clone(),
            })],
        }
    }
}

impl Book {
    /// 旁路快照被取消（会话离开了 / 连接断了）时那条会话**此刻**的终局 —— 快照行可能已经把刚落定的 tab 翻活，
    /// 原样再说一次它现在是什么（原先这里恒补「已结束」，那是 monitor 自己在裁）。活着 ⇒ 不说；这条连接上没说过 ⇒ 说不清。
    /// 没说过它：那台还没报完清单 ⇒ 机器级说不清（见 [`Self::unseen_block`]）；报完了 ⇒ 不说（去向由那台答）。
    pub fn settle_again(&self, origin: &str, sid: &str) -> Vec<Out> {
        let b = self.origins.get(origin);
        match b.and_then(|b| b.sessions.get(sid)) {
            Some(Product::Live(_)) => Vec::new(),
            Some(Product::Left(fate, words)) => vec![Out::Left {
                origin: origin.to_string(),
                sid: sid.to_string(),
                fate: *fate,
                words: words.clone(),
            }],
            None if b.is_some_and(|b| b.listed) => Vec::new(),
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
                    Product::Left(SessionFate::Reconnectable, words) => v.push(Out::Left {
                        origin: origin.to_string(),
                        sid: sid.clone(),
                        fate: SessionFate::Reconnectable,
                        words: words.clone(),
                    }),
                    Product::Left(SessionFate::Ended, _) => {}
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

/// 机器表换了（`backend_control::reconcile_remotes` 那一处调）：记账，有话就交出口。
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
