//! 策略设置的声明：策略头里的 `form`。界面照它画，后端照它校验值的**形状与范围**，不认格的意思。
//! 格的种类只有五种（[`Kind`]）；号列表（`settings.accounts`）不进 `form`，固定在那里：每项是 `{start: true}` 或 `{id}`，
//! 再带 `scope: each` 的那几格。号牌不分排（§2.5），号列表的先后就是号牌的先后。

use super::script::Text;
/// 设置声明那几格（策略文件头里写的 · `rotation-view` 带的同一形）：线上形住 `cmd-wire`。
pub(crate) use cmd_wire::rotation::{
    FormCell as Cell, FormKind as Kind, FormScope as Scope, RotFrom,
};
use serde::Deserialize;
use serde_json::Value;

pub(crate) use crate::accounts::quota::rules::ACCOUNTS;

/// 一天里的分钟数（`daySegments` 的上界）。
pub(crate) const DAY_MIN: u64 = 1440;

/// 一格收不下：哪一格（`margin` · `accounts[2].delay` · `caps.5h[1]`）· 短码。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Bad {
    pub(crate) cell: String,
    /// `type` · `range` · `of` · `unknown` · `dup` · `account` · `overlap`。
    pub(crate) code: &'static str,
}

/// 声明本身对不对（读策略文件那一下）：键不重、不空、不占号列表那一格；`choice` 有选项；`inherit` 只在 `each`。
pub(crate) fn check_form(form: &[Cell]) -> Result<(), String> {
    let mut seen: Vec<&str> = Vec::new();
    for c in form {
        if c.key.is_empty() || c.key == ACCOUNTS || seen.contains(&c.key.as_str()) {
            return Err(format!(
                "form: key `{}` is empty, reserved or repeated",
                c.key
            ));
        }
        seen.push(&c.key);
        if c.kind == Kind::OneOf && c.choices.is_empty() {
            return Err(format!("form: `{}` is a choice without choices", c.key));
        }
        if c.inherit && c.scope != Scope::Each {
            return Err(format!("form: `{}` inherits but is not per account", c.key));
        }
    }
    Ok(())
}

fn num_in(v: &Value, lo: f64, hi: f64, int: bool) -> Result<(), &'static str> {
    let n = v.as_f64().ok_or("type")?;
    if int && v.as_u64().is_none() {
        return Err("type");
    }
    if n < lo || n > hi {
        return Err("range");
    }
    Ok(())
}

/// 一格的值照种类校验（`null` 只在 `nullable` 时收）。
fn value(c: &Cell, v: &Value, at: &str, out: &mut Vec<Bad>) {
    let bad = |code: &'static str| Bad {
        cell: at.to_string(),
        code,
    };
    if v.is_null() {
        if c.nullable.is_none() {
            out.push(bad("type"));
        }
        return;
    }
    let r = match c.kind {
        Kind::Percent => num_in(v, c.min.unwrap_or(0.0), c.max.unwrap_or(100.0), false),
        Kind::Minutes => num_in(v, c.min.unwrap_or(0.0), c.max.unwrap_or(f64::MAX), true),
        Kind::Toggle => v.as_bool().map(|_| ()).ok_or("type"),
        Kind::OneOf => match v.as_str() {
            Some(s) if c.choices.iter().any(|x| x.v == s) => Ok(()),
            Some(_) => Err("of"),
            None => Err("type"),
        },
        Kind::DaySegments => {
            day_segments(c, v, at, out);
            Ok(())
        }
    };
    if let Err(code) = r {
        out.push(bad(code));
    }
}

/// `daySegments`：`{窗口: [{from, to, v}]}`，每组照先后、不重叠，落在 0–1440，值在 min–max。
fn day_segments(c: &Cell, v: &Value, at: &str, out: &mut Vec<Bad>) {
    let Some(m) = v.as_object() else {
        out.push(Bad {
            cell: at.to_string(),
            code: "type",
        });
        return;
    };
    for (w, segs) in m {
        let here = format!("{at}.{w}");
        if !c.windows.is_empty() && !c.windows.iter().any(|x| x == w) {
            out.push(Bad {
                cell: here,
                code: "unknown",
            });
            continue;
        }
        let Some(a) = segs.as_array() else {
            out.push(Bad {
                cell: here,
                code: "type",
            });
            continue;
        };
        let mut last = 0;
        for (j, s) in a.iter().enumerate() {
            let cell = format!("{here}[{j}]");
            let get = |k: &str| s.get(k).and_then(Value::as_u64);
            let (Some(from), Some(to), Some(n)) = (get("from"), get("to"), s.get("v")) else {
                out.push(Bad { cell, code: "type" });
                continue;
            };
            let extra = s.as_object().is_some_and(|o| o.len() != 3);
            if extra || from >= to || to > DAY_MIN {
                out.push(Bad {
                    cell,
                    code: if extra { "unknown" } else { "range" },
                });
                continue;
            }
            if from < last {
                out.push(Bad {
                    cell: cell.clone(),
                    code: "overlap",
                });
            }
            last = to;
            if let Err(code) = num_in(n, c.min.unwrap_or(0.0), c.max.unwrap_or(100.0), false) {
                out.push(Bad { cell, code });
            }
        }
    }
}

