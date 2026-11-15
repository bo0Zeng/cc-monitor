//! 会话输入队：往一条会话说的每一句（聊天 ➤ · 压缩 · 清空 · Skill · 别的 agent 发来的）都排进那条会话的这一条队。
//! 队住会话所在那台（主人），一条会话一份 `~/.cc-monitor/session-queue/<sid>.json`；后端自己的状态（删了会丢）。
//!
//! - **序号**：放进队那一下由主人发，`nextSeq` 落盘、单调递增，重启接着数；先后只看序号。发件端自报的几点（`atMs`）只记在盘上，
//!   各台的钟歪多少都不影响排队。主人自己的钟只用来判「收着的等够了没有」。
//! - **去重**：同 id 再投只入队一次（送过 · 撤过的 id 也记着，最近 [`SEEN_KEEP`] 个），回原来那一条的样子。
//! - **端内保序**：带端序号 `n` 的，前一号还没到 ⇒ 先收着（不发序号）；补上了连同后面接得上的一起排；
//!   收着超过 [`HOLD_MS`] 还没等到 ⇒ 照到达先后排。不带 `n` 的照到达先后。不起定时器：每一次读写先把等够了的排掉。
//! - **撤**：排着 · 收着 · 未送达的撤得了（未送达的撤 ＝ 从名单里拿掉）；投递已交出去的（在送）· 送过的撤不了（`cancelled: false`）；没见过的 id 记作撤过（先撤后到不再送）。回包带那台写好的一句。
//! - **会话结束**：没送的全标未送达、留在名单里给发件人看（[`end`]）。
//! - **投递口**（`faces/session_queue_deliver.rs` 那条线程用）：[`sids`] 这台有队的会话 · [`due`] 交排在最前、还没送的那一条
//!   （连同收着的几点等够）· [`delivered`] 把它摘掉 · [`give_up`] 送不进去标未送达 · [`end`] 会话结束 · [`mark_box`] 记下屏上开没开着框（排着的那几条那一句跟着说等需手动）· [`mark_turn`] 编辑已发的那一句在等这一轮 · [`take`] 认定要送那一下标在送 · [`untake`] 收回成排着 · [`settle_sending`] 起来时把留着在送的当已送
//!   · [`step`] 排在最前那一条此刻送不送（纯判定；不碰盘、不送字，投递那一遍住 faces 是因为送那一下要同时用 control 与 observe）。
//! - watcher 看见队目录动了：[`sids_touched`] 哪几条 · [`changed`] 推的那一帧。
//! - 全仓唯一的写口在这里：读—改—写在那个目录的跨进程锁里、原子换。读不出来（坏了 · 更新的版本）⇒ `queue_unreadable`，不覆盖。

use crate::common::own_state::{Doc, Read};
use crate::common::said::Said;
use crate::platform::fs::Own;
use cmd_wire::queue::{
    QueueCancelled, QueueItem, QueueList, QueuePut, QueuePutOutcome, QueueState, SessionQueuePutReq,
};
use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

/// 收着的（同一端前一号没到）最多等多久（毫秒，主人这台的钟）就照到达先后排。
pub(crate) const HOLD_MS: i64 = 3_000;
/// 记住最近几个见过的 id（重发去重）。
pub(crate) const SEEN_KEEP: usize = 512;
/// 名单的 `sent` 留最近几个送进去的 id。
pub(crate) const SENT_KEEP: usize = 32;
/// 一条会话的队里最多几条（排着 · 收着 · 未送达合计）。
pub(crate) const ITEMS_MAX: usize = 256;
/// 一句最长多少字节。
pub(crate) const TEXT_MAX: cap_core::Cap = cap_core::Cap {
    bytes: 64 * 1024,
    over: cap_core::Over::Reject,
};
/// 队文件所在那一层（相对家目录）。
pub const DIR_REL: &str = relay_route_core::SESSION_QUEUE_DIR_REL;

/// 一条会话的那一份：状态（读不懂 · 比这一版新 ⇒ 不覆盖；落点 · 版本 · 读上限照表，上限 ＝ 满队 × 最长一句留余量）。
const DOC: Doc<Book> = Doc::of(&relay_route_core::SESSION_QUEUE_FILE);

