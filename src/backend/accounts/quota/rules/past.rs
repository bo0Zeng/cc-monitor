//! 已经发生的那一截（时间轴左边实心那段）：只照记下来的事 ——
//! 这个会话的换号记录 · 每个号此刻的读数（几点重置 ⇒ 窗口几点开的）。说不出的那几截空着，不编。

use super::book::{Cause, Log};
use super::{standing, Acct, WINDOW_SECS};

/// 最上那条的一段：号 ⇒ 用它；空 ⇒ 停发。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Use {
    pub(crate) from: u64,
    pub(crate) to: u64,
    pub(crate) account: Option<String>,
}

/// 每号一行的一段：事实上发得出去吗 · 这一段开头是不是刚开了新窗。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Seg {
    pub(crate) from: u64,
    pub(crate) to: u64,
    pub(crate) sendable: bool,
    pub(crate) reset: bool,
}

fn push(out: &mut Vec<Use>, from: u64, to: u64, account: Option<&str>) {
    if to <= from {
        return;
    }
    if let Some(last) = out.last_mut() {
        if last.to == from && last.account.as_deref() == account {
            last.to = to;
            return;
        }
    }
    out.push(Use {
        from,
        to,
        account: account.map(str::to_string),
    });
}

/// 这一截里的一种状态：用着某个号，或停发（到 `until`，说不出 ⇒ 空）。
#[derive(Clone)]
enum State {
    Using(String),
    Holding { on: String, until: Option<u64> },
}

/// 状态 `s` 从 `from` 起、到下一条记录 `to` 为止（`last` ＝ 后面再没有记录、`to` 是此刻；`holding` ＝ 此刻还在停）。
fn emit(out: &mut Vec<Use>, s: &State, from: u64, to: u64, last: bool, holding: bool) {
    match s {
        State::Using(a) => push(out, from, to, Some(a)),
        State::Holding { on, until } => match until {
            // 停到几点说得出、在下一条之前到点 ⇒ 停到那一刻，之后接着用原来的号。
            Some(u) if *u < to => {
                push(out, from, *u, None);
                push(out, *u, to, Some(on));
            }
            // 到下一条记录为止都还在停；最后一条且此刻已经不停了 ⇒ 几点停完说不出，空着。
            _ if !last || holding => push(out, from, to, None),
            _ => {}
        },
    }
}

/// ★ 最上那条在 `[from, now)` 里的样子：照记录切段。记录一条都没有 ⇒ 从 `since`（会话起来 / 换成这个号的那一刻）起。
/// `holding` ＝ 此刻这一判是停发。
pub(crate) fn uses(
    log: &[Log],
    current: &str,
    since: u64,
    from: u64,
    now: u64,
    holding: bool,
) -> Vec<Use> {
    let mut out = Vec::new();
    let start = if log.is_empty() {
        since.max(from)
    } else {
        from
    };
    if start >= now {
        return out;
    }
    let state_after = |l: &Log| match l.cause {
        Cause::Hold { until } => State::Holding {
            on: l.to.clone(),
            until,
        },
        _ => State::Using(l.to.clone()),
    };
    let span: Vec<&Log> = log.iter().filter(|l| l.at > start && l.at <= now).collect();
    let mut s = match log.iter().rfind(|l| l.at <= start) {
        Some(l) => state_after(l),
        None => State::Using(
            span.first()
                .map_or_else(|| current.to_string(), |l| l.from.clone()),
        ),
    };
    let mut t = start;
    for l in span {
        emit(&mut out, &s, t, l.at, false, holding);
        t = l.at;
        s = state_after(l);
    }
    emit(&mut out, &s, t, now, true, holding);
    out
}

/// ★ 一个号那一行在 `[from, now)` 里说得出的几段（这个会话的记录只拿来认「它几点被换下：发不出去了」）。
/// - 窗口开在这一截里（重置时刻 − 窗长）⇒ 那一刻起是新窗；之前那一窗只在记录里有它被换下时说得出。
/// - 此刻发得出去 ⇒ 这一窗里一直发得出去（一窗里用量只涨不跌）。
/// - 此刻发不出去 ⇒ 记录里它被换下的那一刻起发不出去、之前发得出去；记录里没有 ⇒ 说不出，空着。
pub(crate) fn lane(a: &Acct, log: &[Log], from: u64, now: u64) -> Vec<Seg> {
    let opened = (0..2)
        .filter_map(|w| {
            a.resets[w]
                .filter(|r| *r > now)
                .map(|r| r.saturating_sub(WINDOW_SECS[w]))
        })
        .filter(|o| *o > from && *o < now)
        .max();
    let out_at = |lo: u64, hi: u64| {
        log.iter()
            .rev()
            .find(|l| l.cause == Cause::Unusable && l.from == a.id && l.at >= lo && l.at < hi)
            .map(|l| l.at)
    };
    let mut segs = Vec::new();
    let seg = |from: u64, to: u64, sendable: bool, reset: bool| Seg {
        from,
        to,
        sendable,
        reset,
    };
    let base = opened.unwrap_or(from);
    if let Some(o) = opened {
        if let Some(t) = out_at(from, o) {
            if t > from {
                segs.push(seg(from, t, true, false));
            }
            segs.push(seg(t, o, false, false));
        }
    }
    if standing::sendable(a, now) {
        segs.push(seg(base, now, true, opened.is_some()));
    } else if let Some(t) = out_at(base, now) {
        if t > base {
            segs.push(seg(base, t, true, opened.is_some()));
        }
        segs.push(seg(t, now, false, opened.is_some() && t == base));
    }
    segs.retain(|s| s.to > s.from);
    segs
}

#[cfg(test)]
#[path = "../../../../../tests/backend/accounts/quota/rules/past_tests.rs"]
mod tests;
