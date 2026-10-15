//! **额度每号几行**（`quota-read` 每号带的 `rows`）：一个号一段，首行 `名 · 类型 · 标签`，其余每行一组格（键 · 值 · ↻ · 距今，缺的格不出）。
//!
//! 唯一一处行模型：桌面悬停卡、CLI `--text`（通用拼字，见 `stream/cli.rs`）、手机、skill 都只照抄这几行，不按码取字。
//! 读的是出口写好时刻字之后的回包（`faces/quota_read.rs`，同一把钟 `TextClock`）：时刻的字 `…Text` 与距今 `…RelText` 已写好；
//! 一个语义位那一格的字与语气是显示态那一处写的（`accounts/quota/show.rs::slot_words`），这里照抄。
//! 「采样那一格」只写几点（数旧带「旧」）：哪台采的是出口自己的事（它知道问的是哪台、那台叫什么）。

use crate::common::cells::{Tone, Words};
use cmd_wire::quota::{
    AccountHead, Kind, LoginState, QuotaShow, QuotaState, ResetsAt, SlotShow, UnseenHead,
};
pub(crate) use cmd_wire::quota::{Cell, Rows, Warm, WarmAct};
use copy_core::copy_text;

fn c(text: String) -> Cell {
    Cell {
        text: Words(text),
        tone: Tone::Plain,
    }
}

fn toned(text: String, tone: Tone) -> Cell {
    Cell {
        text: Words(text),
        tone,
    }
}

fn text_of(w: Option<&Words>) -> &str {
    w.map_or("", |w| w.0.as_str())
}

fn slot_label(slot: &str) -> Cell {
    c(crate::accounts::quota::name_words::slot_text(slot))
}

fn head(account: &str, kind: Kind, login: LoginState) -> Vec<Cell> {
    let mut row = vec![
        c(crate::accounts::quota::name_words::account_text(account)),
        c(if kind == Kind::Api {
            copy_text("acct.kind.api", &[])
        } else {
            copy_text("acct.kind.sub", &[])
        }),
    ];
    match login {
        LoginState::NeedsLogin => row.push(toned(copy_text("acct.tag.login", &[]), Tone::Warn)),
        LoginState::NeedsKey => row.push(toned(copy_text("acct.tag.key", &[]), Tone::Warn)),
        LoginState::Ok => {}
    }
    row
}

/// 重置那一格与距今那一格（那一时刻旁边写好的字）；已过 ⇒ `↻.. 已过`、没有距今。
fn reset_cells(at: Option<u64>, text: Option<&Words>, rel: Option<&Words>) -> Vec<Cell> {
    if at.is_none() {
        return Vec::new();
    }
    let at = text_of(text);
    match rel {
        Some(rel) => vec![
            c(copy_text("acct.reset.at", &[("at", at)])),
            c(rel.0.clone()),
        ],
        None => vec![c(copy_text("acct.reset.past", &[("at", at)]))],
    }
}

fn slot_of<'a>(show: &'a QuotaShow, slot: &str) -> Option<&'a SlotShow> {
    show.slots.iter().find(|x| x.slot == slot)
}

fn slot_row(show: &QuotaShow, slot: &str) -> Vec<Cell> {
    let Some(x) = slot_of(show, slot) else {
        return vec![slot_label(slot), c(copy_text("acct.val.none", &[]))];
    };
    let mut row = vec![slot_label(slot), toned(x.text.0.clone(), x.tone)];
    row.extend(reset_cells(
        x.resets_at,
        x.resets_at_text.as_ref(),
        x.resets_at_rel_text.as_ref(),
    ));
    row
}

fn state_row(show: &QuotaShow, reading: &ResetsAt) -> Option<Vec<Cell>> {
    let value = match show.state {
        QuotaState::Refused => toned(copy_text("acct.val.refusedOnly", &[]), Tone::Fail),
        QuotaState::ResetSinceSeen => c(copy_text("acct.val.none", &[])),
        _ => return None,
    };
    let mut row = vec![c(copy_text("acct.row.state", &[])), value];
    row.extend(reset_cells(
        reading.resets_at,
        reading.resets_at_text.as_ref(),
        reading.resets_at_rel_text.as_ref(),
    ));
    Some(row)
}