/// 盘上那一份（顶层 `version` 由读写口带）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Book {
    /// 下一条放进队的序号。
    next_seq: u64,
    /// 每一端已经排进队的最大端序号。
    #[serde(default)]
    ends: BTreeMap<String, u64>,
    /// 最近见过的 id（先进先出）。
    #[serde(default)]
    seen: VecDeque<String>,
    /// 还在名单里的那几条（排着 · 收着 · 未送达），照到达先后。
    #[serde(default)]
    items: Vec<Entry>,
    /// 最近送进 agent 的那几条的 id（先进先出，留 [`SENT_KEEP`] 个）。
    #[serde(default)]
    sent: VecDeque<String>,
    /// 撤过的 id（先进先出，留 [`SEEN_KEEP`] 个）：见过却不在名单 · 不在这里 ⇒ 送过了（`sent` 只留最近几个）。
    #[serde(default, skip_serializing_if = "VecDeque::is_empty")]
    cancelled: VecDeque<String>,
    /// 那条会话屏上此刻开着批准 · 问答框（投递线程每一遍读活会话表那一态记下，[`mark_box`]；变了才写盘 ⇒ 名单跟着推）：
    /// 排着的那几条那一句说「需手动 · 答完再送」。旧版不认这一格 ⇒ 照常读（不带 ＝ 没开框）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    boxed: bool,
}

/// 盘上的一条：线上那几格 ＋ 端序号 ＋ 到主人这台的时刻（主人的钟）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    seq: Option<u64>,
    id: String,
    end: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    n: Option<u64>,
    /// 发件端自报的几点（入参照交）：名单上不给人看了，盘上仍记着，同版本号的旧后端读这一份要这一格。
    at_ms: i64,
    arrived_ms: i64,
    text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    attachments: Vec<String>,
    /// 编辑已发的那一句：投递时先退到那一句之前（`control/rewind.rs`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rewind: Option<cmd_wire::rewind::Rewind>,
    state: QueueState,
    /// 标未送达时说的为什么：投递那一下交的那一句，或会话结束那一句；每条未送达都带；排着 · 收着的没有这一格。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    why: Option<String>,
    /// 编辑已发的那一句、回退那一串读屏见在出字回了「等」：从几点起在等这一轮（主人的钟，[`Book::mark_turn`]）。旧版不认这一格 ⇒ 照常读。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    turn_since_ms: Option<i64>,
    /// 等这一轮过了 [`TURN_LONG_MS`]：那一句改说久未结束。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    turn_long: bool,
    /// 投递已认定要送、交出去了（[`Book::take`]，锁里标）：名单不列、撤不了；送完摘掉，回「等」· 命令级错收回（[`Book::untake`]）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    sending: bool,
}

impl Default for Book {
    fn default() -> Self {
        Book {
            next_seq: 1,
            ends: BTreeMap::new(),
            seen: VecDeque::new(),
            items: Vec::new(),
            sent: VecDeque::new(),
            cancelled: VecDeque::new(),
            boxed: false,
        }
    }
}

/// 投一条没成的原因（[`Book::put`] 交出来，由写口换成码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refused {
    /// 队满了。
    Full,
    /// 那一句太长。
    TooLong,
}

/// 撤一条的结局（[`Book::cancel`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cancel {
    /// 撤了（或早撤过 · 先撤后到记作撤过）：这一句不会送进会话。
    Done,
    /// 投递已把它交出去、正在送：撤不回。
    Sending,
    /// 已送进会话：撤不回。
    Sent,
}

impl Cancel {
    /// 回包里那一句（会话那台写好，两端照原样画）。
    fn said(self) -> String {
        match self {
            Cancel::Done => copy_text("beQueue.cancel.done", &[]),
            Cancel::Sending => copy_text("beQueue.cancel.sending", &[]),
            Cancel::Sent => copy_text("beQueue.cancel.sent", &[]),
        }
    }
}

impl Book {
    fn saw(&self, id: &str) -> bool {
        self.seen.iter().any(|s| s == id)
    }

    fn remember(&mut self, id: &str) {
        self.seen.push_back(id.to_string());
        while self.seen.len() > SEEN_KEEP {
            self.seen.pop_front();
        }
    }

    fn was_cancelled(&self, id: &str) -> bool {
        self.cancelled.iter().any(|s| s == id)
    }

    /// 记下 `id` 撤过（之后同 id 的投回 `Seen`、说已取消）。
    fn mark_cancelled(&mut self, id: &str) {
        self.cancelled.push_back(id.to_string());
        while self.cancelled.len() > SEEN_KEEP {
            self.cancelled.pop_front();
        }
    }

