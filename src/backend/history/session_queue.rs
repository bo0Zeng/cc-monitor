//! 会话输入队：往一条会话说的每一句（聊天 ➤ · 压缩 · 清空 · Skill · 别的 agent 发来的）都排进那条会话的这一条队。
//! 队住会话所在那台（主人），一条会话一份 `~/.cc-monitor/session-queue/<sid>.json`；后端自己的状态（删了会丢）。
//!
//! - **序号**：放进队那一下由主人发，`nextSeq` 落盘、单调递增，重启接着数；先后只看序号。发件端自报的几点（`atMs`）只给人看，
//!   各台的钟歪多少都不影响排队。主人自己的钟只用来判「收着的等够了没有」。
//! - **去重**：同 id 再投只入队一次（送过 · 撤过的 id 也记着，最近 [`SEEN_KEEP`] 个），回原来那一条的样子。
//! - **端内保序**：带端序号 `n` 的，前一号还没到 ⇒ 先收着（不发序号）；补上了连同后面接得上的一起排；
//!   收着超过 [`HOLD_MS`] 还没等到 ⇒ 照到达先后排。不带 `n` 的照到达先后。不起定时器：每一次读写先把等够了的排掉。
//! - **撤**：排着 · 收着 · 未送达的撤得了（未送达的撤 ＝ 从名单里拿掉）；送过的撤不了（`cancelled: false`）。
//! - **会话结束**：没送的全标未送达、留在名单里给发件人看（[`end`]）。
//! - **投递口**：[`next_due`] 交排在最前、还没送的那一条；[`delivered`] 把它摘掉。
//! - 全仓唯一的写口在这里：读—改—写在那个目录的跨进程锁里、原子换。读不出来（坏了 · 更新的版本）⇒ `queue_unreadable`，不覆盖。

use super::session_queue_wire::{
    QueueCancelled, QueueItem, QueuePut, QueuePutOutcome, QueueState, SessionQueue,
    SessionQueuePutReq,
};
use crate::common::own_state::{json_body, Doc, Read};
use crate::common::said::Said;
use crate::platform::fs::Own;
use crate::wire::spec::Fail;
use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

/// 收着的（同一端前一号没到）最多等多久（毫秒，主人这台的钟）就照到达先后排。
pub(crate) const HOLD_MS: i64 = 3_000;
/// 记住最近几个见过的 id（重发去重）。
pub(crate) const SEEN_KEEP: usize = 512;
/// 一条会话的队里最多几条（排着 · 收着 · 未送达合计）。
pub(crate) const ITEMS_MAX: usize = 256;
/// 一句最长多少字节。
pub(crate) const TEXT_MAX: usize = 64 * 1024;
/// 盘上那一份的版本（顶层整数）。
pub(crate) const VERSION: u32 = 1;

/// 队文件所在那一层（相对家目录）。
pub(crate) const DIR_REL: &str = ".cc-monitor/session-queue";

/// 那一份最大多少字节（满队 × 最长一句，留余量）。
const MAX_BYTES: u64 = 24 * 1024 * 1024;

/// 一条会话的那一份：状态（读不懂不覆盖）。
const DOC: Doc<Book> = Doc::state(".cc-monitor/session-queue/<sid>.json", MAX_BYTES);

/// 盘上那一份。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Book {
    version: u32,
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
    at_ms: i64,
    arrived_ms: i64,
    text: String,
    state: QueueState,
}

impl Default for Book {
    fn default() -> Self {
        Book {
            version: VERSION,
            next_seq: 1,
            ends: BTreeMap::new(),
            seen: VecDeque::new(),
            items: Vec::new(),
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
            return Ok(QueuePut {
                outcome: QueuePutOutcome::Seen,
                seq: None,
                place: None,
            });
        }
        if req.text.len() > TEXT_MAX {
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
            state: QueueState::Held,
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
            place: (e.state == QueueState::Queued).then(|| self.place_of(e)),
        }
    }

    /// 排着的那几条里它排第几（从 1 数）。
    fn place_of(&self, e: &Entry) -> u32 {
        let before = self
            .items
            .iter()
            .filter(|x| x.state == QueueState::Queued && x.seq < e.seq)
            .count();
        u32::try_from(before + 1).unwrap_or(u32::MAX)
    }

    /// 撤一条：在名单里（排着 · 收着 · 未送达）⇒ 拿掉、`true`；不在（送过 · 撤过 · 没有）⇒ `false`、什么都不动。
    pub(crate) fn cancel(&mut self, id: &str, now_ms: i64) -> bool {
        self.settle(now_ms);
        let Some(i) = self.items.iter().position(|e| e.id == id) else {
            return false;
        };
        self.items.remove(i);
        true
    }

    /// 会话结束：没送的全标未送达（收着的照到达先后补上序号）。
    pub(crate) fn end(&mut self, now_ms: i64) {
        self.settle(now_ms);
        while let Some(i) = self.first_held(|_| true) {
            self.admit(i);
        }
        for e in &mut self.items {
            e.state = QueueState::Undelivered;
        }
    }

    /// 排在最前、还没送的那一条。
    pub(crate) fn next_due(&self) -> Option<QueueItem> {
        self.items
            .iter()
            .filter(|e| e.state == QueueState::Queued)
            .min_by_key(|e| e.seq)
            .map(item)
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
        true
    }

