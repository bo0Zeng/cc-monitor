//! **活卡成品**（整台一份）：中转抄出来的流先上屏、记录那一轮到了整轮覆盖 —— 这件事只在这里做，桌面与手机照 `live` 帧画。
//!
//! ```text
//! 中转转发线程 ── TapPort::offer ──▶ Live（锁里：流归位 → 卡表 → 每条连接的发件箱）──Notify──▶ 写者最低优先取 ──▶ `live` 帧
//! RunBook::record（记录到了）· retire / forget（会话走了）──▶ Live（观察口 [`crate::observe::runs::BookWatch`]）
//! `stream-watch`（这条连接在看哪几个）──▶ Live（与 watcher 那份名单同拍交）
//! ```
//!
//! - **一张卡** ＝ 一个运行里的一次上游应答，键 `card`（本进程的段号）。卡有了对账键（`start.rid`）才对外可见。
//! - **对账**：记录过了运行簿 ⇒ 同 `rid` 的卡撤（`close {record}`）；同一运行里已说完、`rid` 不同的卡也撤（它的记录不会再来）。
//!   记录先到 ⇒ `rid` 进墓碑，那张卡出不来。
//! - **子运行**：卡带 `run`，主 tab 不画；说完了照样等它自己的记录（或那个运行收场 / 被挤）。
//! - **上界**：每运行 ≤ [`LIVE_PER_RUN`] 张、整台 ≤ [`LIVE_CARDS_KEEP`] 张（满了先挤说完的、再挤子运行的；只剩正在说的主运行 ⇒
//!   新来的主运行照收、子运行不收）；正文留尾 [`LIVE_TEXT_KEEP`] 字（超出 [`LIVE_TEXT_SLACK`] 才截一次、整张重发）。
//! - **每条连接一个发件箱**（≤ [`LIVE_OUTBOX_CAP`] 件）：满了不丢「某几帧」，那张卡排着的件全撤、改记「欠一张整卡」，写者取到时按卡此刻的样子现写
//!   `open` ⇒ 客户端从不需要自己补缺口；它只认一条：`n` 不接上一帧就先不画那张卡，等下一张 `open` / `close`。
//! - **名单**：只发名单里的会话（同 `line`，`stream-watch` 那一份；没报过 ＝ 全看）。刚进名单 ⇒ 它此刻每张卡补一张 `open`；出名单 ⇒ `close {unwatched}`。
//! - 不阻塞转发：锁里只做内存里的折与记账，不等 IO；不加定时器。

use crate::agents::{BlockKind, StreamEv};
use crate::observe::runs::RunBook;
use crate::relay::TapEvent;
use crate::stream::run_route::{Routed, RunRouter};
use crate::wire::{Frame, LiveBlock, LiveCard, LiveOp, LivePhase, LiveWhy, RunState, TapEnd};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Arc;

/// 每个运行（主运行 ＋ 每个子运行各算一个）最多几张卡（主线程那一张 ＋ 并发的一张）。
pub(crate) const LIVE_PER_RUN: usize = 2;
/// 整台同时最多几张卡。
pub(crate) const LIVE_CARDS_KEEP: usize = 16;
/// 一张卡正文最多留几个字（只留尾巴）。
pub(crate) const LIVE_TEXT_KEEP: usize = 32_768;
/// 正文超过留尾多少字才截一次（截一次就整张重发：摊下来每这么多新字多发一张整卡）。
pub(crate) const LIVE_TEXT_SLACK: usize = 8_192;
/// 每个会话记住几个已定稿的对账键（先进先出）。
pub(crate) const LIVE_TOMB_KEEP: usize = 64;
/// 记住几个「不再收」的段（先进先出）。
const DEAD_KEEP: usize = 256;
/// 每条连接的发件箱能排多少**件**（条数不是体量：一件 ≤ 一段上游文字或一张整卡）。
/// 登记住址 `tests/frontend/shell/byte_cap_registry.rs` 的 `NOT_A_SIZE_CAP`。
pub(crate) const LIVE_OUTBOX_CAP: usize = 256;

/// 先进先出、有上界的集合。
struct Bounded<T: std::hash::Hash + Eq + Clone> {
    order: VecDeque<T>,
    set: HashSet<T>,
    keep: usize,
}