    /// 把第 `i` 条放进队：发序号、记下这一端排到了第几号。
    fn admit(&mut self, i: usize) {
        let seq = self.next_seq;
        self.next_seq += 1;
        let e = &mut self.items[i];
        e.seq = Some(seq);
        e.state = QueueState::Queued;
        if let Some(n) = e.n {
            let last = self.ends.entry(e.end.clone()).or_insert(0);
            *last = (*last).max(n);
        }
    }

    /// 收着的那几条里，第一条（照到达先后）满足 `pick` 的。
    fn first_held(&self, pick: impl Fn(&Entry) -> bool) -> Option<usize> {
        self.items
            .iter()
            .enumerate()
            .filter(|(_, e)| e.state == QueueState::Held && pick(e))
            .min_by_key(|(_, e)| e.arrived_ms)
            .map(|(i, _)| i)
    }

    /// 把能排的排掉：接得上本端那一号的先排；再把收着等够了的照到达先后排。
    fn settle(&mut self, now_ms: i64) {
        loop {
            let ends = &self.ends;
            let next = self
                .first_held(|e| {
                    e.n.is_some_and(|n| n <= ends.get(&e.end).copied().unwrap_or(0) + 1)
                })
                .or_else(|| self.first_held(|e| e.arrived_ms.saturating_add(HOLD_MS) <= now_ms));
            match next {
                Some(i) => self.admit(i),
                None => break,
            }
        }
    }

    /// 投一条。
    pub(crate) fn put(
        &mut self,
        req: &SessionQueuePutReq,
        now_ms: i64,
    ) -> Result<QueuePut, Refused> {
        self.settle(now_ms);
        if let Some(i) = self.items.iter().position(|e| e.id == req.id) {
            return Ok(self.receipt(i, QueuePutOutcome::Seen));
        }
        if self.saw(&req.id) {
            let said = if self.was_cancelled(&req.id) {
                copy_text("reason.op.cancelled", &[])
            } else {
                copy_text("beQueue.said.sent", &[])
            };
            return Ok(QueuePut {
                outcome: QueuePutOutcome::Seen,
                seq: None,
                said,
            });
        }
        if req.text.len() > TEXT_MAX.as_len() {
            return Err(Refused::TooLong);
        }
        if self.items.len() >= ITEMS_MAX {
            return Err(Refused::Full);
        }
        self.remember(&req.id);
        self.items.push(Entry {
            seq: None,
            id: req.id.clone(),
            end: req.end.clone(),
            n: req.n,
            at_ms: req.at_ms,
            arrived_ms: now_ms,
            text: req.text.clone(),
            attachments: req.attachments.clone(),
            rewind: req.rewind.clone(),
            state: QueueState::Held,
            why: None,
            turn_since_ms: None,
            turn_long: false,
            sending: false,
        });
        let i = self.items.len() - 1;
        if req.n.is_none() {
            self.admit(i);
        }
        self.settle(now_ms);
        let outcome = match self.items[i].state {
            QueueState::Held => QueuePutOutcome::Held,
            _ => QueuePutOutcome::Queued,
        };
        Ok(self.receipt(i, outcome))
    }

    fn receipt(&self, i: usize, outcome: QueuePutOutcome) -> QueuePut {
        let e = &self.items[i];
        QueuePut {
            outcome,
            seq: e.seq,
            said: self.said_of(e),
        }
    }

    /// 那一条此刻在等什么（回执 · 名单每条同一句；不说第几、不分哪一端）：排着、屏上开着框 ⇒ 需手动 · 答完再送；
    /// 编辑已发的那一句在等这一轮 ⇒ 等这一轮完再退（过了 [`TURN_LONG_MS`] ⇒ 那一轮久未结束 · 可撤了改选现在退）；
    /// 别的排着 · 收着 ⇒ 发送中 · 等确认；未送达 ⇒ 标它那一下写的为什么（投递交的 · 会话结束那一句）。
    fn said_of(&self, e: &Entry) -> String {
        match e.state {
            QueueState::Queued if self.boxed => copy_text("beQueue.said.waitBox", &[]),
            QueueState::Queued if e.turn_long => copy_text("beQueue.said.turnLong", &[]),
            QueueState::Queued if e.turn_since_ms.is_some() => {
                copy_text("beQueue.said.waitTurn", &[])
            }
            QueueState::Queued | QueueState::Held => copy_text("beQueue.said.confirming", &[]),
            // 盘上没存原因的（这一格加进来之前标的）照实说没留存，不画空。
            QueueState::Undelivered => e
                .why
                .clone()
                .unwrap_or_else(|| copy_text("beQueue.said.noWhy", &[])),
        }
    }

