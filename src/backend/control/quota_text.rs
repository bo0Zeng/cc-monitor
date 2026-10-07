//! 终端 `--text`：`--quota-read` 那一份回包排成给人看的字（每号一段）。CLI 面的修饰词，只给 `quota-read`。
//!
//! - **只读回包 JSON**：与界面一样是那一份回包的消费方，不碰账号域的内部类型、不判 —— 「被拒 · 用满 · 超额在兜 · 上一窗已过 ·
//!   数旧 · 卡人的窗口」都是回包里后端给的词，这里照词选字（用满 `✕`，被拒而没用满「{pct}% · 被拒」）。
//! - 行模型与界面悬停卡（`src/frontend/ui/quota-lines.ts`）同一份写法，两边锁同一份金样
//!   `tests/__fixtures__/quota-text.golden.json`（每号每行每格 ＋ 整段字逐字）。标签走文案表。
//! - 缺省仍是 JSON 进 JSON 出（给 skill / AI）；`--text` 只是给人看的那一形。

use copy_core::copy_text;
use serde_json::Value;

const DAY: i64 = 86_400;

/// 天数（自 1970-01-01）⇒ 年月日（Howard Hinnant 的 civil_from_days）。
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (era * 400 + yoe + i64::from(m <= 2), m, d)
}

/// 一个时刻按此刻与时区偏移（分钟，东正）写：当天 `HH:MM` · 当年 `MM-DD HH:MM` · 别的年 `YYYY-MM-DD HH:MM`。
pub(crate) fn fmt_at(t: i64, now: i64, tz_min: i64) -> String {
    let local = t + tz_min * 60;
    let day = local.div_euclid(DAY);
    let today = (now + tz_min * 60).div_euclid(DAY);
    let secs = local - day * DAY;
    let hm = format!("{:02}:{:02}", secs / 3600, (secs % 3600) / 60);
    if day == today {
        return hm;
    }
    let (y, m, d) = civil(day);
    if y == civil(today).0 {
        format!("{m:02}-{d:02} {hm}")
    } else {
        format!("{y}-{m:02}-{d:02} {hm}")
    }
}

/// 距今（只写未来）：`+12m` · `+1h50m` · `+2h` · `+3d`（满 24h 只写天）；已过 ⇒ `None`。分钟向上取整。
pub(crate) fn fmt_rel(t: i64, now: i64) -> Option<String> {
    let d = t - now;
    if d <= 0 {
        return None;
    }
    if d >= DAY {
        return Some(format!("+{}d", d / DAY));
    }
    let mins = (d + 59) / 60;
    if mins < 60 {
        return Some(format!("+{mins}m"));
    }
    let (h, m) = (mins / 60, mins % 60);
    Some(if m == 0 {
        format!("+{h}h")
    } else {
        format!("+{h}h{m}m")
    })
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or_default()
}

fn label(account: &str) -> String {
    if account == "_" {
        copy_text("acct.home.name", &[])
    } else {
        account.to_string()
    }
}

fn slot_label(slot: &str) -> String {
    match slot {
        "5h" => copy_text("acct.slot.fiveHour", &[]),
        "7d" => copy_text("acct.slot.sevenDay", &[]),
        other => other.to_string(),
    }
}

fn head(v: &Value) -> Vec<String> {
    let kind = if s(v, "kind") == "api" {
        copy_text("acct.kind.api", &[])
    } else {
        copy_text("acct.kind.sub", &[])
    };
    let account = s(v, "account");
    let mut row = vec![label(account), kind];
    if account == "_" {
        row.push(copy_text("acct.tag.home", &[]));
    }
    match s(v, "login") {
        "needsLogin" => row.push(copy_text("acct.tag.login", &[])),
        "needsKey" => row.push(copy_text("acct.tag.key", &[])),
        _ => {}
    }
    row
}

fn reset_cells(at: Option<i64>, now: i64, tz: i64) -> Vec<String> {
    let Some(at) = at else {
        return Vec::new();
    };
    let when = fmt_at(at, now, tz);
    match fmt_rel(at, now) {
        Some(rel) => vec![copy_text("acct.reset.at", &[("at", &when)]), rel],
        None => vec![copy_text("acct.reset.past", &[("at", &when)])],
    }
}