impl<T: std::hash::Hash + Eq + Clone> Bounded<T> {
    fn new(keep: usize) -> Self {
        Self {
            order: VecDeque::new(),
            set: HashSet::new(),
            keep,
        }
    }
    fn has(&self, k: &T) -> bool {
        self.set.contains(k)
    }
    fn add(&mut self, k: T) {
        if self.set.insert(k.clone()) {
            self.order.push_back(k);
            while self.order.len() > self.keep {
                if let Some(old) = self.order.pop_front() {
                    self.set.remove(&old);
                }
            }
        }
    }
}

struct Card {
    sid: String,
    run: Option<String>,
    rid: Option<String>,
    stopped: bool,
    clipped: bool,
    blocks: Vec<LiveBlock>,
    born: u64,
}

impl Card {
    fn open(&self) -> Option<LiveOp> {
        Some(LiveOp::Open {
            rid: self.rid.clone()?,
            phase: if self.stopped {
                LivePhase::Awaiting
            } else {
                LivePhase::Streaming
            },
            clipped: self.clipped,
            blocks: self.blocks.clone(),
        })
    }

    fn text_len(&self) -> usize {
        self.blocks
            .iter()
            .map(|b| b.text.as_ref().map_or(0, |t| t.chars().count()))
            .sum()
    }

    /// 正文总长超留尾 ⇒ 从最前面的块开始截头，截到正好留尾。
    fn clip(&mut self) {
        let mut over = self.text_len().saturating_sub(LIVE_TEXT_KEEP);
        for b in &mut self.blocks {
            if over == 0 {
                break;
            }
            let Some(t) = b.text.as_mut() else { continue };
            let n = t.chars().count();
            let cut = n.min(over);
            if cut == 0 {
                continue;
            }
            let at = t.char_indices().nth(cut).map_or(t.len(), |(i, _)| i);
            t.drain(..at);
            over -= cut;
            self.clipped = true;
        }
    }
}

enum Entry {
    /// 欠这张卡一张整卡（取的时候按卡此刻的样子现写）。
    Snap(u64),
    Op(u64, LiveOp),
    Close(u64, LiveWhy),
}

impl Entry {
    fn card(&self) -> u64 {
        match self {
            Entry::Snap(c) | Entry::Op(c, _) | Entry::Close(c, _) => *c,
        }
    }
}

struct Conn {
    id: u64,
    /// `None` ＝ 没报过 ＝ 全看。
    watching: Option<HashSet<String>>,
    q: VecDeque<Entry>,
    /// 这条连接已经拿到 `open` 的卡 → 下一帧的 `n`。
    next_n: HashMap<u64, u64>,
    /// 已撤、撤卡那一件还排着的卡的身份（会话 · 运行）。
    closed: HashMap<u64, (String, Option<String>)>,
    wake: Arc<tokio::sync::Notify>,
}

impl Conn {
    fn wants(&self, sid: &str) -> bool {
        self.watching.as_ref().is_none_or(|w| w.contains(sid))
    }

    fn has_snap(&self, c: u64) -> bool {
        self.q
            .iter()
            .any(|e| matches!(e, Entry::Snap(x) if *x == c))
    }

    /// 欠一张整卡：排着的这张卡的件全撤（整卡里都有）。
    fn owe(&mut self, c: u64) {
        if self.has_snap(c) {
            return;
        }
        self.q.retain(|e| !matches!(e, Entry::Op(x, _) if *x == c));
        self.room(c);
        self.q.push_back(Entry::Snap(c));
        self.wake.notify_one();
    }

    fn op(&mut self, c: u64, op: LiveOp) {
        if self.has_snap(c) || !self.next_n.contains_key(&c) {
            return;
        }
        if self.q.len() >= LIVE_OUTBOX_CAP {
            // 满了：这张卡改成欠一张整卡（不丢件，合成一张）。
            self.owe(c);
            return;
        }
        self.q.push_back(Entry::Op(c, op));
        self.wake.notify_one();
    }

    fn close(&mut self, c: u64, why: LiveWhy) {
        self.q.retain(|e| e.card() != c);
        if self.next_n.contains_key(&c) {
            self.room(c);
            self.q.push_back(Entry::Close(c, why));
            self.wake.notify_one();
        }
    }

    /// 要往里放一件而满了 ⇒ 排得最多的那张卡（不是 `c`）合成一张整卡，腾出位置。
    fn room(&mut self, c: u64) {
        while self.q.len() >= LIVE_OUTBOX_CAP {
            let mut count: BTreeMap<u64, usize> = BTreeMap::new();
            for e in &self.q {
                if let Entry::Op(x, _) = e {
                    if *x != c {
                        *count.entry(*x).or_default() += 1;
                    }
                }
            }
            let Some((&victim, _)) = count.iter().max_by_key(|(_, n)| **n) else {
                return; // 全是整卡与撤卡（卡数有上界，到不了这里）
            };
            self.q
                .retain(|e| !matches!(e, Entry::Op(x, _) if *x == victim));
            if !self.has_snap(victim) {
                self.q.push_back(Entry::Snap(victim));
            }
        }
    }
}

