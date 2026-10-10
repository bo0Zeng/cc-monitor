//! **额度每号几行**（`quota-read` 每号带的 `rows`）：一个号一段，首行 `名 · 类型 · 标签`，其余每行一组格（键 · 值 · ↻ · 距今，缺的格不出）。
//!
//! 唯一一处行模型：桌面悬停卡、CLI `--text`（通用拼字，见 `control/cli_control.rs`）、手机、skill 都只照抄这几行，不按码取字。
//! 读的是出口那一遍（`common::time::with_texts`）之后的回包：时刻的字 `…Text` 与距今 `…RelText` 已写好；
//! 一个语义位那一格的字与语气是显示态那一处写的（`accounts/quota/show.rs::slot_words`），这里照抄。
//! 「采样那一格」只写几点（数旧带「旧」）：哪台采的是出口自己的事（它知道问的是哪台、那台叫什么）。

use crate::common::cells::{Tone, Words};
use copy_core::copy_text;
use serde_json::Value;

/// 行里的一格。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Cell {
    pub(crate) text: Words,
    pub(crate) tone: Tone,
}

type Rows = Vec<Vec<Cell>>;

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

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn slot_label(slot: &str) -> Cell {
    match slot {
        "5h" => c(copy_text("acct.slot.fiveHour", &[])),
        "7d" => c(copy_text("acct.slot.sevenDay", &[])),
        other => c(other.to_string()),
    }
}

fn head(v: &Value) -> Vec<Cell> {
    let account = s(v, "account");
    let mut row = vec![
        if account == "_" {
            c(copy_text("acct.home.name", &[]))
        } else {
            c(account.to_string())
        },
        c(if s(v, "kind") == "api" {
            copy_text("acct.kind.api", &[])
        } else {
            copy_text("acct.kind.sub", &[])
        }),
    ];
    if account == "_" {
        row.push(c(copy_text("acct.tag.home", &[])));
    }
    match s(v, "login") {
        "needsLogin" => row.push(toned(copy_text("acct.tag.login", &[]), Tone::Warn)),
        "needsKey" => row.push(toned(copy_text("acct.tag.key", &[]), Tone::Warn)),
        _ => {}
    }
    row
}

/// 重置那一格与距今那一格（回包里那一时刻旁边写好的 `resetsAtText` · `resetsAtRelText`）；已过 ⇒ `↻.. 已过`、没有距今。
fn reset_cells(of: &Value) -> Vec<Cell> {
    if of.get("resetsAt").and_then(Value::as_i64).is_none() {
        return Vec::new();
    }
    let at = s(of, "resetsAtText");
    match of.get("resetsAtRelText").and_then(Value::as_str) {
        Some(rel) => vec![
            c(copy_text("acct.reset.at", &[("at", at)])),
            c(rel.to_string()),
        ],
        None => vec![c(copy_text("acct.reset.past", &[("at", at)]))],
    }
}

fn slot_row(a: &Value, slot: &str) -> Vec<Cell> {
    let found = a
        .get("slots")
        .and_then(Value::as_array)
        .and_then(|xs| xs.iter().find(|x| s(x, "slot") == slot));
    let Some(x) = found else {
        return vec![slot_label(slot), c(copy_text("acct.val.none", &[]))];
    };
    let tone = serde_json::from_value(x.get("tone").cloned().unwrap_or(Value::Null))
        .unwrap_or(Tone::Plain);
    let mut row = vec![slot_label(slot), toned(s(x, "text").to_string(), tone)];
    row.extend(reset_cells(x));
    row
}

fn state_row(a: &Value) -> Option<Vec<Cell>> {
    let value = match s(a, "state") {
        "refused" => toned(copy_text("acct.val.refusedOnly", &[]), Tone::Fail),
        "resetSinceSeen" => c(copy_text("acct.val.none", &[])),
        _ => return None,
    };
    let mut row = vec![c(copy_text("acct.row.state", &[])), value];
    if let Some(r) = a.get("reading") {
        row.extend(reset_cells(r));
    }
    Some(row)
}