fn slot_row(a: &Value, slot: &str, now: i64, tz: i64) -> Vec<String> {
    let found = a
        .get("slots")
        .and_then(Value::as_array)
        .and_then(|xs| xs.iter().find(|x| s(x, "slot") == slot));
    let Some(x) = found else {
        return vec![slot_label(slot), copy_text("acct.val.none", &[])];
    };
    let here = s(a, "limiting") == slot;
    let state = s(a, "state");
    let pct = x.get("pct").and_then(Value::as_u64);
    let full = x.get("full").and_then(Value::as_bool).unwrap_or(false);
    let value = if here && state == "overageInUse" {
        copy_text("acct.val.over", &[])
    } else if here && state == "resetSinceSeen" {
        copy_text("acct.val.none", &[])
    } else if full {
        copy_text("acct.val.full", &[])
    } else if here && state == "refused" {
        match pct {
            Some(p) => copy_text("acct.val.refusedPct", &[("pct", &p.to_string())]),
            None => copy_text("acct.val.refusedOnly", &[]),
        }
    } else {
        match pct {
            None => copy_text("acct.val.none", &[]),
            Some(p) => copy_text("acct.val.pct", &[("pct", &p.to_string())]),
        }
    };
    let mut row = vec![slot_label(slot), value];
    row.extend(reset_cells(
        x.get("resetsAt").and_then(Value::as_i64),
        now,
        tz,
    ));
    row
}

fn state_row(a: &Value, now: i64, tz: i64) -> Option<Vec<String>> {
    let value = match s(a, "state") {
        "refused" => copy_text("acct.val.refusedOnly", &[]),
        "resetSinceSeen" => copy_text("acct.val.none", &[]),
        _ => return None,
    };
    let at = a
        .get("reading")
        .and_then(|r| r.get("resetsAt"))
        .and_then(Value::as_i64);
    let mut row = vec![copy_text("acct.row.state", &[]), value];
    row.extend(reset_cells(at, now, tz));
    Some(row)
}

/// 采样那一行：几点 · 哪台；数旧 ⇒ 几点 · 旧。
fn seen_row(at: i64, stale: bool, now: i64, tz: i64, machine: &str) -> Vec<String> {
    let when = fmt_at(at, now, tz);
    let value = if stale {
        copy_text("acct.seen.staleShort", &[("at", &when)])
    } else {
        copy_text("acct.seen.at", &[("at", &when), ("machine", machine)])
    };
    vec![copy_text("acct.row.seen", &[]), value]
}

/// 按量号没有分窗口的限额：`5h/7d — 无限额`。
fn no_limit_row() -> Vec<String> {
    vec![
        copy_text("acct.slot.both", &[]),
        copy_text("acct.val.none", &[]),
        copy_text("acct.val.noLimit", &[]),
    ]
}

/// 出过数的号那一段。
fn seen_block(a: &Value, now: i64, tz: i64, machine: &str) -> Vec<Vec<String>> {
    let mut rows = vec![head(a)];
    let limiting = s(a, "limiting");
    let windowed = !limiting.is_empty()
        && a.get("slots")
            .and_then(Value::as_array)
            .is_some_and(|xs| xs.iter().any(|x| s(x, "slot") == limiting));
    if s(a, "kind") == "api" {
        rows.push(state_row(a, now, tz).unwrap_or_else(no_limit_row));
    } else {
        rows.push(slot_row(a, "5h", now, tz));
        rows.push(slot_row(a, "7d", now, tz));
        if !windowed {
            rows.extend(state_row(a, now, tz));
        }
        rows.push(if s(a, "state") == "overageInUse" {
            vec![
                copy_text("acct.row.over", &[]),
                copy_text("acct.val.overOn", &[]),
                copy_text("acct.val.overBilled", &[]),
            ]
        } else {
            vec![
                copy_text("acct.row.over", &[]),
                copy_text("acct.val.none", &[]),
            ]
        });
    }
    let stale = a.get("stale").and_then(Value::as_bool).unwrap_or(false);
    let seen_at = a.get("seenAt").and_then(Value::as_i64).unwrap_or_default();
    rows.push(seen_row(seen_at, stale, now, tz, machine));
    rows
}