/// 活卡的整台状态（[`hub`] 那一份；判据各造各的）。
pub(crate) struct Live {
    router: RunRouter,
    book: Arc<RunBook>,
    cards: HashMap<u64, Card>,
    dead: Bounded<u64>,
    tombs: HashMap<String, Bounded<String>>,
    born: u64,
    conns: Vec<Conn>,
    next_conn: u64,
    /// 每种撤卡至今几次（不算记录到了 · 会话走了 · 出名单这几种正常的）。
    lost: BTreeMap<LiveWhy, u64>,
}

impl Live {
    pub(crate) fn new(book: Arc<RunBook>, families: Vec<crate::agents::StreamFamily>) -> Self {
        Self {
            router: RunRouter::new(book.clone(), families),
            book,
            cards: HashMap::new(),
            dead: Bounded::new(DEAD_KEEP),
            tombs: HashMap::new(),
            born: 0,
            conns: Vec::new(),
            next_conn: 1,
            lost: BTreeMap::new(),
        }
    }

    /// 换成这台 watcher 写的那一本运行簿（起 watcher 时一次；之前抄出来的段照旧算数）。
    pub(crate) fn adopt(&mut self, book: Arc<RunBook>, families: Vec<crate::agents::StreamFamily>) {
        self.router = RunRouter::new(book.clone(), families);
        self.book = book;
    }

    /// 接上一条连接（没报过名单 ＝ 全看）：此刻每张可见的卡先欠它一张整卡。回它的号与叫醒口。
    pub(crate) fn join(&mut self) -> (u64, Arc<tokio::sync::Notify>) {
        let id = self.next_conn;
        self.next_conn += 1;
        let wake = Arc::new(tokio::sync::Notify::new());
        let mut c = Conn {
            id,
            watching: None,
            q: VecDeque::new(),
            next_n: HashMap::new(),
            closed: HashMap::new(),
            wake: wake.clone(),
        };
        for (k, _) in self.visible() {
            c.owe(k);
        }
        self.conns.push(c);
        (id, wake)
    }

    pub(crate) fn leave(&mut self, id: u64) {
        self.conns.retain(|c| c.id != id);
    }

    /// 这条连接此刻在看哪几个会话（整份换，`stream-watch` 那一份）。刚进的补整卡，出去的撤卡。
    pub(crate) fn watch(&mut self, id: u64, sids: &[String]) {
        let next: HashSet<String> = sids.iter().cloned().collect();
        let vis: Vec<(u64, String)> = self.visible().map(|(k, c)| (k, c.sid.clone())).collect();
        let Some(c) = self.conns.iter_mut().find(|c| c.id == id) else {
            return;
        };
        for (k, sid) in vis {
            let was = c.wants(&sid);
            let now = next.contains(&sid);
            if was && !now {
                c.close(k, LiveWhy::Unwatched);
            } else if !was && now {
                c.owe(k);
            }
        }
        c.watching = Some(next);
    }

    /// 写者取下一帧（没有 ⇒ `None`，等叫醒）。
    pub(crate) fn pull(&mut self, id: u64) -> Option<Frame> {
        let cards = &self.cards;
        let c = self.conns.iter_mut().find(|c| c.id == id)?;
        while let Some(e) = c.q.pop_front() {
            let (card, n, op) = match e {
                Entry::Snap(k) => {
                    let Some(op) = cards.get(&k).and_then(Card::open) else {
                        continue;
                    };
                    c.next_n.insert(k, 1);
                    (k, 0, op)
                }
                Entry::Op(k, op) => {
                    let Some(n) = c.next_n.get_mut(&k) else {
                        continue;
                    };
                    *n += 1;
                    (k, *n - 1, op)
                }
                Entry::Close(k, why) => {
                    let Some(n) = c.next_n.remove(&k) else {
                        continue;
                    };
                    (k, n, LiveOp::Close { why })
                }
            };
            let gone = match op {
                LiveOp::Close { .. } => c.closed.remove(&card),
                _ => None,
            };
            let Some((sid, run)) =
                gone.or_else(|| cards.get(&card).map(|x| (x.sid.clone(), x.run.clone())))
            else {
                continue;
            };
            return Some(Frame::LiveCard(LiveCard {
                sid,
                run,
                card,
                n,
                op,
            }));
        }
        None
    }

