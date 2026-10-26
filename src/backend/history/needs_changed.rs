//! **会话的需手动那几格变了**那一声（[`moved`]）与**哪几条会话挂着待答请求**（[`asked_sids`]）：
//! 流的 tap 订那一声推 `changed {needs, key: sid}`；watcher 订它、照此刻挂着请求的那几条改会话的点；活会话表判「在等人」也问 [`asked_sids`]。
//! 登记表本身（挂上 · 撤掉 · 答掉）住控制面（`control::asks`）：它每变一次调 [`moved`]，头一回开张时把自己的读法交进来（[`answer_asked`]）；
//! 没开张过（一次性子命令 · 常驻进程里还没人挂过）⇒ 空。注解里「没人答时自动答」那一格改了也调 [`moved`]。

use std::collections::BTreeSet;
use std::sync::OnceLock;

/// 某条会话的需手动那几格变了 ⇒ 那个会话 id。
pub(crate) fn changes() -> &'static tokio::sync::broadcast::Sender<String> {
    static TX: OnceLock<tokio::sync::broadcast::Sender<String>> = OnceLock::new();
    TX.get_or_init(|| tokio::sync::broadcast::channel::<String>(64).0)
}

/// 这条会话的需手动那几格变了（挂上 · 撤掉 · 答掉 · 「没人答时自动答」那一格改了）⇒ 推 `changed {needs, key: sid}`。
pub(crate) fn moved(sid: &str) {
    let _ = changes().send(sid.to_string());
}

/// 订那一声（watcher：它照此刻挂着请求的那几条改会话的点）。
pub(crate) fn moved_rx() -> tokio::sync::broadcast::Receiver<String> {
    changes().subscribe()
}

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
