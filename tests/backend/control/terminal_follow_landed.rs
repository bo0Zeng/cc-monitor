//! 托管那一形看屏连接的「读线落下几帧」可等口（只在测试档）：读线每落下一帧「变了」记一笔，判据等它涨了再按键（不睡、不看墙钟）。
//! 发布构建里 `Landed` 是 `control/terminal_follow.rs` 里那一份空的。

use super::Watch;
use std::sync::{Arc, Condvar, Mutex};

#[derive(Clone)]
pub(crate) struct Landed(Arc<(Mutex<u64>, Condvar)>);

impl Landed {
    pub(crate) fn new() -> Landed {
        Landed(Arc::default())
    }

    pub(crate) fn mark(&self) {
        let (n, cv) = &*self.0;
        *n.lock().unwrap_or_else(|p| p.into_inner()) += 1;
        cv.notify_all();
    }
}

impl Watch {
    /// 托管那一形读线到此刻落下了几帧「变了」（已交进通道，或并进了还没取走的那一笔）。
    pub(crate) fn landed(&self) -> u64 {
        *self.landed.0 .0.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// 等到读线落下的帧数到 `n`（至多兜底那一刻，[`guard_core::wall::CEILING`]）；到了 ⇒ `true`。
    pub(crate) fn wait_landed(&self, n: u64) -> bool {
        let (count, cv) = &*self.landed.0;
        let g = count.lock().unwrap_or_else(|p| p.into_inner());
        let (g, _) = cv
            .wait_timeout_while(g, guard_core::wall::CEILING, |c| *c < n)
            .unwrap_or_else(|p| p.into_inner());
        *g >= n
    }
}

/// 记一笔就涨一，几份拷贝记的是同一个数（读线拿一份、看屏那一头拿一份）。
#[test]
fn each_mark_counts_once_and_the_copies_share_the_count() {
    let l = Landed::new();
    let reader = l.clone();
    reader.mark();
    reader.mark();
    assert_eq!(*l.0 .0.lock().unwrap(), 2);
}