    /// 名单里的一条（带会话那台写好的那一句）。
    fn item(&self, e: &Entry) -> QueueItem {
        QueueItem {
            seq: e.seq,
            id: e.id.clone(),
            end: e.end.clone(),
            text: e.text.clone(),
            attachments: e.attachments.clone(),
            rewind: e.rewind.clone(),
            state: e.state,
            said: self.said_of(e),
        }
    }

    /// 记下屏上此刻开没开着框（队空着 ⇒ 记成没开：免得空队跟着框开合来回写盘）。变了 ⇒ `true`。
    pub(crate) fn set_boxed(&mut self, boxed: bool) -> bool {
        let now = boxed && !self.items.is_empty();
        let changed = self.boxed != now;
        self.boxed = now;
        changed
    }

    /// 编辑已发的那一句 `id` 此刻在等这一轮（回退那一串读屏见在出字）：头一回记下几点起；等过 [`TURN_LONG_MS`] 记上久了。
    /// 不在排着 · 没变 ⇒ `false`。
    pub(crate) fn mark_turn(&mut self, id: &str, now_ms: i64) -> bool {
        let Some(e) = self
            .items
            .iter_mut()
            .find(|e| e.id == id && e.state == QueueState::Queued)
        else {
            return false;
        };
        let before = (e.turn_since_ms, e.turn_long);
        let since = *e.turn_since_ms.get_or_insert(now_ms);
        e.turn_long = now_ms >= since.saturating_add(TURN_LONG_MS);
        before != (e.turn_since_ms, e.turn_long)
    }

    /// 撤一条：在名单里（排着 · 收着 · 未送达）⇒ 拿掉、记作撤过；投递已交出去（在送）⇒ 不动；送过 ⇒ 不动；
    /// 撤过 · 没见过（先撤后到）⇒ 记作撤过（之后同 id 的投回 `Seen`、不入队）。
    pub(crate) fn cancel(&mut self, id: &str, now_ms: i64) -> Cancel {
        self.settle(now_ms);
        if let Some(i) = self.items.iter().position(|e| e.id == id) {
            if self.items[i].sending {
                return Cancel::Sending;
            }
            self.items.remove(i);
            self.boxed &= !self.items.is_empty();
            self.mark_cancelled(id);
            return Cancel::Done;
        }
        if self.saw(id) && !self.was_cancelled(id) {
            return Cancel::Sent;
        }
        if !self.saw(id) {
            self.remember(id);
            self.mark_cancelled(id);
        }
        Cancel::Done
    }

    /// 投递认定要送那一下（锁里）：`id` 还排着、没交出去 ⇒ 标在送、`true`；不在（撤了 · 已在送）⇒ `false`，不送。
    pub(crate) fn take(&mut self, id: &str) -> bool {
        match self
            .items
            .iter_mut()
            .find(|e| e.id == id && e.state == QueueState::Queued && !e.sending)
        {
            Some(e) => {
                e.sending = true;
                true
            }
            None => false,
        }
    }

    /// 送那一下回「等」· 命令级错：交出去的那一条收回成排着（下一遍再判）。
    pub(crate) fn untake(&mut self, id: &str) {
        if let Some(e) = self.items.iter_mut().find(|e| e.id == id) {
            e.sending = false;
        }
    }

    /// 盘上留着在送的（进程在送的那一刻没了）：照「不知道送没送到 ⇒ 不重送」当已送。
    pub(crate) fn settle_sending(&mut self) {
        let left: Vec<String> = self
            .items
            .iter()
            .filter(|e| e.sending)
            .map(|e| e.id.clone())
            .collect();
        for id in left {
            self.delivered(&id);
        }
    }

    /// 会话结束：没送的全标未送达（收着的照到达先后补上序号）。
    pub(crate) fn end(&mut self, now_ms: i64) {
        self.settle(now_ms);
        while let Some(i) = self.first_held(|_| true) {
            self.admit(i);
        }
        let ended = copy_text("beQueue.said.ended", &[]);
        for e in &mut self.items {
            if e.state != QueueState::Undelivered {
                e.state = QueueState::Undelivered;
                e.why = Some(ended.clone());
            }
        }
    }

    /// 排在最前、还没送的那一条。
    pub(crate) fn next_due(&self) -> Option<QueueItem> {
        self.items
            .iter()
            .filter(|e| e.state == QueueState::Queued && !e.sending)
            .min_by_key(|e| e.seq)
            .map(|e| self.item(e))
    }