/// ★ 照声明校验一份设置。收得下 ⇒ 空。只认形状与范围：哪一格是什么意思后端不知道。
pub(crate) fn check(form: &[Cell], settings: &Value) -> Vec<Bad> {
    let mut out = Vec::new();
    let Some(top) = settings.as_object() else {
        return vec![Bad {
            cell: String::new(),
            code: "type",
        }];
    };
    let find = |k: &str| form.iter().find(|c| c.key == k);
    for (k, v) in top {
        if k == ACCOUNTS {
            continue;
        }
        match find(k) {
            Some(c) if c.scope == Scope::All || c.inherit => value(c, v, k, &mut out),
            _ => out.push(Bad {
                cell: k.clone(),
                code: "unknown",
            }),
        }
    }
    let Some(list) = top.get(ACCOUNTS) else {
        return out;
    };
    let Some(list) = list.as_array() else {
        out.push(Bad {
            cell: ACCOUNTS.into(),
            code: "type",
        });
        return out;
    };
    let mut ids: Vec<String> = Vec::new();
    for (i, row) in list.iter().enumerate() {
        let at = format!("{ACCOUNTS}[{i}]");
        let Some(o) = row.as_object() else {
            out.push(Bad {
                cell: at,
                code: "type",
            });
            continue;
        };
        let who = match (o.get("start"), o.get("id")) {
            (Some(Value::Bool(true)), None) => Some("\u{0}start".to_string()),
            (None, Some(Value::String(id))) if crate::relay::segment_is_safe(id) => {
                Some(id.clone())
            }
            _ => None,
        };
        match who {
            None => out.push(Bad {
                cell: at.clone(),
                code: "account",
            }),
            Some(w) if ids.contains(&w) => out.push(Bad {
                cell: at.clone(),
                code: "dup",
            }),
            Some(w) => ids.push(w),
        }
        for (k, v) in o {
            if k == "start" || k == "id" {
                continue;
            }
            let here = format!("{at}.{k}");
            match find(k) {
                Some(c) if c.scope == Scope::Each => value(c, v, &here, &mut out),
                _ => out.push(Bad {
                    cell: here,
                    code: "unknown",
                }),
            }
        }
    }
    out
}

/// 讲内部结构、或用第二人称的字（文案规矩，`src/shared/rotation/form-words.json`）：策略写的标签与说明里有这些 ⇒
/// 界面上提示「这份策略的说明写法不合规」，设置照样能用。
#[derive(Deserialize)]
struct BadWords {
    zh: Vec<String>,
    en: Vec<String>,
}

fn bad_words() -> &'static BadWords {
    static W: std::sync::OnceLock<BadWords> = std::sync::OnceLock::new();
    W.get_or_init(|| {
        serde_json::from_str(include_str!("../../../../shared/rotation/form-words.json")).unwrap_or(
            BadWords {
                zh: Vec::new(),
                en: Vec::new(),
            },
        )
    })
}

fn text_bad(t: &Text) -> bool {
    let (zh, en) = match t {
        Text::One(s) => (s.as_str(), s.as_str()),
        Text::Langs { zh, en } => (zh.as_str(), en.as_deref().unwrap_or("")),
    };
    let w = bad_words();
    w.zh.iter().any(|x| zh.contains(x.as_str()))
        || en
            .split(|c: char| !c.is_ascii_alphabetic())
            .any(|x| w.en.iter().any(|b| b.eq_ignore_ascii_case(x)))
}

/// 这份声明的字合不合文案规矩（标签 · 说明 · 空的那一项 · 选项）。
pub(crate) fn words_bad(form: &[Cell]) -> bool {
    form.iter().any(|c| {
        text_bad(&c.label)
            || c.help.as_ref().is_some_and(text_bad)
            || c.nullable.as_ref().is_some_and(text_bad)
            || c.choices.iter().any(|x| text_bad(&x.label))
    })
}

