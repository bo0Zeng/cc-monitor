//! **流归位**：中转抄出来的每一段流（一次上游应答）折成归一事件、定它归哪个运行，然后才变成 `tap` 帧。
//!
//! 只用适配层给的那几格（协议面 `StreamFace` · 请求自报的运行 · 运行簿里学到的对账键），不认任何一家的形状：
//! 1. 用哪个协议面：头一件事折得出「开始」的那一个（各家上游的协议面挨个问）；都折不出 ⇒ 这段不收。
//! 2. 归哪个运行：请求自报了（中转从登记的头里取到值）⇒ 就是它；没自报 ⇒ 这个会话此刻**没有在跑的子运行** ⇒ 归主运行；
//!    **有** ⇒ 先挂起，等哪条记录（主或子）的对账键对上，按它的归属放出来。挂起的那段在对上之前不上任何活卡。
//! 3. 号：放出去的帧按段重新从 0 连续编号（界面靠它看缺口）；上游那一侧缺了号（tap 通道满）⇒ 这一段收尾成 `broken`、不再收。

use crate::agents::{StreamEv, StreamFace};
use crate::observe::runs::RunBook;
use crate::relay::{TapBody, TapEvent};
use crate::stream::wire::{Frame, TapEnd};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

/// 同时在攒的段最多几个（超了挤掉最老的）。
pub(crate) const ROUTE_RESPS_KEEP: usize = 32;
/// 一段挂起时最多攒几件（超了这一段作废：它的记录到了自然定稿）。
pub(crate) const ROUTE_PENDING_KEEP: usize = 512;
/// 记住几个「不再收」的段（先进先出）。
const DEAD_KEEP: usize = 256;

type Item = (Option<StreamEv>, Option<TapEnd>);

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
    faces: Vec<StreamFace>,
    resps: HashMap<u64, Resp>,
    order: VecDeque<u64>,
    dead: HashSet<u64>,
    dead_order: VecDeque<u64>,
}

impl RunRouter {
    pub(crate) fn new(book: Arc<RunBook>, faces: Vec<StreamFace>) -> Self {
        Self {
            book,
            faces,
            resps: HashMap::new(),
            order: VecDeque::new(),
            dead: HashSet::new(),
            dead_order: VecDeque::new(),
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
            let TapBody::Data(d) = &ev.body else {
                self.bury(resp);
                return Vec::new();
            };
            if ev.n != 0 || ev.stream.is_empty() {
                self.bury(resp);
                return Vec::new();
            }
            let Some(face) = self.faces.iter().copied().find(|f| {
                (f.fold)(d)
                    .iter()
                    .any(|e| matches!(e, StreamEv::Start { .. }))
            }) else {
                self.bury(resp);
                return Vec::new();
            };
            let route = if !ev.owner.is_empty() {
                Route::Run(Some(ev.owner.clone()))
            } else if self.book.running(&ev.stream) == 0 {
                Route::Run(None)
            } else {
                Route::Pending(Vec::new())
            };
            if self.resps.len() >= ROUTE_RESPS_KEEP {
                if let Some(old) = self.order.front().copied() {
                    self.bury(old);
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
            self.bury(resp);
            return out;
        }
        r.raw_next += 1;
        let items: Vec<Item> = match ev.body {
            TapBody::Data(d) => (r.face.fold)(&d)
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
                    self.bury(resp);
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
