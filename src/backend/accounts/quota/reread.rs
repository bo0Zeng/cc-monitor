//! ★ **到点重读**（唯一一处）：额度账上一窗的重置时刻一过，手上那份读数就过时了 —— 用那个号的用量查询重读一次
//! （[`super::probe`]：起官方客户端报用量，不发消息、不开窗）。读回来照新数记账，显示照新数；
//! 读回来之前显示「读取中」；那一家查不了 · 没读成 ⇒ 账上记「这一窗这一刻没读成」（[`super::ledger::record_reread_failed`]），显示「已重置」。
//!
//! 「这个号会不会到点重读」只判一处（[`rereadable`]）：显示那一判（`show::windows_of`）与这里都照它。
//! 每个（家, 号, 那一窗, 过了的那一刻）只问一次：问过的记在进程里（[`Asked`]），问过仍是那一刻的记没成（账上，重启后照账不再问）。
//! 不是每次画都问：常驻那一路（[`watch_here`]）只在下一个到点的时刻、或额度账变了的那一下醒。

use super::ledger::Observed;
use std::collections::{BTreeMap, BTreeSet};

/// 一次要问的：哪一家（路由名）· 哪个号 · 过了的那几窗（窗口名 → 那一刻）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Due {
    pub agent: String,
    pub account: String,
    pub lapsed: BTreeMap<String, u64>,
}

/// 进程里问过的（家, 号, 窗, 那一刻）。
pub type Asked = BTreeSet<(String, String, String, u64)>;

/// ★ 这个号过了重置时刻会不会到点重读（唯一判法）：那一家查得了用量（注册表里有那一形），号是号库里的号（`_` 不顶任何一份登录，查不了）。
pub fn rereadable(agent: &str, account: &str) -> bool {
    super::name_words::is_candidate(account)
        && crate::agents::route_named(agent)
            .and_then(crate::agents::usage_of)
            .is_some()
}

/// ★ 一趟：账上过了重置时刻（再过 [`super::RESET_MARGIN_S`]，等上游换窗）、还没问过、没记过没成的那几窗，按号各问一次（记进 `asked`）；
/// `probe` 交回没成 ⇒ 交 `failed` 记账；问过了、账上仍是那一刻 ⇒ 那一趟没读回新数，也交 `failed`。
/// `can` ＝ [`rereadable`]（判据交替身）。交回下一个该醒的时刻（没有 ⇒ `None`）。
pub fn tick(
    entries: &[Observed],
    now: u64,
    can: &dyn Fn(&str, &str) -> bool,
    asked: &mut Asked,
    probe: &mut dyn FnMut(&Due) -> bool,
    failed: &mut dyn FnMut(&Due),
) -> Option<u64> {
    let mut next: Option<u64> = None;
    for o in entries {
        if !can(&o.agent, &o.account) {
            continue;
        }
        let (mut lapsed, mut stuck) = (BTreeMap::new(), BTreeMap::new());
        for w in &o.reading.windows {
            let Some(t) = w.resets_at else { continue };
            let ask_at = t.saturating_add(super::RESET_MARGIN_S);
            if now < ask_at {
                next = Some(next.map_or(ask_at, |n| n.min(ask_at)));
                continue;
            }
            if o.reread_failed_for(&w.name, Some(t)) {
                continue;
            }
            // 问过了、账上仍是过了的那一刻 ⇒ 那一趟没读回这一窗的新数：记没成（显示「已重置」），不再问。
            if !asked.insert((o.agent.clone(), o.account.clone(), w.name.clone(), t)) {
                stuck.insert(w.name.clone(), t);
                continue;
            }
            lapsed.insert(w.name.clone(), t);
        }
        let due = |lapsed| Due {
            agent: o.agent.clone(),
            account: o.account.clone(),
            lapsed,
        };
        if !stuck.is_empty() {
            failed(&due(stuck));
        }
        if !lapsed.is_empty() {
            let d = due(lapsed);
            if !probe(&d) {
                failed(&d);
            }
        }
    }
    next
}

/// 这台的账号库照家读（宿主交：额度这一块不认账号库管理）。读不懂 · 没建 ⇒ `None`。
pub type Books =
    std::sync::Arc<dyn Fn(&std::path::Path) -> Option<acct_core::AccountBook> + Send + Sync>;

