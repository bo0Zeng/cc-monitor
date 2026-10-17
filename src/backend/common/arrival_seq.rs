//! 这台的报到序号：每当一条会话从「不活」变成「活」、或活着的会话由一个新进程接上，加一（`observe::arrivals` 记）。
//! 起会话之前（`session-open` · `bus-spawn` 回包的 `arrival.since`）· 换号重启停旧之前记下此刻的序号，之后报到的那一条才算它。

use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

/// 此刻的报到序号。
pub(crate) fn mark() -> u64 {
    SEQ.load(Ordering::SeqCst)
}

/// 又一条会话报到了 ⇒ 它的序号。
pub(crate) fn next() -> u64 {
    SEQ.fetch_add(1, Ordering::SeqCst) + 1
}