    /// 那一条送不进去（恢复没成 · 附件没了 · 不归我们 · 没退成）：标未送达、留给发件人看；`why` ＝ 投递那一下说的为什么。不在排着 ⇒ `false`。
    pub(crate) fn give_up(&mut self, id: &str, why: String) -> bool {
        let Some(e) = self
            .items
            .iter_mut()
            .find(|e| e.id == id && e.state == QueueState::Queued)
        else {
            return false;
        };
        e.state = QueueState::Undelivered;
        e.why = Some(why);
        e.sending = false;
        true
    }

    /// 排队那一句（状态行 `queue` 那一格）：还在等的（排着 · 收着）合计 N 条；一条都没有 ⇒ `None`。
    pub(crate) fn waiting_line(&self) -> Option<String> {
        let n = self
            .items
            .iter()
            .filter(|e| e.state != QueueState::Undelivered && !e.sending)
            .count();
        (n > 0).then(|| copy_text("beQueue.said.waiting", &[("n", &n.to_string())]))
    }

    /// 有收着的（同一端前一号还没到）：最早那一条等够了是几点（主人的钟）。
    pub(crate) fn held_until(&self) -> Option<i64> {
        self.items
            .iter()
            .filter(|e| e.state == QueueState::Held)
            .map(|e| e.arrived_ms.saturating_add(HOLD_MS))
            .min()
    }

    /// 那一条送掉了：从队里摘掉（id 照记）。不在排着 ⇒ `false`。
    pub(crate) fn delivered(&mut self, id: &str) -> bool {
        let Some(i) = self
            .items
            .iter()
            .position(|e| e.id == id && e.state == QueueState::Queued)
        else {
            return false;
        };
        self.items.remove(i);
        self.boxed &= !self.items.is_empty();
        self.sent.push_back(id.to_string());
        while self.sent.len() > SENT_KEEP {
            self.sent.pop_front();
        }
        true
    }

    /// 名单：排着的照序号，收着的照到达先后，未送达的照序号。
    pub(crate) fn view(&self) -> QueueList {
        let rank = |s: QueueState| match s {
            QueueState::Queued => 0,
            QueueState::Held => 1,
            QueueState::Undelivered => 2,
        };
        let mut v: Vec<&Entry> = self.items.iter().filter(|e| !e.sending).collect();
        v.sort_by_key(|e| {
            (
                rank(e.state),
                if e.state == QueueState::Held {
                    e.arrived_ms as i128
                } else {
                    e.seq.unwrap_or(0) as i128
                },
            )
        });
        QueueList {
            items: v.into_iter().map(|e| self.item(e)).collect(),
            sent: self.sent.iter().cloned().collect(),
        }
    }
}

fn io_failed(s: Said) -> Said {
    crate::wire::spec::fail("io_failed", s)
}

fn unreadable(s: Said) -> Said {
    crate::wire::spec::fail("queue_unreadable", s)
}

/// 现读那一份（不在 ⇒ 空的一份；读不懂 · 版本比这一版新 ⇒ `queue_unreadable`）。
fn load(own: &Own, sid: &str) -> Result<Book, Said> {
    let file = own
        .at_home(DIR_REL)
        .and_then(|d| d.join(&format!("{sid}.json")))
        .map_err(io_failed)?;
    match DOC.read(file.as_path()) {
        Read::Absent => Ok(Book::default()),
        Read::Present(b) => Ok(b),
        Read::Unreadable(why) => Err(unreadable(why)),
    }
}

/// 这台的队目录，不在就建（连家目录那一层）：写口 · 要盯它的（状态行的耳朵）都从这里拿。
pub fn made_dir(own: &Own) -> Result<crate::platform::fs::OwnPath, Said> {
    let home = own.home_dir()?;
    let dir = own.at_home(DIR_REL)?;
    for d in [&home, &dir] {
        own.ensure_dir(d).map_err(|e| {
            Said::with_raw(
                copy_text(
                    "reason.fs.writeFailed",
                    &[
                        ("path", &d.as_path().display().to_string()),
                        ("why", &copy_core::io_reason(e.kind())),
                    ],
                ),
                e,
            )
        })?;
    }
    Ok(dir)
}