    /// 名单：排着的照序号，收着的照到达先后，未送达的照序号。
    pub(crate) fn view(&self) -> SessionQueue {
        let rank = |s: QueueState| match s {
            QueueState::Queued => 0,
            QueueState::Held => 1,
            QueueState::Undelivered => 2,
        };
        let mut v: Vec<&Entry> = self.items.iter().collect();
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
        SessionQueue {
            items: v.into_iter().map(item).collect(),
        }
    }
}

fn item(e: &Entry) -> QueueItem {
    QueueItem {
        seq: e.seq,
        id: e.id.clone(),
        end: e.end.clone(),
        at_ms: e.at_ms,
        text: e.text.clone(),
        state: e.state,
    }
}

fn io_failed(s: Said) -> Fail {
    Fail::from(("io_failed", s))
}

fn unreadable(s: Said) -> Fail {
    Fail::from(("queue_unreadable", s))
}

/// 现读那一份（不在 ⇒ 空的一份；读不懂 · 版本比这一版新 ⇒ `queue_unreadable`）。
fn load(own: &Own, sid: &str) -> Result<Book, Fail> {
    let file = own
        .at_home(DIR_REL)
        .and_then(|d| d.join(&format!("{sid}.json")))
        .map_err(io_failed)?;
    match DOC.read(file.as_path()) {
        Read::Absent => Ok(Book::default()),
        Read::Present(b) if b.version > VERSION => Err(unreadable(Said::from(copy_text(
            "reason.fs.readFailed",
            &[
                ("path", &file.as_path().display().to_string()),
                ("why", &copy_text("reason.code.unsupported", &[])),
            ],
        )))),
        Read::Present(b) => Ok(b),
        Read::Unreadable(why) => Err(unreadable(why)),
    }
}

/// 锁里现读 → `f` 改 → 有变化才原子换；交 `f` 的结果。
fn with_book<R>(
    own: &Own,
    sid: &str,
    f: impl FnOnce(&mut Book) -> Result<R, Fail>,
) -> Result<R, Fail> {
    let home = own.home_dir().map_err(io_failed)?;
    let dir = own.at_home(DIR_REL).map_err(io_failed)?;
    let file = dir.join(&format!("{sid}.json")).map_err(io_failed)?;
    for d in [&home, &dir] {
        own.ensure_dir(d).map_err(|e| {
            io_failed(Said::with_raw(
                copy_text(
                    "reason.fs.writeFailed",
                    &[
                        ("path", &d.as_path().display().to_string()),
                        ("why", &copy_core::io_reason(e.kind())),
                    ],
                ),
                e,
            ))
        })?;
    }
    let held = own.lock(&dir).map_err(|e| io_failed(Said::from(e)))?;
    let mut book = load(own, sid)?;
    let before = book.clone();
    let out = f(&mut book)?;
    if book != before {
        let body = json_body(file.as_path(), &book).map_err(io_failed)?;
        own.write(&file, &held, &body).map_err(io_failed)?;
    }
    Ok(out)
}

/// `session-queue-put` 在主人这台的那一半：投一条。
pub(crate) fn put(own: &Own, req: &SessionQueuePutReq, now_ms: i64) -> Result<QueuePut, Fail> {
    with_book(own, req.sid.as_str(), |b| {
        b.put(req, now_ms).map_err(|r| {
            let code = match r {
                Refused::Full => "queue_full",
                Refused::TooLong => "text_too_long",
            };
            Fail::new(code, copy_text("reason.code.overLimit", &[]))
        })
    })
}

/// `session-queue-cancel` 在主人这台的那一半：撤一条，回撤完的整份名单。
pub(crate) fn cancel(own: &Own, sid: &str, id: &str, now_ms: i64) -> Result<QueueCancelled, Fail> {
    with_book(own, sid, |b| {
        let cancelled = b.cancel(id, now_ms);
        Ok(QueueCancelled {
            cancelled,
            queue: b.view(),
        })
    })
}

/// `session-queue-read` 在主人这台的那一半：此刻的名单（先把等够了的排掉）。
pub(crate) fn read(own: &Own, sid: &str, now_ms: i64) -> Result<SessionQueue, Fail> {
    with_book(own, sid, |b| {
        b.settle(now_ms);
        Ok(b.view())
    })
}

/// 会话结束：没送的标未送达，回此刻的名单。
#[cfg_attr(not(test), allow(dead_code))] // 投递那一步接上之前只有判据调它（下面两个同）
pub(crate) fn end(own: &Own, sid: &str, now_ms: i64) -> Result<SessionQueue, Fail> {
    with_book(own, sid, |b| {
        b.end(now_ms);
        Ok(b.view())
    })
}

/// 投递口：排在最前、还没送的那一条。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn next_due(own: &Own, sid: &str, now_ms: i64) -> Result<Option<QueueItem>, Fail> {
    with_book(own, sid, |b| {
        b.settle(now_ms);
        Ok(b.next_due())
    })
}

/// 投递口：那一条送掉了（不在排着 ⇒ `false`）。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn delivered(own: &Own, sid: &str, id: &str) -> Result<bool, Fail> {
    with_book(own, sid, |b| Ok(b.delivered(id)))
}

#[cfg(test)]
#[path = "../../../tests/backend/history/session_queue_tests.rs"]
mod tests;