fn row_of(settings: &Value, row: usize) -> Option<&serde_json::Map<String, Value>> {
    settings.get(ACCOUNTS)?.as_array()?.get(row)?.as_object()
}

/// ★ 一格此刻生效的值与它从哪来（`row` ＝ 号列表第几行；缺 ⇒ 总体那一格）。号那一行有这一格（值是 `null` 也算有）⇒ 自己的；
/// 声明说跟总体 ⇒ 总体的（总体也没设 ⇒ 声明默认）；否则声明默认。全仓只这一处解继承。
pub(crate) fn value_of(c: &Cell, settings: &Value, row: Option<usize>) -> (Value, RotFrom) {
    let total = settings.get(&c.key);
    let Some(i) = row else {
        return total.map_or((c.default.clone(), RotFrom::Default), |v| {
            (v.clone(), RotFrom::Own)
        });
    };
    if let Some(v) = row_of(settings, i).and_then(|r| r.get(&c.key)) {
        return (v.clone(), RotFrom::Own);
    }
    if c.inherit {
        (
            total.cloned().unwrap_or_else(|| c.default.clone()),
            RotFrom::All,
        )
    } else {
        (c.default.clone(), RotFrom::Default)
    }
}

fn segs_in(v: Option<&Value>, window: &str) -> Option<Vec<(u64, u64, f64)>> {
    let a = v?.get(window)?.as_array()?;
    Some(
        a.iter()
            .filter_map(|s| {
                let n = |k: &str| s.get(k).and_then(Value::as_u64);
                Some((n("from")?, n("to")?, s.get("v")?.as_f64()?))
            })
            .collect(),
    )
}

/// ★ 24 小时条某一窗此刻生效的那一组段与它从哪来（同 [`value_of`]，按窗：那一窗是段组才算设了）。
pub(crate) fn day_of(
    c: &Cell,
    settings: &Value,
    row: Option<usize>,
    window: &str,
) -> (Vec<(u64, u64, f64)>, RotFrom) {
    let total = settings.get(&c.key);
    let Some(i) = row else {
        return segs_in(total, window)
            .map_or((Vec::new(), RotFrom::Default), |s| (s, RotFrom::Own));
    };
    if let Some(s) = segs_in(row_of(settings, i).and_then(|r| r.get(&c.key)), window) {
        return (s, RotFrom::Own);
    }
    if c.inherit {
        (segs_in(total, window).unwrap_or_default(), RotFrom::All)
    } else {
        (Vec::new(), RotFrom::Default)
    }
}

/// 改设置的一个意图（`rotation-settings` 的 `edit`）：设一格 · 24 小时条上刷一段 · 拖号牌 · 加号 · 移出 · 只把基于的版本跟上。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) enum RotEdit {
    /// 设一格（`row` 缺 ⇒ 总体那一格）。
    Set {
        key: String,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        row: Option<usize>,
        #[cfg_attr(test, ts(type = "unknown"))]
        v: Value,
    },
    /// 在那一窗此刻生效的段上把 `[from, to)` 刷成 `v`（`null` ⇒ 删掉那一截；`to ≤ from` ⇒ 跨午夜），写成那一行自己的。
    Day {
        key: String,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        row: Option<usize>,
        window: String,
        #[cfg_attr(test, ts(type = "number"))]
        from: u64,
        #[cfg_attr(test, ts(type = "number"))]
        to: u64,
        #[cfg_attr(test, ts(type = "number | null"))]
        v: Option<f64>,
    },
    /// 拖号牌：第 `from` 行松在第 `to` 行上 ⇒ 排到它前头。
    Move { from: usize, to: usize },
    /// 加一个号（排在最后）。
    Add { account: String },
    /// 移出一行。
    Remove { row: usize },
}

/// 数照它的样子落盘：整数写成整数（同界面交来的、文件里写的那一形）。
fn num(x: f64) -> Value {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        Value::from(x as i64)
    } else {
        Value::from(x)
    }
}