    fn visible(&self) -> impl Iterator<Item = (u64, &Card)> {
        self.cards
            .iter()
            .filter(|(_, c)| c.rid.is_some())
            .map(|(k, c)| (*k, c))
    }

    /// 中转抄出来的一件事：归位、折成卡上的变化、分给各条连接。
    pub(crate) fn tap(&mut self, ev: TapEvent) {
        let routed = self.router.on_tap(ev);
        for r in routed {
            self.apply(r);
        }
    }

    /// 一条记录过了运行簿（主或子运行；`owner`：学到的归属，`None` ＝ 不知道）。
    pub(crate) fn record(&mut self, sid: &str, rid: &str, owner: Option<Option<String>>) {
        self.tombs
            .entry(sid.to_string())
            .or_insert_with(|| Bounded::new(LIVE_TOMB_KEEP))
            .add(rid.to_string());
        let hit: Vec<u64> = self
            .cards
            .iter()
            .filter(|(_, c)| {
                c.sid == sid
                    && (c.rid.as_deref() == Some(rid)
                        || c.stopped && owner.as_ref().is_some_and(|o| *o == c.run))
            })
            .map(|(k, _)| *k)
            .collect();
        for k in hit {
            self.kill(k, LiveWhy::Record);
        }
        // 挂起的段可能对上了。
        for r in self.router.on_learned() {
            self.apply(r);
        }
        self.reap_runs(sid);
    }

    /// 会话走了：它的卡全撤。
    pub(crate) fn gone(&mut self, sid: &str) {
        let hit: Vec<u64> = self
            .cards
            .iter()
            .filter(|(_, c)| c.sid == sid)
            .map(|(k, _)| *k)
            .collect();
        for k in hit {
            self.kill(k, LiveWhy::Gone);
        }
        self.tombs.remove(sid);
    }

    /// 运行簿说已收场的子运行：它的卡撤。
    fn reap_runs(&mut self, sid: &str) {
        // 每条记录都会走到这里（预读大记录时一秒上万条）：这个会话没有子运行的卡就不去问运行簿。
        if !self.cards.values().any(|c| c.sid == sid && c.run.is_some()) {
            return;
        }
        let runs = self.book.runs(sid);
        let hit: Vec<u64> = self
            .cards
            .iter()
            .filter(|(_, c)| {
                c.sid == sid
                    && c.run.as_ref().is_some_and(|r| {
                        runs.iter()
                            .any(|x| &x.run == r && x.state != RunState::Running)
                    })
            })
            .map(|(k, _)| *k)
            .collect();
        for k in hit {
            self.kill(k, LiveWhy::Gone);
        }
    }

    fn apply(&mut self, r: Routed) {
        let k = r.resp;
        if self.dead.has(&k) {
            return;
        }
        if !self.cards.contains_key(&k) {
            let card = Card {
                sid: r.sid.clone(),
                run: r.run.clone(),
                rid: None,
                stopped: false,
                clipped: false,
                blocks: Vec::new(),
                born: self.born,
            };
            self.born += 1;
            if !self.admit(k, card) {
                return;
            }
        }
        if let Some(end) = r.end {
            let stopped = self.cards.get(&k).is_some_and(|c| c.stopped);
            if end == TapEnd::Done || stopped {
                self.settle(k);
            } else {
                self.kill(k, LiveWhy::Broken);
            }
            return;
        }
        let Some(ev) = r.ev else { return };
        match ev {
            StreamEv::Start { rid } => {
                let tomb = self.tombs.get(&r.sid).is_some_and(|t| t.has(&rid));
                if tomb {
                    self.kill(k, LiveWhy::Record);
                    return;
                }
                if let Some(c) = self.cards.get_mut(&k) {
                    c.rid = Some(rid);
                }
                self.fan_owe(k);
            }
            StreamEv::Block { i, kind, tool } => {
                let Some(c) = self.cards.get_mut(&k) else {
                    return;
                };
                let i = i as usize;
                while c.blocks.len() <= i {
                    c.blocks.push(LiveBlock {
                        kind: BlockKind::Other,
                        text: None,
                        tool: None,
                    });
                }
                c.blocks[i] = LiveBlock {
                    kind,
                    text: (kind == BlockKind::Text).then(String::new),
                    tool: tool.clone(),
                };
                self.fan(
                    k,
                    LiveOp::Block {
                        i: i as u64,
                        block: kind,
                        tool,
                    },
                );
            }
            StreamEv::Text { i, s } => {
                let Some(c) = self.cards.get_mut(&k) else {
                    return;
                };
                let Some(t) = c.blocks.get_mut(i as usize).and_then(|b| b.text.as_mut()) else {
                    return;
                };
                t.push_str(&s);
                if c.text_len() > LIVE_TEXT_KEEP + LIVE_TEXT_SLACK {
                    c.clip();
                    self.fan_owe(k);
                } else {
                    self.fan(k, LiveOp::Text { i, s });
                }
            }
            StreamEv::Stop { ok } => {
                if ok {
                    self.settle(k);
                } else {
                    self.kill(k, LiveWhy::Error);
                }
            }
        }
    }

