//! **远端那几台的会话清单副本**（本机后端与手机核心同一份）：副本表 · 手上那份此刻什么样 · 要问时带的记号 · 照答换副本 · 落盘的那一形。
//!
//! ```text
//! 要问那台 ⇒ [`Copies::to_ask`]：手上有、不 fresh ⇒ 直接给手上那份；否则给记号去问（问是调用方的事）
//!   问成了 ⇒ [`Copies::took`]（照答换 · 记下落下的那一刻 · 整份写进 [`Shelf`]）；没成 ⇒ [`Copies::missed`]（手上那份不动、记下那一句）
//! 只读不问 ⇒ [`Copies::kept`]
//! 副本表头一次碰那台 ⇒ 先从 [`Shelf`] 装回那台的那一份（读不懂 · 版本对不上 ⇒ 当没有）
//! ```
//!
//! - 落盘的是缓存：照那台再问一次就能重建 ⇒ 读不懂当没有、不迁移、不升级。
//! - 这里不看钟、不碰文件系统：落下的那一刻由调用方给，字节存哪由 [`Shelf`] 的实现定（本机后端：`~/.cc-monitor/cache/remote-listing/<机器>.json`）。

use std::collections::BTreeMap;
use std::sync::Mutex;

use cmd_wire::listing::{Listing, ListingHead, RawRow};
use cmd_wire::sync::{Hand, Held, Patch};
use serde::{Deserialize, Serialize};

/// 落盘那一形的版本（形状变了就加一；对不上 ⇒ 当没有）。
pub const VERSION: u32 = 1;

/// 副本的字节存哪：按机器取一份 · 存一份。取不到 ⇒ `None`；存不成由实现自己记，不往上报（缓存）。
pub trait Shelf: Send + Sync {
    fn load(&self, machine: &str) -> Option<Vec<u8>>;
    fn store(&self, machine: &str, body: &[u8]);
}

/// 一台的副本：没问成过 ⇒ `hand.held` 缺。
#[derive(Default)]
struct Slot {
    hand: Hand<ListingHead, RawRow>,
    failed: Option<String>,
    /// 手上那份落下的那一刻（毫秒）：同步没成时照它说「下面是那一刻读到的」。
    at_ms: i64,
}

/// 盘上一份（读那一形）。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    version: u32,
    at_ms: i64,
    hand: Hand<ListingHead, RawRow>,
}

/// 盘上一份（写那一形，借着手上那份不抄）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SavedRef<'a> {
    version: u32,
    at_ms: i64,
    hand: &'a Hand<ListingHead, RawRow>,
}

/// 远端那几台的副本：机器名 ⇒ 手上那份 ＋ 上一次同步没成的那一句（成了就清掉）。
#[derive(Default)]
pub struct Copies {
    book: Mutex<BTreeMap<String, Slot>>,
    shelf: Option<Box<dyn Shelf>>,
}

/// 手上那份那台的清单此刻是什么样（不去问）。
#[derive(Debug)]
pub enum Kept {
    /// 拼回来的那份。
    Got(Listing),
    /// 上一次同步没成、手上有之前那份：照出、标明是旧的（那一句 ＋ 那份落下的那一刻）。
    Stale {
        listing: Listing,
        why: String,
        at_ms: i64,
    },
    /// 从没问成过、同步又没成：只有那一句。
    Failed(String),
    /// 还没问成过、这一趟也没去问。
    Unread,
}

/// 要不要问那台。
#[derive(Debug)]
pub enum Ask {
    /// 手上有、不 fresh：就用手上那份。
    Held(Listing),
    /// 去问，带这个记号（缺 ⇒ 整份）。
    With(Option<Held>),
}

impl Copies {
    /// 落盘的副本表：头一次碰一台先从 `shelf` 装回，每次同步成了整份写回。
    pub fn on(shelf: impl Shelf + 'static) -> Copies {
        Copies {
            book: Mutex::default(),
            shelf: Some(Box::new(shelf)),
        }
    }