fn seen_row(a: &Value) -> Vec<Cell> {
    let at = s(a, "seenAtText");
    let value = if a.get("stale").and_then(Value::as_bool).unwrap_or(false) {
        toned(copy_text("acct.seen.staleShort", &[("at", at)]), Tone::Warn)
    } else {
        c(at.to_string())
    };
    vec![c(copy_text("acct.row.seen", &[])), value]
}

/// 出过数的号那一段。
fn seen_block(a: &Value) -> Rows {
    let mut rows = vec![head(a)];
    let limiting = s(a, "limiting");
    let windowed = !limiting.is_empty()
        && a.get("slots")
            .and_then(Value::as_array)
            .is_some_and(|xs| xs.iter().any(|x| s(x, "slot") == limiting));
    if s(a, "kind") == "api" {
        rows.push(state_row(a).unwrap_or_else(|| {
            vec![
                c(copy_text("acct.slot.both", &[])),
                c(copy_text("acct.val.none", &[])),
                c(copy_text("acct.val.noLimit", &[])),
            ]
        }));
    } else {
        rows.push(slot_row(a, "5h"));
        rows.push(slot_row(a, "7d"));
        if !windowed {
            rows.extend(state_row(a));
        }
        rows.push(if s(a, "state") == "overageInUse" {
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
fn unseen_block(u: &Value) -> Rows {
    let mut rows = vec![head(u)];
    if s(u, "kind") == "api" {
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
pub(crate) fn head_text(state: &str, empty: bool, reason: Option<&str>) -> Option<Words> {
    if state == "unreadable" {
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
fn five_hour(value: &str) -> String {
    copy_text(
        "resumeMenu.account.quota",
        &[
            ("slot", &copy_text("acct.slot.fiveHour", &[])),
            ("value", value),
        ],
    )
}

/// `quota-read` 顶上那一格「5h」：账读不出 ⇒ `5h 读不到`（每号那一格这时没有号可挂）；否则没有。
pub(crate) fn head_five_hour(state: &str) -> Option<Words> {
    (state == "unreadable").then(|| Words(five_hour(&copy_text("acct.val.unreadable", &[]))))
}

/// ★ 给 `quota-read` 的回包（出口那一遍之后）每号添 `rows`（读不出 ⇒ 账上本来就没有号）与 `fiveHour`
/// （出过数的订阅号才有；按量号没有分窗口 · 没出过数的号 ⇒ `null`）。顶上那一格在回包类型里（[`head_five_hour`]）。
pub(crate) fn with_rows(reply: &mut Value) {
    for (list, block) in [
        ("accounts", seen_block as fn(&Value) -> Rows),
        ("unseen", unseen_block),
    ] {
        if let Some(xs) = reply.get_mut(list).and_then(Value::as_array_mut) {
            for x in xs {
                x["rows"] = serde_json::to_value(block(x)).unwrap_or(Value::Null);
                x["fiveHour"] = if list == "unseen" || x["kind"] == "api" {
                    Value::Null
                } else {
                    let none = copy_text("acct.val.none", &[]);
                    let text = x["slots"]
                        .as_array()
                        .and_then(|xs| xs.iter().find(|s| s["slot"] == "5h"))
                        .and_then(|s| s["text"].as_str())
                        .unwrap_or(&none)
                        .to_string();
                    Value::String(five_hour(&text))
                };
            }
        }
    }
}

/// 开窗那一判：重置时刻之后再等这么久才看（重置按整点 / 十分对齐，留余量）。
const WARM_MARGIN_S: i64 = 60;
/// 登录拿不到 ⇒ 过这么久再看。
const WARM_LOGIN_RETRY_S: i64 = 60 * 60;

/// 开窗那一判的一个号：`send`（发一句让 5h 窗口开始计时）· `wait`（睡到 `at`）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum WarmAct {
    Send,
    Wait,
}

/// `warm`：quota-warm 照它办（只执行、不判）。`at` 只在 `wait` 时有（旁边由出口写好 `atText`）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Warm {
    pub(crate) act: WarmAct,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) at: Option<i64>,
    /// 为什么这样办（「5h 83% ↻14:20」「被拒（7d）↻10-12 09:00」「无采样 · 发一句才知道」…）。
    pub(crate) text: Words,
}

fn slot_of<'a>(a: &'a Value, slot: &str) -> Option<&'a Value> {
    a.get("slots")
        .and_then(Value::as_array)
        .and_then(|xs| xs.iter().find(|x| s(x, "slot") == slot))
}

/// ★ 一个号此刻要不要发一句让窗口开始计时（唯一一处；原住 quota-warm skill 的 `judge()`）。
pub(crate) fn warm_of(a: &Value, seen: bool, now: i64) -> Warm {
    let wait = |at: i64, text: String| Warm {
        act: WarmAct::Wait,
        at: Some(at),
        text: Words(text),
    };
    let send = |text: String| Warm {
        act: WarmAct::Send,
        at: None,
        text: Words(text),
    };
    match s(a, "login") {
        "needsLogin" => return wait(now + WARM_LOGIN_RETRY_S, copy_text("acct.tag.login", &[])),
        "needsKey" => return wait(now + WARM_LOGIN_RETRY_S, copy_text("acct.tag.key", &[])),
        _ => {}
    }
    if !seen {
        return send(copy_text("acct.warm.unseen", &[]));
    }
    let at_of = |x: Option<&Value>| x.and_then(|x| x.get("resetsAt")).and_then(Value::as_i64);
    let (s5, s7) = (slot_of(a, "5h"), slot_of(a, "7d"));
    if s(a, "state") == "refused" {
        let limiting = Some(s(a, "limiting"))
            .filter(|l| !l.is_empty())
            .unwrap_or("5h");
        let blocker = slot_of(a, limiting).or(s5);
        if let Some(at) = at_of(blocker).filter(|t| *t > now) {
            let when = blocker.map_or("", |b| s(b, "resetsAtText"));
            return wait(
                at + WARM_MARGIN_S,
                copy_text("acct.warm.refused", &[("w", limiting), ("at", when)]),
            );
        }
    }
    if let Some(x) = s7 {
        let full = x.get("pct").and_then(Value::as_u64).unwrap_or(0) >= 100;
        if let Some(at) = at_of(Some(x)).filter(|t| full && *t > now) {
            return wait(
                at + WARM_MARGIN_S,
                copy_text("acct.warm.weekFull", &[("at", s(x, "resetsAtText"))]),
            );
        }
    }
    if let Some(x) = s5 {
        if let Some(at) = at_of(Some(x)).filter(|t| *t > now) {
            let pct = x
                .get("pct")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                .to_string();
            return wait(
                at + WARM_MARGIN_S,
                copy_text(
                    "acct.warm.ticking",
                    &[("pct", &pct), ("at", s(x, "resetsAtText"))],
                ),
            );
        }
        return send(copy_text("acct.warm.expired", &[]));
    }
    send(copy_text("acct.warm.noWindow", &[]))
}

/// ★ 给 `quota-read` 的回包（出口那一遍之后）每号添 `warm`；`at` 旁边的字按看的那一台的时区写好。
pub(crate) fn with_warm(reply: &mut Value, tz: &crate::Tz) {
    let now = reply.get("now").and_then(Value::as_i64).unwrap_or_default();
    for (list, seen) in [("accounts", true), ("unseen", false)] {
        if let Some(xs) = reply.get_mut(list).and_then(Value::as_array_mut) {
            for x in xs {
                let mut w = serde_json::to_value(warm_of(x, seen, now)).unwrap_or(Value::Null);
                crate::common::time::with_texts_now(&mut w, u64::try_from(now).unwrap_or(0), tz);
                x["warm"] = w;
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/quota_rows_tests.rs"]
mod tests;