    /// 收一张新卡。同一运行满了 ⇒ 先挤最老的说完的，没有就挤最老的；整台满了 ⇒ 先挤最老的说完的、再挤最老的子运行的；
    /// 只剩正在说的主运行 ⇒ 不挤，新来的是主运行照收、是子运行不收（回 `false`）。
    fn admit(&mut self, k: u64, card: Card) -> bool {
        let mut mine: Vec<(u64, &Card)> = self
            .cards
            .iter()
            .filter(|(_, c)| c.sid == card.sid && c.run == card.run)
            .map(|(k, c)| (*k, c))
            .collect();
        mine.sort_by_key(|(_, c)| c.born);
        if mine.len() >= LIVE_PER_RUN {
            let victim = mine
                .iter()
                .find(|(_, c)| c.stopped)
                .or(mine.first())
                .map(|(k, _)| *k);
            if let Some(v) = victim {
                self.kill(v, LiveWhy::Evicted);
            }
        }
        if self.cards.len() >= LIVE_CARDS_KEEP {
            let mut all: Vec<(u64, &Card)> = self.cards.iter().map(|(k, c)| (*k, c)).collect();
            all.sort_by_key(|(_, c)| c.born);
            let victim = all
                .iter()
                .find(|(_, c)| c.stopped)
                .or_else(|| all.iter().find(|(_, c)| c.run.is_some()))
                .map(|(k, _)| *k);
            match victim {
                Some(v) => self.kill(v, LiveWhy::Evicted),
                None if card.run.is_some() => {
                    self.dead.add(k);
                    self.note(LiveWhy::Evicted);
                    return false;
                }
                None => {}
            }
        }
        self.cards.insert(k, card);
        true
    }

    /// 说完了：等记录。
    fn settle(&mut self, k: u64) {
        let Some(c) = self.cards.get_mut(&k) else {
            return;
        };
        if c.stopped {
            return;
        }
        c.stopped = true;
        self.fan(k, LiveOp::Awaiting);
    }

    fn kill(&mut self, k: u64, why: LiveWhy) {
        let Some(card) = self.cards.remove(&k) else {
            return;
        };
        self.dead.add(k);
        for c in &mut self.conns {
            if c.next_n.contains_key(&k) {
                c.closed.insert(k, (card.sid.clone(), card.run.clone()));
            }
            c.close(k, why);
        }
        if !matches!(why, LiveWhy::Record | LiveWhy::Gone | LiveWhy::Unwatched) {
            self.note(why);
        }
    }

    fn note(&mut self, why: LiveWhy) {
        let c = self.lost.entry(why).or_default();
        *c += 1;
        let n = *c;
        if n.is_power_of_two() {
            tracing::warn!(
                "[live] 一张活卡没上屏 / 半路撤了：{why:?}（这一种第 {n} 次；累计 {:?}）",
                self.lost
            );
        }
    }

    fn fan(&mut self, k: u64, op: LiveOp) {
        let Some(sid) = self
            .cards
            .get(&k)
            .filter(|c| c.rid.is_some())
            .map(|c| c.sid.clone())
        else {
            return;
        };
        for c in &mut self.conns {
            if c.wants(&sid) {
                c.op(k, op.clone());
            }
        }
    }

    fn fan_owe(&mut self, k: u64) {
        let Some(sid) = self
            .cards
            .get(&k)
            .filter(|c| c.rid.is_some())
            .map(|c| c.sid.clone())
        else {
            return;
        };
        for c in &mut self.conns {
            if c.wants(&sid) {
                c.owe(k);
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/stream/live_tests.rs"]
mod tests;