/// 锁里现读 → `f` 改 → 有变化才原子换；交 `f` 的结果。
fn with_book<R>(
    own: &Own,
    sid: &str,
    f: impl FnOnce(&mut Book) -> Result<R, Said>,
) -> Result<R, Said> {
    let dir = made_dir(own).map_err(io_failed)?;
    let held = own.lock(&dir).map_err(io_failed)?;
    let mut book = load(own, sid)?;
    let before = book.clone();
    let out = f(&mut book)?;
    if book != before {
        DOC.write_named(own, &dir, &format!("{sid}.json"), &held, &book)
            .map_err(io_failed)?;
    }
    Ok(out)
}

/// 队目录动了就叫一声的那一个（一个进程一个；队不认是谁在听）。
static LISTENER: std::sync::OnceLock<fn()> = std::sync::OnceLock::new();

/// 登记「队目录动了叫我一声」（投递线程起来时登记，`faces::session_queue_deliver::start`）。已有一个 ⇒ 不换，回 `false`。
pub fn on_moved(ring: fn()) -> bool {
    LISTENER.set(ring).is_ok()
}

/// 队目录动了（watcher 看见别的进程写了 `session-queue/` 里的一份）：叫一声登记的那一个。
pub fn moved() {
    if let Some(ring) = LISTENER.get() {
        ring();
    }
}

/// `session-queue-put` 在主人这台的那一半：投一条。
pub fn put(own: &Own, req: &SessionQueuePutReq, now_ms: i64) -> Result<QueuePut, Said> {
    with_book(own, req.sid.as_str(), |b| {
        b.put(req, now_ms).map_err(|r| {
            let code = match r {
                Refused::Full => "queue_full",
                Refused::TooLong => "text_too_long",
            };
            crate::wire::spec::fail(code, copy_text("reason.code.overLimit", &[]))
        })
    })
}

/// `session-queue-cancel` 在主人这台的那一半：撤一条，回撤完的整份名单。
pub fn cancel(own: &Own, sid: &str, id: &str, now_ms: i64) -> Result<QueueCancelled, Said> {
    with_book(own, sid, |b| {
        let c = b.cancel(id, now_ms);
        Ok(QueueCancelled {
            cancelled: c == Cancel::Done,
            said: c.said(),
            queue: b.view(),
        })
    })
}

/// `session-queue-read` 在主人这台的那一半：此刻的名单（先把等够了的排掉）。
pub fn read(own: &Own, sid: &str, now_ms: i64) -> Result<QueueList, Said> {
    with_book(own, sid, |b| {
        b.settle(now_ms);
        Ok(b.view())
    })
}

/// 排队那一句（[`Book::waiting_line`]，状态行 `queue` 那一格，先把等够了的排掉）；读不出 ⇒ `None`。
pub fn waiting_said(own: &Own, sid: &str, now_ms: i64) -> Option<String> {
    with_book(own, sid, |b| {
        b.settle(now_ms);
        Ok(b.waiting_line())
    })
    .ok()
    .flatten()
}

/// 托管终端 `id` 此刻排队那一句（状态行的 `queue` 那一格，`faces::status_line_face`）：没打标 · 没在等的 · 读不出 ⇒ `None`。
/// 瘦客户端、托管终端与会话同在一台（托管口是本机套接字）⇒ 这台就是队的主人；sid 由后端打标写进 `hosts/<id>/facts.json`。
pub fn line_of_host(own: &Own, id: &str) -> Option<String> {
    use crate::host::managed::home;
    let dir = home::dir_of(own, id).ok()?;
    let sid = home::read_facts(dir.as_path())?.sid;
    if sid.is_empty() {
        return None;
    }
    waiting_said(own, &sid, crate::common::time::now_ms() as i64)
}

/// 会话结束：没送的标未送达，回此刻的名单。
pub fn end(own: &Own, sid: &str, now_ms: i64) -> Result<QueueList, Said> {
    with_book(own, sid, |b| {
        b.end(now_ms);
        Ok(b.view())
    })
}

/// 投递口：排在最前、还没送的那一条 ＋ 还有收着的话最早那一条等够了是几点（到那一刻要再看一次）。
pub fn due(own: &Own, sid: &str, now_ms: i64) -> Result<Due, Said> {
    with_book(own, sid, |b| {
        b.settle(now_ms);
        Ok(Due {
            item: b.next_due(),
            held_until: b.held_until(),
        })
    })
}

/// [`due`] 交的那一份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Due {
    pub item: Option<QueueItem>,
    pub held_until: Option<i64>,
}

