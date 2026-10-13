//! 策略设置的声明（`轮换-v5-分层.md` §6.4.1）：策略头里的 `form`。界面照它画，后端照它校验值的**形状与范围**，不认格的意思。
//! 格的种类只有五种（[`Kind`]）；号列表（`settings.accounts`）不进 `form`，固定在那里：每项是 `{start: true}` 或 `{id}`，
//! 再带 `scope: each` 的那几格。号牌不分排（§2.5），号列表的先后就是号牌的先后。

use super::script::Text;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 格的种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "RotFormKind"
    )
)]
pub enum Kind {
    /// 0–100 的数。
    Percent,
    /// 分钟数（整数）。
    Minutes,
    /// 开 / 关。
    Toggle,
    /// 几个里选一个（线上 `choice`）。
    #[serde(rename = "choice")]
    OneOf,
    /// 24 小时条上的分段：每个窗口一组 `[{from, to, v}]`（一天里的分钟）。
    DaySegments,
}

/// 作用范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "RotFormScope"
    )
)]
pub enum Scope {
    /// 总体设置那一块画一份（值在 `settings[key]`）。
    All,
    /// 每个号展开以后画一份（值在 `settings.accounts[i][key]`）。
    Each,
}

/// `choice` 的一项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "RotFormChoice"
    )
)]
pub struct FormChoice {
    pub v: String,
    pub label: Text,
}

/// 一格。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "RotFormCell"
    )
)]
pub struct Cell {
    pub key: String,
    pub kind: Kind,
    pub scope: Scope,
    /// 只对 `scope: each`：号没另设就跟总体（总体那一份在 `settings[key]`）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(test, ts(optional, as = "Option<bool>"))]
    pub inherit: bool,
    pub label: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub help: Option<Text>,
    /// 只对 `daySegments`：哪几个窗口（`5h` · `7d`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(test, ts(optional, as = "Option<Vec<String>>"))]
    pub windows: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub unit: Option<String>,
    #[serde(default)]
    #[cfg_attr(test, ts(type = "unknown"))]
    pub default: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(test, ts(optional, as = "Option<Vec<FormChoice>>"))]
    pub choices: Vec<FormChoice>,
    /// 能是空（`null`）：空的那一项怎么叫（例如「不回切」）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub nullable: Option<Text>,
}

/// 号列表那一格的键（固定，不进 `form`）。
pub(crate) const ACCOUNTS: &str = "accounts";

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

/// 24 小时条上拖出一段（`rotation-day-paint`）：在 `segs` 上把 `[from, to)` 刷成 `v`（`null` ⇒ 删掉那一截，回到跟总体）；
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
#[path = "../../../../../tests/backend/accounts/quota/rules/form_tests.rs"]
mod tests;
