//! 带键集合的生产那一侧：业务交「这一问此刻排好的那几项」，这里按问法记一份备忘（每项摘要 ＋ 修订号），
//! 比出「没变 / 只给变了的 / 整份」（线上形 `cmd_wire::sync`）。版本从内容摘要来，业务不记版本。
//! 零定时器：备忘只在被问时更新。

use crate::common::digest::Fnv;
use cmd_wire::sync::{Head, Held, Patch, Range};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

/// 一种带键集合：问法的键（入参里决定集合的那几格，不含窗口）· 此刻排好、截好的全部项（键在这一问里唯一）。
pub trait ViewSource {
    type Head: Serialize;
    type Item: Serialize;
    fn ask_key(&self) -> String;
    fn now(&self) -> (Self::Head, Vec<(String, Self::Item)>);
}

/// 一个业务一份（进程内）：至多 `cap` 种问法的备忘，多了挤掉最久没问的那一份。
pub struct Views {
    epoch: u64,
    cap: usize,
    book: Mutex<Book>,
}

#[derive(Default)]
struct Book {
    next_id: u64,
    tick: u64,
    memos: HashMap<String, Memo>,
}

/// 一种问法上一次答出去的那一窗：每项（摘要 · 修订）、先后与头各一对；`rev` 是这份备忘最新的修订。
struct Memo {
    id: u64,
    rev: u64,
    used: u64,
    items: HashMap<String, (u64, u64)>,
    order: (u64, u64),
    head: (u64, u64),
}

impl Views {
    pub fn new(cap: usize) -> Self {
        Views {
            epoch: epoch(),
            cap: cap.max(1),
            book: Mutex::new(Book::default()),
        }
    }

    /// 答一问。记号对不上（别的进程 · 备忘被挤掉 · 换了问法）⇒ 整份；什么都没变 ⇒ `same`；
    /// 否则只给修订比手上新的那几项。进了这一窗本身算一次变（被挤进来 · 出去又回来的都进 `up`），
    /// 所以「修订不比手上新」就是「手上那一份里有、且没变」。
    pub fn answer<S: ViewSource>(
        &self,
        src: &S,
        held: Option<&Held>,
        range: Option<Range>,
    ) -> Patch<S::Head, S::Item> {
        let key = src.ask_key();
        let (info, all) = src.now();
        let total = u32::try_from(all.len()).unwrap_or(u32::MAX);
        let window: Vec<(String, S::Item)> = match range {
            Some(r) => all
                .into_iter()
                .skip(r.from as usize)
                .take(r.limit as usize)
                .collect(),
            None => all,
        };
        let digests: Vec<u64> = window.iter().map(|(_, it)| digest(it)).collect();
        let mut order_fnv = Fnv::default();
        for (k, _) in &window {
            order_fnv.part(k.as_bytes());
        }
        let head = Head { total, info };
        let head_digest = digest(&head);

        let mut book = self.book.lock().unwrap_or_else(|e| e.into_inner());
        let memo = book.memo(&key, self.cap);
        let had = held.and_then(|h| self.rev_of(h, memo));
        let next = memo.rev + 1;
        let mut items = HashMap::with_capacity(window.len());
        let mut changed = false;
        for ((k, _), d) in window.iter().zip(&digests) {
            let rev = match memo.items.get(k) {
                Some(&(d0, r0)) if d0 == *d => r0,
                _ => {
                    changed = true;
                    next
                }
            };
            items.insert(k.clone(), (*d, rev));
        }
        memo.items = items;
        changed |= bump(&mut memo.order, order_fnv.value(), next);
        changed |= bump(&mut memo.head, head_digest, next);
        if changed {
            memo.rev = next;
        }
        let token = Held(format!("{:x}.{:x}.{:x}", self.epoch, memo.id, memo.rev));
        let Some(had) = had else {
            let order = window.iter().map(|(k, _)| k.clone()).collect();
            return Patch {
                held: token,
                whole: true,
                same: false,
                order: Some(order),
                up: window.into_iter().collect(),
                head: Some(head),
            };
        };
        let newer = |&(_, rev): &(u64, u64)| rev > had;
        let order = newer(&memo.order).then(|| window.iter().map(|(k, _)| k.clone()).collect());
        let head = newer(&memo.head).then_some(head);
        let up: BTreeMap<String, S::Item> = window
            .into_iter()
            .filter(|(k, _)| memo.items.get(k).is_some_and(newer))
            .collect();
        Patch {
            held: token,
            whole: false,
            same: order.is_none() && head.is_none() && up.is_empty(),
            order,
            up,
            head,
        }
    }

    /// 记号里的修订：这一进程 · 这一份备忘 · 不比备忘新，才算数。
    fn rev_of(&self, held: &Held, memo: &Memo) -> Option<u64> {
        let mut parts = held.0.split('.').map(|p| u64::from_str_radix(p, 16).ok());
        let (Some(Some(epoch)), Some(Some(id)), Some(Some(rev)), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return None;
        };
        (epoch == self.epoch && id == memo.id && rev <= memo.rev).then_some(rev)
    }
}

impl Book {
    /// 这种问法的备忘（没有 ⇒ 新开一份、换一个号）；记一次「刚问过」，多出 `cap` 份挤掉最久没问的。
    fn memo(&mut self, key: &str, cap: usize) -> &mut Memo {
        self.tick += 1;
        let tick = self.tick;
        if !self.memos.contains_key(key) {
            while self.memos.len() >= cap {
                let Some(oldest) = self
                    .memos
                    .iter()
                    .min_by_key(|(_, m)| m.used)
                    .map(|(k, _)| k.clone())
                else {
                    break;
                };
                self.memos.remove(&oldest);
            }
            self.next_id += 1;
            let memo = Memo {
                id: self.next_id,
                rev: 0,
                used: tick,
                items: HashMap::new(),
                order: (0, 0),
                head: (0, 0),
            };
            self.memos.insert(key.to_string(), memo);
        }
        let memo = self.memos.get_mut(key).expect("刚放进去");
        memo.used = tick;
        memo
    }
}

/// 摘要变了（或还没记过：修订 0）⇒ 换成这一次的修订、答「变了」。离开这一窗的项总会改动先后，由这一格答出来。
fn bump(slot: &mut (u64, u64), digest: u64, next: u64) -> bool {
    if slot.0 == digest && slot.1 != 0 {
        return false;
    }
    *slot = (digest, next);
    true
}

fn digest<T: Serialize>(v: &T) -> u64 {
    let bytes = serde_json::to_vec(v).unwrap_or_default();
    Fnv::default().part(&bytes).value()
}

/// 这一份 `Views` 的号：进程随机种子（标准库每个进程一份）＋ 进程内计数，重启过 · 另一份都对不上。
fn epoch() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(SEQ.fetch_add(1, Ordering::Relaxed));
    h.write_u32(std::process::id());
    h.finish()
}

#[cfg(test)]
#[path = "../../../tests/backend/common/views_tests.rs"]
mod tests;
