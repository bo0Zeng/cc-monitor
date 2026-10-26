//! **哪几条会话挂着待答请求**：活会话表判「在等人」· watcher 改会话的点都问 [`asked_sids`]。
//! 登记表本身（挂上 · 撤掉 · 答掉）住控制面（`control::asks`），它头一回开张时把自己的读法交进来（[`answer_asked`]）；
//! 没开张过（一次性子命令 · 常驻进程里还没人挂过）⇒ 空。

use std::collections::BTreeSet;
use std::sync::OnceLock;

/// 登记表交进来的读法：此刻挂着请求的会话 id。
static ASKED: OnceLock<fn() -> Vec<String>> = OnceLock::new();

/// 待答登记表开张那一刻交它的读法（只认头一回）。
pub(crate) fn answer_asked(read: fn() -> Vec<String>) {
    ASKED.get_or_init(|| read);
}

/// 此刻挂着请求的会话。
pub(crate) fn asked_sids() -> BTreeSet<String> {
    ASKED
        .get()
        .map(|read| read())
        .unwrap_or_default()
        .into_iter()
        .collect()
}