fn seen_row(a: &AccountHead) -> Vec<Cell> {
    let at = &a.seen_at_text.0;
    let value = if a.show.stale {
        toned(copy_text("acct.seen.staleShort", &[("at", at)]), Tone::Warn)
    } else {
        c(at.clone())
    };
    vec![c(copy_text("acct.row.seen", &[])), value]
}

/// 出过数的号那一段。
pub(crate) fn seen_block(a: &AccountHead) -> Rows {
    let show = &a.show;
    let mut rows = vec![head(&a.account, show.kind, show.login)];
    let windowed = show
        .limiting
        .as_deref()
        .is_some_and(|l| !l.is_empty() && slot_of(show, l).is_some());
    let reading = &a.reading.resets_at;
    if show.kind == Kind::Api {
        rows.push(state_row(show, reading).unwrap_or_else(|| {
            vec![
                c(copy_text("acct.slot.both", &[])),
                c(copy_text("acct.val.none", &[])),
                c(copy_text("acct.val.noLimit", &[])),
            ]
        }));
    } else {
        rows.push(slot_row(show, "5h"));
        rows.push(slot_row(show, "7d"));
        if !windowed {
            rows.extend(state_row(show, reading));
        }
        rows.push(if show.state == QuotaState::OverageInUse {
            vec![
                c(copy_text("acct.row.over", &[])),
                toned(copy_text("acct.val.overOn", &[]), Tone::Warn),
                c(copy_text("acct.val.overBilled", &[])),
            ]
        } else {
            vec![
                c(copy_text("acct.row.over", &[])),
                c(copy_text("acct.val.none", &[])),
            ]
        });
    }
    rows.push(seen_row(a));
    rows
}

/// 没出过数的号那一段：`— 无采样`（按量号 `5h/7d — 无采样`）。
pub(crate) fn unseen_block(u: &UnseenHead) -> Rows {
    let mut rows = vec![head(&u.account, u.kind, u.login)];
    if u.kind == Kind::Api {
        rows.push(vec![
            c(copy_text("acct.slot.both", &[])),
            c(copy_text("acct.val.none", &[])),
            c(copy_text("acct.seen.none", &[])),
        ]);
    } else {
        rows.push(vec![
            slot_label("5h"),
            c(copy_text("acct.val.none", &[])),
            c(copy_text("acct.seen.none", &[])),
        ]);
        rows.push(vec![slot_label("7d"), c(copy_text("acct.val.none", &[]))]);
    }
    rows
}

/// `quota-read` 顶上那一句：读不出 ⇒ 读答的原因那一句（读的哪份 · 原因词；没给才说「原因不明」）；一个号都没有 ⇒ 无采样；否则没有。
pub(crate) fn head_text(
    state: crate::accounts::quota::ledger::ReadState,
    empty: bool,
    reason: Option<&str>,
) -> Option<Words> {
    if state == crate::accounts::quota::ledger::ReadState::Unreadable {
        Some(Words(reason.filter(|r| !r.trim().is_empty()).map_or_else(
            || copy_text("acct.text.readFail", &[]),
            str::to_string,
        )))
    } else if empty {
        Some(Words(copy_text("acct.seen.none", &[])))
    } else {
        None
    }
}

/// 「5h 那一格」（恢复菜单 · 新会话框每个号后面那一格）：`5h 41%` · 卡着的照语义位那一格的字。
pub(crate) fn five_hour(value: &str) -> String {
    copy_text(
        "resumeMenu.account.quota",
        &[
            ("slot", &crate::accounts::quota::name_words::slot_text("5h")),
            ("value", value),
        ],
    )
}

/// `quota-read` 顶上那一格「5h」：账读不出 ⇒ `5h 读不到`（每号那一格这时没有号可挂）；否则没有。
pub(crate) fn head_five_hour(state: crate::accounts::quota::ledger::ReadState) -> Option<Words> {
    (state == crate::accounts::quota::ledger::ReadState::Unreadable)
        .then(|| Words(five_hour(&copy_text("acct.val.unreadable", &[]))))
}