/// 照意图改出整份新设置（形状照声明的校验另做：[`check`]）。行号越界 · 那一格不是 24 小时条 · 分钟越界 ⇒ 那一句（入参不对）。
pub(crate) fn apply(form: &[Cell], settings: &Value, edit: &RotEdit) -> Result<Value, String> {
    let mut out = settings.clone();
    if !out.is_object() {
        out = Value::Object(Default::default());
    }
    let top = out.as_object_mut().ok_or("settings is not an object")?;
    let rows = || -> Vec<Value> {
        settings
            .get(ACCOUNTS)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    fn no_row(i: usize) -> String {
        format!("no row {i} in `accounts`")
    }
    fn row_mut(
        top: &mut serde_json::Map<String, Value>,
        i: usize,
    ) -> Result<&mut serde_json::Map<String, Value>, String> {
        top.get_mut(ACCOUNTS)
            .and_then(Value::as_array_mut)
            .and_then(|a| a.get_mut(i))
            .and_then(Value::as_object_mut)
            .ok_or_else(|| no_row(i))
    }
    match edit {
        RotEdit::Set { key, row: None, v } => {
            top.insert(key.clone(), v.clone());
        }
        RotEdit::Set {
            key,
            row: Some(i),
            v,
        } => {
            row_mut(top, *i)?.insert(key.clone(), v.clone());
        }
        RotEdit::Day {
            key,
            row,
            window,
            from,
            to,
            v,
        } => {
            let c = form
                .iter()
                .find(|c| c.key == *key && c.kind == Kind::DaySegments)
                .ok_or_else(|| format!("`{key}` is not a daySegments cell"))?;
            if *from > DAY_MIN || *to > DAY_MIN {
                return Err("`from` and `to` must be minutes of a day (0–1440)".into());
            }
            let (now, _) = day_of(c, settings, *row, window);
            let segs: Vec<Value> = day_paint(&now, *from, *to, *v)
                .into_iter()
                .map(|(a, b, x)| serde_json::json!({"from": a, "to": b, "v": num(x)}))
                .collect();
            let place = match row {
                None => &mut *top,
                Some(i) => row_mut(top, *i)?,
            };
            let mut cell = place
                .get(key)
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            if segs.is_empty() {
                cell.remove(window);
            } else {
                cell.insert(window.clone(), Value::Array(segs));
            }
            if row.is_some() && cell.is_empty() {
                place.remove(key);
            } else {
                place.insert(key.clone(), Value::Object(cell));
            }
        }
        RotEdit::Move { from, to } => {
            let mut r = rows();
            if *from >= r.len() || *to >= r.len() {
                return Err(no_row((*from).max(*to)));
            }
            if from != to {
                let m = r.remove(*from);
                r.insert(if to > from { to - 1 } else { *to }, m);
            }
            top.insert(ACCOUNTS.into(), Value::Array(r));
        }
        RotEdit::Add { account } => {
            let mut r = rows();
            r.push(serde_json::json!({"id": account}));
            top.insert(ACCOUNTS.into(), Value::Array(r));
        }
        RotEdit::Remove { row } => {
            let mut r = rows();
            if *row >= r.len() {
                return Err(no_row(*row));
            }
            r.remove(*row);
            top.insert(ACCOUNTS.into(), Value::Array(r));
        }
    }
    Ok(out)
}

/// 24 小时条上刷一段（[`RotEdit::Day`]）：在 `segs` 上把 `[from, to)` 刷成 `v`（`null` ⇒ 删掉那一截，回到跟总体）；
/// `to ≤ from` ⇒ 跨午夜（相等 ⇒ 一整天）。回照先后排好、连着一样的并成一段的新分段。只是分段的算术，不是换号的想法。
pub(crate) fn day_paint(
    segs: &[(u64, u64, f64)],
    from: u64,
    to: u64,
    v: Option<f64>,
) -> Vec<(u64, u64, f64)> {
    let n = DAY_MIN as usize;
    let mut day: Vec<Option<f64>> = vec![None; n];
    for &(a, b, x) in segs {
        for m in a.min(DAY_MIN) as usize..b.min(DAY_MIN) as usize {
            day[m] = Some(x);
        }
    }
    let (from, to) = (from.min(DAY_MIN) as usize, to.min(DAY_MIN) as usize);
    let mut paint = |a: usize, b: usize| day[a..b].iter_mut().for_each(|m| *m = v);
    if from < to {
        paint(from, to);
    } else {
        paint(from, n);
        paint(0, to);
    }
    let mut out: Vec<(u64, u64, f64)> = Vec::new();
    let mut i = 0;
    while i < n {
        let Some(x) = day[i] else {
            i += 1;
            continue;
        };
        let mut j = i + 1;
        while j < n && day[j] == Some(x) {
            j += 1;
        }
        out.push((i as u64, j as u64, x));
        i = j;
    }
    out
}

#[cfg(test)]
#[path = "../../../../../tests/backend/accounts/quota/setup/form_tests.rs"]
mod tests;