/// 投递口：那一条送不进去 ⇒ 标未送达，`why` 是投递那一下说的为什么（不在排着 ⇒ `false`）。
pub fn give_up(own: &Own, sid: &str, id: &str, why: String) -> Result<bool, Said> {
    with_book(own, sid, |b| Ok(b.give_up(id, why)))
}

/// 投递口：这台此刻有队的那几条会话（`session-queue/` 里每一份 `<sid>.json`，照名排；目录不在 · 读不了 ⇒ 空）。
pub fn sids(own: &Own) -> Vec<String> {
    let Ok(dir) = own.at_home(DIR_REL) else {
        return Vec::new();
    };
    let Ok(rd) = crate::platform::fs::Ro::read_dir(dir.as_path()) else {
        return Vec::new();
    };
    let mut out: Vec<String> = rd
        .filter_map(Result::ok)
        .filter_map(|e| sid_of_name(e.file_name().to_str()?))
        .collect();
    out.sort();
    out
}

/// `<sid>.json` ⇒ 那个 sid（别的名字：锁 · 换名上位前的旁名 ⇒ `None`）。
fn sid_of_name(name: &str) -> Option<String> {
    let sid = name.strip_suffix(".json")?;
    sid.parse::<cmd_wire::Sid>().ok().map(|_| sid.to_string())
}

/// 这台的队目录（家目录解析不出来 ⇒ `None`）：watcher 盯它。
pub fn dir_here() -> Option<std::path::PathBuf> {
    crate::platform::paths::home_dir().map(|h| crate::platform::paths::home_join(&h, DIR_REL))
}

/// 这一批文件事件里动过的那几条会话的队（`dir` 下的 `<sid>.json`；批内去重、照名排）。**纯函数**（判据直接喂路径）。
pub fn sids_touched<'a>(
    paths: impl Iterator<Item = &'a std::path::Path>,
    dir: &std::path::Path,
) -> Vec<String> {
    let out: std::collections::BTreeSet<String> = paths
        .filter(|p| p.parent() == Some(dir))
        .filter_map(|p| sid_of_name(p.file_name()?.to_str()?))
        .collect();
    out.into_iter().collect()
}

/// 一条会话的队此刻那一帧 `changed {session_queue}`（watcher 看见队目录动了就推它）：体 ＝ `session-queue-read` 在这台那一半的回包
/// （[`read`]，同一个处理器）；读不出 ⇒ 不带体（客户端重问）。
pub fn changed(own: &Own, sid: &str, now_ms: i64) -> crate::wire::Changed {
    crate::wire::Changed::SessionQueue {
        key: sid.to_string(),
        body: read(own, sid, now_ms).ok(),
    }
}

/// 投递口：那条会话屏上此刻开没开着批准 · 问答框（投递线程每一遍照活会话表记，[`Book::set_boxed`]）；变了才写盘、回 `true`。
pub fn mark_box(own: &Own, sid: &str, boxed: bool) -> Result<bool, Said> {
    with_book(own, sid, |b| Ok(b.set_boxed(boxed)))
}

/// 投递口：编辑已发的那一句在等这一轮（回退那一串回了「等」、屏上没开框）⇒ 记下几点起 · 久了没有（[`Book::mark_turn`]）；变了才写盘 ⇒ 名单跟着推。
pub fn mark_turn(own: &Own, sid: &str, id: &str, now_ms: i64) -> Result<bool, Said> {
    with_book(own, sid, |b| Ok(b.mark_turn(id, now_ms)))
}

/// 投递口：判定要送那一下，锁里把 `id` 标成在送（[`Book::take`]）；不在排着了（撤了）⇒ `false`，不送。
pub fn take(own: &Own, sid: &str, id: &str) -> Result<bool, Said> {
    with_book(own, sid, |b| Ok(b.take(id)))
}

/// 投递口：送那一下回「等」· 命令级错 ⇒ 收回成排着（[`Book::untake`]）。
pub fn untake(own: &Own, sid: &str, id: &str) -> Result<(), Said> {
    with_book(own, sid, |b| {
        b.untake(id);
        Ok(())
    })
}

/// 投递线程起来那一下：盘上留着在送的当已送（[`Book::settle_sending`]）。
pub fn settle_sending(own: &Own, sid: &str) -> Result<(), Said> {
    with_book(own, sid, |b| {
        b.settle_sending();
        Ok(())
    })
}

/// 投递口：那一条送掉了（不在排着 ⇒ `false`）。
pub fn delivered(own: &Own, sid: &str, id: &str) -> Result<bool, Said> {
    with_book(own, sid, |b| Ok(b.delivered(id)))
}