    /// 那一台手上那份此刻什么样（不去问）。
    pub fn kept(&self, machine: &str) -> Kept {
        let mut book = self.lock();
        let Some(c) = self.slot(&mut book, machine) else {
            return Kept::Unread;
        };
        match (&c.failed, c.hand.held.is_some()) {
            (Some(why), true) => Kept::Stale {
                listing: assembled(&c.hand),
                why: why.clone(),
                at_ms: c.at_ms,
            },
            (Some(why), false) => Kept::Failed(why.clone()),
            (None, true) => Kept::Got(assembled(&c.hand)),
            (None, false) => Kept::Unread,
        }
    }

    /// 要不要问那台：手上有、不 `fresh` ⇒ 手上那份；否则去问、带手上那份的记号。
    pub fn to_ask(&self, machine: &str, fresh: bool) -> Ask {
        let mut book = self.lock();
        match self.slot(&mut book, machine).map(|c| &c.hand) {
            Some(hand) if !fresh && hand.held.is_some() => Ask::Held(assembled(hand)),
            Some(hand) => Ask::With(hand.held.clone()),
            None => Ask::With(None),
        }
    }

    /// 问成了：照答换（没有判断）、清掉上一次没成的那一句、记下落下的那一刻、整份写进落盘处；交拼回来的那份。
    pub fn took(&self, machine: &str, patch: Patch<ListingHead, RawRow>, at_ms: i64) -> Listing {
        let mut book = self.lock();
        let slot = book.entry(machine.to_string()).or_default();
        slot.failed = None;
        slot.hand.apply(patch);
        slot.at_ms = at_ms;
        if let Some(shelf) = &self.shelf {
            if let Some(body) = encoded(slot) {
                shelf.store(machine, &body);
            }
        }
        assembled(&slot.hand)
    }

    /// 问不成：手上那份不动、记下那一句（[`Copies::kept`] 照它答），下一次问成了清掉。
    pub fn missed(&self, machine: &str, why: String) {
        let mut book = self.lock();
        match self.slot(&mut book, machine) {
            Some(slot) => slot.failed = Some(why),
            None => {
                book.insert(
                    machine.to_string(),
                    Slot {
                        failed: Some(why),
                        ..Slot::default()
                    },
                );
            }
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Slot>> {
        self.book.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 表里那一台；表里没有、落盘处有读得懂的一份 ⇒ 先装回。
    fn slot<'b>(
        &self,
        book: &'b mut BTreeMap<String, Slot>,
        machine: &str,
    ) -> Option<&'b mut Slot> {
        if !book.contains_key(machine) {
            let loaded = self
                .shelf
                .as_ref()
                .and_then(|s| s.load(machine))
                .and_then(|b| decoded(&b))?;
            book.insert(machine.to_string(), loaded);
        }
        book.get_mut(machine)
    }
}

/// 落盘那一份的字节（紧凑 JSON ＋ 收尾换行）。
fn encoded(slot: &Slot) -> Option<Vec<u8>> {
    let mut body = serde_json::to_vec(&SavedRef {
        version: VERSION,
        at_ms: slot.at_ms,
        hand: &slot.hand,
    })
    .ok()?;
    body.push(b'\n');
    Some(body)
}

/// 盘上那一份装回：读不懂 · 版本对不上 · 没有记号 ⇒ 当没有。
fn decoded(body: &[u8]) -> Option<Slot> {
    let saved: Saved = serde_json::from_slice(body).ok()?;
    if saved.version != VERSION || saved.hand.held.is_none() {
        return None;
    }
    Some(Slot {
        hand: saved.hand,
        failed: None,
        at_ms: saved.at_ms,
    })
}

/// 一台落盘的文件名（不带扩展名）：字母数字与 `-` `_` 照写，别的（连打头的 `.`）写成 `%XX`，一台一个、不会拼出路径。
pub fn file_stem(machine: &str) -> String {
    let mut out = String::with_capacity(machine.len());
    for b in machine.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    if out.is_empty() {
        out.push('%');
    }
    out
}

/// 手上那份拼回一份清单（行照 `order` 的先后）。
fn assembled(hand: &Hand<ListingHead, RawRow>) -> Listing {
    let rows = hand.in_order().cloned().collect();
    let head = hand
        .head
        .as_ref()
        .map(|h| h.info.clone())
        .unwrap_or_default();
    Listing::join(head, rows)
}

#[cfg(test)]
#[path = "../../../../tests/common/face-core/listing_copy_tests.rs"]
mod tests;