/// 用（家, 号）查一次用量（[`super::probe::probe`]，号照这台的账号库找、读到的记进 `ledger`）：读成了 ⇒ 真。
fn probe_here(
    ledger: &super::ledger::Ledger,
    own: &crate::platform::fs::Own,
    books: &Books,
    agent: &str,
    account: &str,
) -> bool {
    let Some(route) = crate::agents::route_named(agent) else {
        return false;
    };
    let lib = crate::platform::paths::home_dir()
        .as_deref()
        .and_then(|h| books(h))
        .unwrap_or_default();
    let kind = crate::agents::kind_of_route(agent);
    let ask = super::probe::Ask {
        route,
        kind: kind.as_str(),
        account,
        now: crate::common::time::now_secs(),
        path: None,
    };
    match super::probe::probe(&lib, ledger, own, &ask) {
        Ok(super::probe::Probe::Read(_)) => true,
        Ok(super::probe::Probe::Unreadable(why)) => {
            tracing::warn!("[quota] 到点重读 {agent}/{account} 读不懂：{why}");
            false
        }
        Err(e) => {
            tracing::warn!("[quota] 到点重读 {agent}/{account} 没成：{}", e.said);
            false
        }
    }
}

/// 睡到额度账变了，或到 `wake` 那一刻（有的话）。叫醒的那一头没了 ⇒ `false`（线程收）。
#[expect(
    clippy::disallowed_methods,
    reason = "到点重读要在重置时刻那一下醒：那一刻没有事件来，不到点醒就一直挂「读取中」；没有要等的时刻就只等额度账变了"
)]
fn sleep_until(rx: &std::sync::mpsc::Receiver<()>, wake: Option<u64>, now: u64) -> bool {
    use std::sync::mpsc::RecvTimeoutError;
    let Some(at) = wake else {
        return rx.recv().is_ok();
    };
    let wait = std::time::Duration::from_secs(at.saturating_sub(now));
    !matches!(rx.recv_timeout(wait), Err(RecvTimeoutError::Disconnected))
}

/// 常驻那一路起来时调一次：起一条线程，照 [`tick`] 问（[`probe_here`]，号照 `books` 读的账号库找）；
/// 睡到下一个到点的时刻，或额度账变了（节点 `Reading` · `QuotaFile` · `Logins`）。进程里只起一条。
pub fn watch_here(own: crate::platform::fs::Own, books: Books) {
    static STARTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    let ledger = super::ledger::Ledger::at(
        crate::platform::paths::data_home()
            .as_deref()
            .map(super::ledger::path_under),
    );
    let (wake_tx, wake_rx) = std::sync::mpsc::channel::<()>();
    let mut nodes = super::nodes::bus().subscribe();
    let woken = std::thread::Builder::new()
        .name("quota-reread-wake".into())
        .spawn(move || loop {
            use super::nodes::Source;
            use tokio::sync::broadcast::error::RecvError;
            let wake = match nodes.blocking_recv() {
                Ok(n) => matches!(
                    n.source,
                    Source::Reading | Source::QuotaFile | Source::Logins
                ),
                Err(RecvError::Lagged(_)) => true,
                Err(RecvError::Closed) => return,
            };
            if wake && wake_tx.send(()).is_err() {
                return;
            }
        });
    if let Err(e) = woken {
        tracing::warn!("[quota] 到点重读起不来：{e}（过了重置的窗口照「读取中」等下一次查）");
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("quota-reread".into())
        .spawn(move || {
            let mut asked = Asked::new();
            loop {
                let now = crate::common::time::now_secs();
                let entries = super::ledger::answer_of(ledger.path(), now).accounts;
                let mut probed = false;
                let next = tick(
                    &entries,
                    now,
                    &rereadable,
                    &mut asked,
                    &mut |d: &Due| {
                        probed = true;
                        probe_here(&ledger, &own, &books, &d.agent, &d.account)
                    },
                    &mut |d: &Due| {
                        if let Err(e) = super::ledger::record_reread_failed(
                            &ledger,
                            &own,
                            &d.agent,
                            &d.account,
                            d.lapsed.clone(),
                        ) {
                            tracing::warn!("[quota] 重读没成那一记写不进额度账：{e}");
                        }
                    },
                );
                // 这一趟问过 ⇒ 很快再看一遍：没读回那一窗新数的记没成（显示「已重置」），不让「读取中」挂到下一个到点。
                let wake = if probed {
                    Some(now.saturating_add(super::RESET_MARGIN_S))
                } else {
                    next
                };
                if !sleep_until(&wake_rx, wake, now) {
                    return;
                }
                while wake_rx.try_recv().is_ok() {}
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("[quota] 到点重读起不来：{e}（过了重置的窗口照「读取中」等下一次查）");
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/reread_tests.rs"]
mod tests;
