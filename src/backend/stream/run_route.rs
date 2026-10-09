//! **流归位**：中转抄出来的每一段流（一次上游应答）折成归一事件、定它归哪个运行，然后才变成 `tap` 帧。
//!
//! 只用适配层给的那几格（协议面 `StreamFace` · 请求自报的运行 · 运行簿里学到的对账键），不认任何一家的形状：
//! 1. 用哪个协议面：头一件事折得出「开始」的那一个（各家上游的协议面挨个问）；都折不出 ⇒ 这段不收。
//! 2. 归哪个运行：请求自报了（中转从登记的头里取到值）且不等于会话标签 ⇒ 就是它（等于 ⇒ 主运行）；没自报而这一家声明了自报的头 ⇒ 就是主运行（当场定）；
//!    这一家没声明那个头 ⇒ 这个会话此刻**没有在跑的子运行**就归主运行，**有**就先挂起，等哪条记录（主或子）的对账键对上，
//!    按它的归属放出来。挂起的那段在对上之前不上任何活卡。
//! 3. 号：放出去的帧按段重新从 0 连续编号（界面靠它看缺口）；上游那一侧缺了号（tap 通道满）⇒ 这一段收尾成 `broken`、不再收。
//! 4. 没放出去 / 半路收了的段按原因数（[`Lost`]），每种第 1、2、4、8… 次说一行；正常说完不算。

use crate::agents::{StreamEv, StreamFace, StreamFamily};
use crate::observe::runs::RunBook;
use crate::relay::{TapBody, TapEvent};
use crate::stream::wire::{Frame, TapEnd};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Arc;

/// 同时在攒的段最多几个（超了挤掉最老的）。
pub(crate) const ROUTE_RESPS_KEEP: usize = 32;
/// 一段挂起时最多攒几件（超了这一段作废：它的记录到了自然定稿）。
pub(crate) const ROUTE_PENDING_KEEP: usize = 512;
/// 记住几个「不再收」的段（先进先出）。
const DEAD_KEEP: usize = 256;

type Item = (Option<StreamEv>, Option<TapEnd>);

/// 一段流为什么没放出去 / 半路收了（这条连接上数）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Lost {
    /// 头一件不是 0 号（开头在 tap 通道那一跳丢了，对账键在里面）。
    Head,
    /// 没有会话标签（请求没带会话头）⇒ 对不上任何 tab。
    NoStream,
    /// 哪家的协议面都折不出「开始」。
    NoFace,
    /// 同时在攒的段满了，最老的被挤掉。
    Evicted,
    /// 半路缺了号（tap 通道满）⇒ 收尾成断。
    Gap,
    /// 挂起时攒的件超了上界。
    PendingOverflow,
}

enum Route {
    Run(Option<String>),
    Pending(Vec<Item>),
}

struct Resp {
    sid: String,
    face: StreamFace,
    raw_next: u64,
    out_n: u64,
    rid: Option<String>,
    route: Route,
    ended: bool,
}

/// 一条流连接的流归位（`tap::TapRx` 持有）。
pub(crate) struct RunRouter {
    book: Arc<RunBook>,
    families: Vec<StreamFamily>,
    resps: HashMap<u64, Resp>,
    order: VecDeque<u64>,
    dead: HashSet<u64>,
    dead_order: VecDeque<u64>,
    /// 这条连接上至今每种丢了几段。
    lost: BTreeMap<Lost, u64>,
}

impl RunRouter {
    pub(crate) fn new(book: Arc<RunBook>, families: Vec<StreamFamily>) -> Self {
        Self {
            book,
            families,
            resps: HashMap::new(),
            order: VecDeque::new(),
            dead: HashSet::new(),
            dead_order: VecDeque::new(),
            lost: BTreeMap::new(),
        }
    }

    /// 这一段不收了，因为 `why`：埋掉、数一次、该说就说。
    fn lose(&mut self, resp: u64, why: Lost) {
        self.bury(resp);
        let c = self.lost.entry(why).or_default();
        *c += 1;
        let n = *c;
        if n.is_power_of_two() {
            tracing::warn!(
                "[tap] 一段流没放出去 / 半路收了：{why:?}（这条连接上这一种第 {n} 次；累计 {:?}）",
                self.lost
            );
        }
    }

    fn bury(&mut self, resp: u64) {
        self.resps.remove(&resp);
        self.order.retain(|r| *r != resp);
        if self.dead.insert(resp) {
            self.dead_order.push_back(resp);
            while self.dead_order.len() > DEAD_KEEP {
                if let Some(old) = self.dead_order.pop_front() {
                    self.dead.remove(&old);
                }
            }
        }
    }

