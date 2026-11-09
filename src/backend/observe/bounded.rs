//! 活卡与流归位共用的两样小账：先进先出、有上界的集合（[`Bounded`]），与「同一种第 1、2、4、8… 次才说一行」的计数（[`Tally`]）。

use std::collections::{BTreeMap, HashSet, VecDeque};

/// 记住几个「不再收」的段（活卡 · 流归位各一份，同一个上界）。
pub const DEAD_KEEP: usize = 256;

/// 先进先出、有上界的集合：满了挤掉最早进来的；已在里面的再放一次不改先后。
pub struct Bounded<T: std::hash::Hash + Eq + Clone> {
    order: VecDeque<T>,
    set: HashSet<T>,
    keep: usize,
}

impl<T: std::hash::Hash + Eq + Clone> Bounded<T> {
    pub fn new(keep: usize) -> Self {
        Self {
            order: VecDeque::new(),
            set: HashSet::new(),
            keep,
        }
    }

    pub fn has(&self, k: &T) -> bool {
        self.set.contains(k)
    }

    pub fn add(&mut self, k: T) {
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

/// 每种至今几次；[`Tally::bump`] 只在第 1、2、4、8… 次回「该说」。`{:?}` 印累计那一张。
pub struct Tally<K: Ord>(BTreeMap<K, u64>);

impl<K: Ord> Default for Tally<K> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<K: Ord> Tally<K> {
    /// 这一种再记一次：这是第 2 的幂次 ⇒ `Some(第几次)`（该说一行），否则 `None`。
    pub fn bump(&mut self, k: K) -> Option<u64> {
        let c = self.0.entry(k).or_default();
        *c += 1;
        c.is_power_of_two().then_some(*c)
    }
}

impl<K: Ord + std::fmt::Debug> std::fmt::Debug for Tally<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