// ---------------------------------------------------------------- 投递的纯判定（投递那一遍住 `faces/session_queue_deliver.rs`）

/// 送出一条之后，那一态一直没换（那一句没起一轮：`/clear` 一类）最多等多久就送下一条（毫秒，这台的钟）。
pub const CONFIRM_MS: i64 = 15_000;

/// 送那一下回「等」（框开着 · 回退那一串要等）之后，最晚多久再试一次（毫秒）：等的那一样了结时不一定有事件来，
/// 不到点再试那一条就一直排着。
pub const WAITING_RETRY_MS: i64 = 5_000;

/// 编辑已发的那一句等这一轮完，等多久（毫秒，主人的钟）名单就改说那一轮久未结束、可撤了改选现在退：比一次前台工具调用最长的等待
/// （10 分钟）再宽一截，到这时还在出字多半是这一轮真的长，人该知道有别的出口。只改说法，不改结局（不标未送达、不替人停）。
pub const TURN_LONG_MS: i64 = 15 * 60 * 1000;

/// 一条会话此刻的样子（活会话表那一份）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    /// 活着 ⇒ `Some(那一态)`（说不清 ⇒ `Some(None)`）；不活 ⇒ `None`。
    pub doing: Option<Option<crate::agents::SessionActivity>>,
    /// 那一态从何时起（epoch ms，这台的钟）。
    pub since_ms: Option<u64>,
}

/// 投递线程替一条会话记着的（不落盘：重启之后从头看）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Memo {
    /// 上一条送出的时刻（等那一轮起来）；确认过 · 过了期限 ⇒ 清掉。
    pub sent_ms: Option<i64>,
    /// 这一段队里有东西时看见它活过（队空了就清掉）。
    pub live: bool,
    /// 上一回送那一下回了「等」：最晚这一刻再试（[`WAITING_RETRY_MS`]）；之后送成 · 被拒 · 队空了 ⇒ 清掉。
    pub retry_ms: Option<i64>,
}

/// 每条会话一份 [`Memo`]。
pub type Memos = BTreeMap<String, Memo>;

/// 这一步做什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Send,
    Wait,
    End,
}

/// 上一条送出之后那一轮起来过没有：那一态换过（起始晚于送出那一刻）· 过了期限 ⇒ 算过了。
pub fn settled(look: &Look, sent_ms: i64, now_ms: i64) -> bool {
    let moved = look
        .since_ms
        .and_then(|s| i64::try_from(s).ok())
        .is_some_and(|s| s > sent_ms);
    moved || now_ms >= sent_ms.saturating_add(CONFIRM_MS)
}

/// 投递口：排在最前那一条此刻送不送。不看会话在不在跑、也不分来源（人发的 · cc-bus 投的 · 斜杠命令 · 计划审面退回一样）：
/// - 不活：这一段队里有东西时看见它活过 ⇒ 会话结束，没送的标未送达；没看见过 ⇒ 送（送字本体恢复它，那一句当第一句）。
/// - 屏上正有批准框 · 问答框（在等人）⇒ 压着：这时送字带回车会被当成对框的回答、可能误批；框答完就送。
/// - 别的（闲着 · 在跑 · 后台在跑 · 说不清）⇒ 送。一次一条：上一条送出之后那一态没换过（那一态起始不晚于送出那一刻）、又没过
///   [`CONFIRM_MS`] ⇒ 下一条先等，免得几句粘成一行。
/// - 编辑已发的那一句也照这几条：「等这一轮完」在不在出字不看活会话表（只有后台子 agent 在跑、主 agent 空着时那一态也是在跑），
///   交给回退那一串读屏判（`control::rewind` 第 1 步：在出字 ⇒ 回「等」，照排着；空着的输入行 ⇒ 照常退）。
/// - 那台离线不归这里：发件端留着待发，接上之后照同一个编号补交。
pub fn step(look: &Look, memo: &Memo, now_ms: i64) -> Step {
    use crate::agents::SessionActivity;
    let waiting_turn = memo.sent_ms.is_some_and(|t| !settled(look, t, now_ms));
    match look.doing {
        None if memo.live => Step::End,
        None => {
            if waiting_turn {
                Step::Wait
            } else {
                Step::Send
            }
        }
        Some(Some(SessionActivity::NeedsYou)) => Step::Wait,
        Some(_) if waiting_turn => Step::Wait,
        Some(_) => Step::Send,
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/history/session_queue_tests.rs"]
mod tests;