    /// 一件中转抄出来的事 ⇒ 此刻能放出去的帧（挂起的不出）。
    pub(crate) fn on_tap(&mut self, ev: TapEvent) -> Vec<Frame> {
        let resp = ev.resp;
        if self.dead.contains(&resp) {
            return Vec::new();
        }
        if !self.resps.contains_key(&resp) {
            // 头一件不是 0 号 ⇒ 开头丢了（对账键在里面）；没有会话标签 ⇒ 对不上任何 tab。都不收。
            let d = match &ev.body {
                TapBody::Data(_) | TapBody::Clipped { .. } => &ev.body,
                TapBody::End { .. } => {
                    self.lose(resp, Lost::Head);
                    return Vec::new();
                }
            };
            if ev.n != 0 {
                self.lose(resp, Lost::Head);
                return Vec::new();
            }
            if ev.stream.is_empty() {
                self.lose(resp, Lost::NoStream);
                return Vec::new();
            }
            let Some(family) = self.families.iter().copied().find(|f| {
                fold_body(f.face, d)
                    .iter()
                    .any(|e| matches!(e, StreamEv::Start { .. }))
            }) else {
                self.lose(resp, Lost::NoFace);
                return Vec::new();
            };
            let face = family.face;
            // 自报的运行就是会话本身（有的家主运行也带那个头、值等于会话标识）⇒ 主运行。
            let route = if !ev.owner.is_empty() && ev.owner != ev.stream {
                Route::Run(Some(ev.owner.clone()))
            } else if family.owns || self.book.running(&ev.stream) == 0 {
                Route::Run(None)
            } else {
                Route::Pending(Vec::new())
            };
            if self.resps.len() >= ROUTE_RESPS_KEEP {
                if let Some(old) = self.order.front().copied() {
                    self.lose(old, Lost::Evicted);
                }
            }
            self.resps.insert(
                resp,
                Resp {
                    sid: ev.stream.clone(),
                    face,
                    raw_next: 0,
                    out_n: 0,
                    rid: None,
                    route,
                    ended: false,
                },
            );
            self.order.push_back(resp);
        }
        let Some(r) = self.resps.get_mut(&resp) else {
            return Vec::new();
        };
        if ev.n != r.raw_next {
            // 上游那一侧缺了号：这一段认不全了 ⇒ 收尾成断、不再收（定稿归记录）。
            let items = vec![(None, Some(TapEnd::Broken))];
            let out = emit(r, resp, items);
            self.lose(resp, Lost::Gap);
            return out;
        }
        r.raw_next += 1;
        let items: Vec<Item> = match ev.body {
            body @ (TapBody::Data(_) | TapBody::Clipped { .. }) => fold_body(r.face, &body)
                .into_iter()
                .map(|e| {
                    if let StreamEv::Start { rid } = &e {
                        r.rid = Some(rid.clone());
                    }
                    (Some(e), None)
                })
                .collect(),
            TapBody::End { broken } => {
                r.ended = true;
                vec![(
                    None,
                    Some(if broken { TapEnd::Broken } else { TapEnd::Done }),
                )]
            }
        };
        // 挂起的这一段：此刻的对账键也许已经学到了（记录比流先到）。
        if let (Route::Pending(_), Some(rid)) = (&r.route, &r.rid) {
            if let Some(owner) = self.book.owner_of(&r.sid, rid) {
                let held = match std::mem::replace(&mut r.route, Route::Run(owner)) {
                    Route::Pending(h) => h,
                    Route::Run(_) => Vec::new(),
                };
                let mut all = held;
                all.extend(items);
                let out = emit(r, resp, all);
                if r.ended {
                    self.bury(resp);
                }
                return out;
            }
        }
        match &mut r.route {
            Route::Pending(held) => {
                held.extend(items);
                if held.len() > ROUTE_PENDING_KEEP {
                    self.lose(resp, Lost::PendingOverflow);
                }
                Vec::new()
            }
            Route::Run(_) => {
                let out = emit(r, resp, items);
                if r.ended {
                    self.bury(resp);
                }
                out
            }
        }
    }

    /// 运行簿学到了新的对账键归属 ⇒ 挂起的那几段里对得上的放出来。
    pub(crate) fn on_learned(&mut self) -> Vec<Frame> {
        let mut out = Vec::new();
        let mut done = Vec::new();
        for resp in self.order.clone() {
            let Some(r) = self.resps.get_mut(&resp) else {
                continue;
            };
            let (Route::Pending(_), Some(rid)) = (&r.route, &r.rid) else {
                continue;
            };
            let Some(owner) = self.book.owner_of(&r.sid, rid) else {
                continue;
            };
            let held = match std::mem::replace(&mut r.route, Route::Run(owner)) {
                Route::Pending(h) => h,
                Route::Run(_) => Vec::new(),
            };
            out.extend(emit(r, resp, held));
            if r.ended {
                done.push(resp);
            }
        }
        for resp in done {
            self.bury(resp);
        }
        out
    }
}

/// 一件事 ⇒ 那一家协议面折出的归一事件（截断的那一件只走它的截断折法）。
fn fold_body(face: StreamFace, body: &TapBody) -> Vec<StreamEv> {
    match body {
        TapBody::Data(d) => (face.fold)(d),
        TapBody::Clipped { head, .. } => (face.fold_clipped)(head),
        TapBody::End { .. } => Vec::new(),
    }
}

fn emit(r: &mut Resp, resp: u64, items: Vec<Item>) -> Vec<Frame> {
    let Route::Run(run) = &r.route else {
        return Vec::new();
    };
    items
        .into_iter()
        .map(|(ev, end)| {
            let n = r.out_n;
            r.out_n += 1;
            Frame::Tap {
                stream: r.sid.clone(),
                run: run.clone(),
                resp,
                n,
                ev,
                end,
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "../../../tests/backend/stream/run_route_tests.rs"]
pub(crate) mod tests; // `pub(crate)`：听 `tracing` 说了什么的 `tests::heard` 给 hub 的单测共用