/// 开窗那一判：重置时刻之后再等这么久才看（重置按整点 / 十分对齐，留余量）。
const WARM_MARGIN_S: i64 = 60;
/// 登录拿不到 ⇒ 过这么久再看。
const WARM_LOGIN_RETRY_S: i64 = 60 * 60;
/// 额度账读不懂 ⇒ 过这么久再看。
const WARM_UNREADABLE_RETRY_S: i64 = 15 * 60;

/// 出口那一下：按钟写 `at` 旁边那两格。
pub(crate) fn stamped(mut w: Warm, clock: &crate::common::time::TextClock) -> Warm {
    if let Some(t) = w.at.and_then(|t| u64::try_from(t).ok()) {
        w.at_text = Some(clock.text(t));
        w.at_rel_text = clock.rel(t);
    }
    w
}

/// ★ 一个号此刻要不要发一句让窗口开始计时（唯一一处）：`seen` ＝ 出过数的号的显示态（没出过数 ⇒ `None`）。
pub(crate) fn warm_of(login: LoginState, seen: Option<&QuotaShow>, now: i64) -> Warm {
    let wait = |at: i64, text: String| Warm {
        act: WarmAct::Wait,
        at: Some(at),
        at_text: None,
        at_rel_text: None,
        text: Words(text),
    };
    let send = |text: String| Warm {
        act: WarmAct::Send,
        at: None,
        at_text: None,
        at_rel_text: None,
        text: Words(text),
    };
    match login {
        LoginState::NeedsLogin => {
            return wait(now + WARM_LOGIN_RETRY_S, copy_text("acct.tag.login", &[]))
        }
        LoginState::NeedsKey => {
            return wait(now + WARM_LOGIN_RETRY_S, copy_text("acct.tag.key", &[]))
        }
        LoginState::Ok => {}
    }
    let Some(show) = seen else {
        return send(copy_text("acct.warm.unseen", &[]));
    };
    let ahead = |x: Option<&SlotShow>| {
        x.and_then(|x| x.resets_at)
            .and_then(|t| i64::try_from(t).ok())
            .filter(|t| *t > now)
    };
    let when = |x: Option<&SlotShow>| text_of(x.and_then(|x| x.resets_at_text.as_ref())).to_string();
    let (s5, s7) = (slot_of(show, "5h"), slot_of(show, "7d"));
    if show.state == QuotaState::Refused {
        let limiting = show
            .limiting
            .as_deref()
            .filter(|l| !l.is_empty())
            .unwrap_or("5h");
        let blocker = slot_of(show, limiting).or(s5);
        if let Some(at) = ahead(blocker) {
            return wait(
                at + WARM_MARGIN_S,
                copy_text(
                    "acct.warm.refused",
                    &[("w", limiting), ("at", &when(blocker))],
                ),
            );
        }
    }
    if let Some(x) = s7 {
        if let Some(at) = ahead(Some(x)).filter(|_| x.pct.unwrap_or(0) >= 100) {
            return wait(
                at + WARM_MARGIN_S,
                copy_text("acct.warm.weekFull", &[("at", &when(Some(x)))]),
            );
        }
    }
    if let Some(x) = s5 {
        if let Some(at) = ahead(Some(x)) {
            let pct = x.pct.unwrap_or(0).to_string();
            return wait(
                at + WARM_MARGIN_S,
                copy_text("acct.warm.ticking", &[("pct", &pct), ("at", &when(Some(x)))]),
            );
        }
        return send(copy_text("acct.warm.expired", &[]));
    }
    send(copy_text("acct.warm.noWindow", &[]))
}

/// ★ 额度账读不懂时每个号那一判：一个都不发（账上说不出它此刻什么样，「没见过就发」那一条不成立），过一阵再看；
/// 那一句照实说哪一份读不懂、为什么（`why` ＝ 顶上那一句，[`head_text`]）。
pub(crate) fn warm_unreadable(why: &str, now: i64) -> Warm {
    Warm {
        act: WarmAct::Wait,
        at: Some(now + WARM_UNREADABLE_RETRY_S),
        at_text: None,
        at_rel_text: None,
        text: Words(copy_text("acct.warm.unreadable", &[("why", why)])),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/quota_rows_tests.rs"]
mod tests;
