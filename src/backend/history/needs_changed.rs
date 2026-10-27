//! **需手动的铃**（进程里只这一个）与**哪几条会话挂着待答请求**（[`asked_sids`]）：
//! 一条会话的需手动那一判可能变了 ⇒ [`moved`]（挂着的请求挂上 · 撤掉 · 答掉 —— 登记表 `control::asks` 调；注解里「没人答时自动答」改了；
//! 活会话表发布时那条进出等人 · 离开「活」—— `live_table::publish` 调）；不止一条会话或会话外那几项可能变了 ⇒ [`ring`]
//! （账号清单 · 某个号记「MCP 要登录」的那份文件翻了 · 一份登录作业收了 · 续令牌没成 · 计划要人看的数变了）。
//! watcher 订这一个铃：照此刻挂着请求的那几条改会话的点、给运行表配 needs，再从出方向推 `changed {needs, key?}`（不可丢）；活会话表判「在等人」也问 [`asked_sids`]。
//! 登记表头一回开张时把自己的读法交进来（[`answer_asked`]）；没开张过（一次性子命令 · 常驻进程里还没人挂过）⇒ 空。

use std::collections::BTreeSet;
use std::sync::OnceLock;

/// 铃：`Some(sid)` ＝ 那条会话；`None` ＝ 整份可能变了。
pub(crate) fn changes() -> &'static tokio::sync::broadcast::Sender<Option<String>> {
    static TX: OnceLock<tokio::sync::broadcast::Sender<Option<String>>> = OnceLock::new();
    TX.get_or_init(|| tokio::sync::broadcast::channel::<Option<String>>(64).0)
}

/// 这条会话的需手动那一判可能变了 ⇒ 推 `changed {needs, key: sid}`。
pub(crate) fn moved(sid: &str) {
    ring_for(Some(sid.to_string()));
}

/// 不止一条会话、或会话外那几项可能变了 ⇒ 推 `changed {needs}`（不带 key，整份重问）。
pub(crate) fn ring() {
    ring_for(None);
}

fn ring_for(sid: Option<String>) {
    let _ = changes().send(sid);
}

/// 订那一声（watcher）。
pub(crate) fn moved_rx() -> tokio::sync::broadcast::Receiver<Option<String>> {
    changes().subscribe()
}

/// 登记表交进来的两个读法：此刻挂着请求的会话 id · 某条会话此刻挂着的请求（挂上的先后）。
struct Reads {
    sids: fn() -> Vec<String>,
    pending: fn(&str) -> Vec<cmd_wire::needs::Ask>,
}

static ASKED: OnceLock<Reads> = OnceLock::new();

/// 待答登记表开张那一刻交它的读法（只认头一回）。
pub(crate) fn answer_asked(
    sids: fn() -> Vec<String>,
    pending: fn(&str) -> Vec<cmd_wire::needs::Ask>,
) {
    ASKED.get_or_init(|| Reads { sids, pending });
}

/// 此刻挂着请求的会话。
pub(crate) fn asked_sids() -> BTreeSet<String> {
    ASKED
        .get()
        .map(|r| (r.sids)())
        .unwrap_or_default()
        .into_iter()
        .collect()
}

/// 这条会话此刻挂着的请求（挂上的先后；watcher 照它给运行表那几项配 needs）。登记表没开张 ⇒ 空。
pub(crate) fn pending_of(sid: &str) -> Vec<cmd_wire::needs::Ask> {
    ASKED.get().map(|r| (r.pending)(sid)).unwrap_or_default()
}