/// 没出过数的号那一段：`— 无采样`（按量号 `5h/7d — 无采样`）。
fn unseen_block(u: &Value) -> Vec<Vec<String>> {
    let (none, unseen) = (
        copy_text("acct.val.none", &[]),
        copy_text("acct.seen.none", &[]),
    );
    let mut rows = vec![head(u)];
    if s(u, "kind") == "api" {
        rows.push(vec![copy_text("acct.slot.both", &[]), none, unseen]);
    } else {
        rows.push(vec![slot_label("5h"), none.clone(), unseen]);
        rows.push(vec![slot_label("7d"), none]);
    }
    rows
}

/// ★ 整份回包 ⇒ 每号一段 `(号, 行)`（先出过数的、再没出过的，各按回包的次序）。读不出 ⇒ 空。
pub(crate) fn blocks(reply: &Value, tz_min: i64, machine: &str) -> Vec<(String, Vec<Vec<String>>)> {
    if s(reply, "state") == "unreadable" {
        return Vec::new();
    }
    let now = reply.get("now").and_then(Value::as_i64).unwrap_or_default();
    let list = |k: &str| {
        reply
            .get(k)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let mut out: Vec<(String, Vec<Vec<String>>)> = list("accounts")
        .iter()
        .map(|a| {
            (
                s(a, "account").to_string(),
                seen_block(a, now, tz_min, machine),
            )
        })
        .collect();
    out.extend(
        list("unseen")
            .iter()
            .map(|u| (s(u, "account").to_string(), unseen_block(u))),
    );
    out
}

/// 终端里一格占几列（汉字与全角记 2）。
fn width(cell: &str) -> usize {
    cell.chars()
        .map(|c| {
            let u = c as u32;
            let wide = matches!(u,
                0x1100..=0x115F | 0x2E80..=0x303E | 0x3041..=0x33FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF
                | 0xA000..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6);
            if wide {
                2
            } else {
                1
            }
        })
        .sum()
}

/// 一段排成字：首行各格隔两个空格；其余行缩进两格，非末格按本段同列最宽补齐，格间两个空格。
fn block_text(rows: &[Vec<String>]) -> String {
    let Some((first, body)) = rows.split_first() else {
        return String::new();
    };
    let mut widths: Vec<usize> = Vec::new();
    for r in body {
        for (i, c) in r.iter().enumerate().take(r.len().saturating_sub(1)) {
            if widths.len() <= i {
                widths.resize(i + 1, 0);
            }
            widths[i] = widths[i].max(width(c));
        }
    }
    let mut lines = vec![first.join("  ")];
    for r in body {
        let cells: Vec<String> = r
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if i + 1 < r.len() {
                    format!("{c}{}", " ".repeat(widths[i] - width(c)))
                } else {
                    c.clone()
                }
            })
            .collect();
        lines.push(format!("  {}", cells.join("  ")));
    }
    lines.join("\n")
}

/// ★ 整份回包 ⇒ 给人看的整段字（不带末尾换行）。
pub(crate) fn text(reply: &Value, tz_min: i64, machine: &str) -> String {
    if s(reply, "state") == "unreadable" {
        return copy_text("acct.text.readFail", &[]);
    }
    let bs = blocks(reply, tz_min, machine);
    if bs.is_empty() {
        return copy_text("acct.seen.none", &[]);
    }
    bs.iter()
        .map(|(_, rows)| block_text(rows))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// CLI 那一臂：回包 ⇒ 按这台此刻的本地钟排、采样那台写「本机」。
pub(crate) fn render_here(reply: &Value) -> String {
    let now = reply.get("now").and_then(Value::as_u64).unwrap_or_default();
    let tz_min = crate::platform::local_tz::offset_secs(now).unwrap_or(0) / 60;
    text(reply, tz_min, &copy_text("acct.machine.here", &[]))
}

#[cfg(test)]
#[path = "../../../tests/backend/control/quota_text_tests.rs"]
mod tests;
