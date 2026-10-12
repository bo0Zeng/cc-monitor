//! 参数声明（`meta().params` / `acct_params`）的校验 ＋ 参数值照声明逐格校验（E14）。界面只照 [`Shown`]（声明 ＋ 写好的字）画格，
//! 不认得任何策略。
//!
//! 七种类型：`bool` · `minutes` · `points` · `percent` · `choice`（带 `of`）· `line`（数 · `{"5h": n, "7d": n}` · `null`）·
//! `slots`（`[{at: "HH:MM-HH:MM", line: 0..=99}]`）。约定的参数名（`lib` 与合图认得）：每号 `line`（`line`）· `slots`（`slots`）；
//! 整份 `delay`（`minutes`）· `all_out`（`choice` `["stop", "go"]`）—— 同名必须同类型。

use super::{Lang, Texts};
use serde::Serialize;
use serde_json::{Map, Value};

/// 声明的类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "DeclType"
    )
)]
pub enum Type {
    Bool,
    Minutes,
    Points,
    Percent,
    #[serde(rename = "choice")]
    OneOf,
    Line,
    Slots,
}

impl Type {
    fn of(s: &str) -> Option<Self> {
        Some(match s {
            "bool" => Type::Bool,
            "minutes" => Type::Minutes,
            "points" => Type::Points,
            "percent" => Type::Percent,
            "choice" => Type::OneOf,
            "line" => Type::Line,
            "slots" => Type::Slots,
            _ => return None,
        })
    }

    fn numeric(self) -> bool {
        matches!(self, Type::Minutes | Type::Points | Type::Percent)
    }
}

/// 约定的参数名（`lib` 与合图 · 推演认得）。
pub(crate) const LINE: &str = "line";
pub(crate) const SLOTS: &str = "slots";
pub(crate) const DELAY: &str = "delay";
pub(crate) const ALL_OUT: &str = "all_out";

/// 一项声明。`label` · `hint` 是这份策略 `texts` 里的键。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Decl {
    pub(crate) k: String,
    pub(crate) ty: Type,
    pub(crate) init: Value,
    pub(crate) label: String,
    pub(crate) of: Vec<String>,
    pub(crate) min: Option<f64>,
    pub(crate) max: Option<f64>,
    pub(crate) hint: Option<String>,
}

/// 声明是整份的还是每号的。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    Whole,
    PerAccount,
}

/// `texts()` 的回值读成两张表：`zh` · `en` 都要有、键要一样、值都是串。
pub(crate) fn texts_of(v: &Value) -> Result<Texts, String> {
    let m = v
        .as_object()
        .ok_or("texts() must return #{zh: #{…}, en: #{…}}")?;
    let side = |lang: &str| -> Result<std::collections::BTreeMap<String, String>, String> {
        let t = m
            .get(lang)
            .and_then(Value::as_object)
            .ok_or_else(|| format!("texts().{lang} must be a map"))?;
        t.iter()
            .map(|(k, v)| {
                v.as_str()
                    .map(|s| (k.clone(), s.to_string()))
                    .ok_or_else(|| format!("texts().{lang}.{k} must be a string"))
            })
            .collect()
    };
    let (zh, en) = (side("zh")?, side("en")?);
    if let Some(k) = zh.keys().find(|k| !en.contains_key(*k)) {
        return Err(format!("texts().en is missing `{k}`"));
    }
    if let Some(k) = en.keys().find(|k| !zh.contains_key(*k)) {
        return Err(format!("texts().zh is missing `{k}`"));
    }
    Ok(Texts { zh, en })
}

/// 一组声明的缺省值（按 `init`）。
pub(crate) fn init_of(decls: &[Decl]) -> Map<String, Value> {
    decls
        .iter()
        .map(|d| (d.k.clone(), d.init.clone()))
        .collect()
}

/// 读一组声明；错 ⇒ （出错那一项的键，原话）。
pub(crate) fn decls_of(
    v: Option<&Value>,
    texts: &Texts,
    scope: Scope,
) -> Result<Vec<Decl>, (String, String)> {
    let what = match scope {
        Scope::Whole => "params",
        Scope::PerAccount => "acct_params",
    };
    let Some(v) = v else {
        return Ok(Vec::new());
    };
    let arr = v
        .as_array()
        .ok_or_else(|| (String::new(), format!("meta().{what} must be an array")))?;
    let mut out: Vec<Decl> = Vec::new();
    for (i, item) in arr.iter().enumerate() {
        let m = item
            .as_object()
            .ok_or_else(|| (String::new(), format!("meta().{what}[{i}] must be a map")))?;
        let k = m
            .get("k")
            .and_then(Value::as_str)
            .filter(|k| !k.is_empty() && !k.starts_with('@'))
            .ok_or_else(|| {
                (
                    String::new(),
                    format!("meta().{what}[{i}].k must be a name"),
                )
            })?
            .to_string();
        let err = |msg: String| (k.clone(), msg);
        if out.iter().any(|d| d.k == k) {
            return Err(err(format!("`{k}` is declared twice")));
        }
        let ty = m
            .get("type")
            .and_then(Value::as_str)
            .and_then(Type::of)
            .ok_or_else(|| {
                err(format!(
                    "`{k}`.type must be one of bool · minutes · points · percent · choice · line · slots"
                ))
            })?;
        let expect = match k.as_str() {
            LINE => Some(Type::Line),
            SLOTS => Some(Type::Slots),
            DELAY => Some(Type::Minutes),
            ALL_OUT => Some(Type::OneOf),
            _ => None,
        };
        if let Some(t) = expect.filter(|t| *t != ty) {
            return Err(err(format!(
                "`{k}` is a convention name and must have type {}",
                serde_json::to_string(&t).unwrap_or_default()
            )));
        }
        let label = m
            .get("label")
            .and_then(Value::as_str)
            .ok_or_else(|| err(format!("`{k}`.label must name a key of texts()")))?
            .to_string();
        if !texts.has(&label) {
            return Err(err(format!(
                "`{k}`.label `{label}` is not in texts() (zh and en)"
            )));
        }
        let hint = match m.get("hint") {
            None | Some(Value::Null) => None,
            Some(h) => {
                let h = h
                    .as_str()
                    .ok_or_else(|| err(format!("`{k}`.hint must name a key of texts()")))?;
                if !texts.has(h) {
                    return Err(err(format!(
                        "`{k}`.hint `{h}` is not in texts() (zh and en)"
                    )));
                }
                Some(h.to_string())
            }
        };
        let num = |n: &str| -> Result<Option<f64>, (String, String)> {
            match m.get(n) {
                None | Some(Value::Null) => Ok(None),
                Some(x) => x
                    .as_f64()
                    .map(Some)
                    .ok_or_else(|| err(format!("`{k}`.{n} must be a number"))),
            }
        };
        let (min, max) = (num("min")?, num("max")?);
        if let (Some(a), Some(b)) = (min, max) {
            if a > b {
                return Err(err(format!("`{k}`: min > max")));
            }
        }
        let of: Vec<String> = match (ty, m.get("of")) {
            (Type::OneOf, Some(Value::Array(a))) if !a.is_empty() => a
                .iter()
                .map(|x| {
                    x.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| err(format!("`{k}`.of must be strings")))
                })
                .collect::<Result<_, _>>()?,
            (Type::OneOf, _) => return Err(err(format!("`{k}`.of must be a non-empty array"))),
            _ => Vec::new(),
        };
        for o in &of {
            if !texts.has(&format!("{label}_{o}")) {
                return Err(err(format!(
                    "`{k}`: option `{o}` needs texts() key `{label}_{o}` (zh and en)"
                )));
            }
        }
        if k == ALL_OUT && of != ["stop", "go"] {
            return Err(err(format!("`{k}`.of must be [\"stop\", \"go\"]")));
        }
        let mut d = Decl {
            k: k.clone(),
            ty,
            init: Value::Null,
            label,
            of,
            min,
            max,
            hint,
        };
        let init = m.get("init").cloned().unwrap_or(Value::Null);
        // 缺省值自己得过自己的校验（数字格的 `()` 只在缺省就是 `()` 时收）。
        d.init = init.clone();
        d.init = check(&d, &init)
            .map_err(|c| err(format!("`{k}`.init does not fit its own type ({})", c.0)))?;
        out.push(d);
    }
    Ok(out)
}

/// 一格为什么不收（短码，界面那一格红框、照它选字）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Code(pub(crate) &'static str);

/// 同一个窗口的线收哪些值。
const LINE_RANGE: std::ops::RangeInclusive<i64> = 1..=99;
/// 时段里的线收哪些值（多一个 `0` ＝ 这段不用）。
const SLOT_LINE_RANGE: std::ops::RangeInclusive<i64> = 0..=99;
/// 一行至多几段时段。
const MAX_SLOTS: usize = 24;

fn int_in(v: &Value, r: &std::ops::RangeInclusive<i64>) -> Option<i64> {
    v.as_i64()
        .or_else(|| v.as_f64().filter(|f| f.fract() == 0.0).map(|f| f as i64))
        .filter(|n| r.contains(n))
}

/// ★ 一格的值照声明校验，回规整过的值（`line` 两窗同值并成一个数、空表成 `null`）；不收 ⇒ 短码
/// `type` · `range` · `of` · `time` · `overlap`。时段那一格的错落在第几段由 [`check_cell`] 写进格名。
pub(crate) fn check(d: &Decl, v: &Value) -> Result<Value, Code> {
    check_cell(d, v).map_err(|(c, _)| c)
}

/// 同 [`check`]，另回出错的那一小格（`slots` 的 `[i].at` · `[i].line`；`line` 的 `.5h` · `.7d`；整格 ⇒ 空串）。
pub(crate) fn check_cell(d: &Decl, v: &Value) -> Result<Value, (Code, String)> {
    let whole = |c: &'static str| Err((Code(c), String::new()));
    let within = |n: f64| d.min.is_none_or(|a| n >= a) && d.max.is_none_or(|b| n <= b);
    match d.ty {
        Type::Bool => match v {
            Value::Bool(_) => Ok(v.clone()),
            _ => whole("type"),
        },
        t if t.numeric() => match v {
            Value::Null if d.init.is_null() => Ok(Value::Null),
            Value::Number(n) => {
                let f = n.as_f64().unwrap_or(f64::NAN);
                let pct_ok = t != Type::Percent || (0.0..=100.0).contains(&f);
                if within(f) && pct_ok && f.is_finite() {
                    Ok(v.clone())
                } else {
                    whole("range")
                }
            }
            _ => whole("type"),
        },
        Type::OneOf => match v.as_str() {
            Some(s) if d.of.iter().any(|o| o == s) => Ok(v.clone()),
            Some(_) => whole("of"),
            None => whole("type"),
        },
        Type::Line => line_value(d, v),
        Type::Slots => slots_value(v),
        _ => whole("type"),
    }
}

fn line_value(d: &Decl, v: &Value) -> Result<Value, (Code, String)> {
    let one = |x: &Value, cell: &str| -> Result<i64, (Code, String)> {
        let n = match x
            .as_i64()
            .or_else(|| x.as_f64().filter(|f| f.fract() == 0.0).map(|f| f as i64))
        {
            Some(n) => n,
            None => return Err((Code("type"), cell.to_string())),
        };
        let lo = d
            .min
            .map_or(*LINE_RANGE.start(), |m| m.ceil() as i64)
            .max(*LINE_RANGE.start());
        let hi = d
            .max
            .map_or(*LINE_RANGE.end(), |m| m.floor() as i64)
            .min(*LINE_RANGE.end());
        if (lo..=hi).contains(&n) {
            Ok(n)
        } else {
            Err((Code("range"), cell.to_string()))
        }
    };
    match v {
        Value::Null => Ok(Value::Null),
        Value::Number(_) => one(v, "").map(Value::from),
        Value::Object(m) => {
            if let Some(k) = m.keys().find(|k| *k != "5h" && *k != "7d") {
                return Err((Code("type"), format!(".{k}")));
            }
            let mut out = Map::new();
            for w in ["5h", "7d"] {
                match m.get(w) {
                    None | Some(Value::Null) => {}
                    Some(x) => {
                        out.insert(w.to_string(), Value::from(one(x, &format!(".{w}"))?));
                    }
                }
            }
            match (out.get("5h"), out.get("7d")) {
                (None, None) => Ok(Value::Null),
                (Some(a), Some(b)) if a == b => Ok(a.clone()),
                _ => Ok(Value::Object(out)),
            }
        }
        _ => Err((Code("type"), String::new())),
    }
}

fn slots_value(v: &Value) -> Result<Value, (Code, String)> {
    let arr = v.as_array().ok_or((Code("type"), String::new()))?;
    if arr.len() > MAX_SLOTS {
        return Err((Code("range"), String::new()));
    }
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for (i, s) in arr.iter().enumerate() {
        let m = s
            .as_object()
            .filter(|m| m.len() == 2)
            .ok_or((Code("type"), format!("[{i}]")))?;
        let at = m
            .get("at")
            .and_then(Value::as_str)
            .filter(|at| span_of(at).is_some())
            .ok_or((Code("time"), format!("[{i}].at")))?;
        let line = m
            .get("line")
            .and_then(|n| int_in(n, &SLOT_LINE_RANGE))
            .ok_or((Code("range"), format!("[{i}].line")))?;
        if seen.iter().any(|x| overlaps(x, at)) {
            return Err((Code("overlap"), format!("[{i}].at")));
        }
        seen.push(at.to_string());
        out.push(serde_json::json!({"at": at, "line": line}));
    }
    Ok(Value::Array(out))
}

/// `"HH:MM"` → 一天里第几分钟（`24:00` 收作止）。
pub(crate) fn clock_of(s: &str) -> Option<u16> {
    let (h, m) = s.split_once(':')?;
    if h.len() != 2 || m.len() != 2 {
        return None;
    }
    let (h, m): (u16, u16) = (h.parse().ok()?, m.parse().ok()?);
    if m >= 60 || h > 24 || (h == 24 && m != 0) {
        return None;
    }
    Some(h * 60 + m)
}

/// `"HH:MM-HH:MM"` → （起, 止）分钟；起止相同 · 读不懂 ⇒ `None`。
pub(crate) fn span_of(at: &str) -> Option<(u16, u16)> {
    let (a, b) = at.split_once('-')?;
    let (from, to) = (clock_of(a)?, clock_of(b)?);
    (from < 24 * 60 && from != to).then_some((from, to))
}

/// 一段占一天里的哪几截（跨午夜的拆成两截）。
fn pieces(at: &str) -> Vec<(u16, u16)> {
    match span_of(at) {
        Some((from, to)) if from < to => vec![(from, to)],
        Some((from, to)) => vec![(from, 24 * 60), (0, to)],
        None => Vec::new(),
    }
}

/// 两段有没有重叠的分钟（含跨午夜的）。
pub(crate) fn overlaps(a: &str, b: &str) -> bool {
    pieces(a)
        .iter()
        .any(|(f1, t1)| pieces(b).iter().any(|(f2, t2)| f1 < t2 && f2 < t1))
}

/// 一组声明照一份值逐格补齐、校验：缺的按 `init`（`fill` 先给的那一层 ⇒ 再按 `init`）；多出来的键丢掉。
/// 错的那几格 ⇒ （格名 `<prefix>.<k><小格>`, 短码）。
pub(crate) fn fill_checked(
    decls: &[Decl],
    given: &Map<String, Value>,
    fill: Option<&Map<String, Value>>,
    prefix: &str,
    errors: &mut Vec<(String, Code)>,
) -> Map<String, Value> {
    let mut out = Map::new();
    for d in decls {
        let v = given
            .get(&d.k)
            .or_else(|| fill.and_then(|f| f.get(&d.k)))
            .unwrap_or(&d.init);
        match check_cell(d, v) {
            Ok(v) => {
                out.insert(d.k.clone(), v);
            }
            Err((c, sub)) => {
                errors.push((format!("{prefix}.{}{sub}", d.k), c));
                out.insert(d.k.clone(), d.init.clone());
            }
        }
    }
    out
}

/// 界面画一格要的：声明 ＋ 照请求的语言写好的字。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "DeclShown"
    )
)]
pub struct Shown {
    pub k: String,
    #[serde(rename = "type")]
    pub ty: Type,
    #[cfg_attr(test, ts(type = "unknown"))]
    pub init: Value,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(test, ts(optional, as = "Option<Vec<String>>"))]
    pub of: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub max: Option<f64>,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub hint: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(test, ts(optional, as = "Option<Vec<ShownOption>>"))]
    pub options: Vec<ShownOption>,
}

/// `choice` 的一个选项：值 ＋ 字 ＋ 灰字。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "DeclOption"
    )
)]
pub struct ShownOption {
    pub v: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub hint: Option<String>,
}

/// 一项声明照语言写好字。
pub(crate) fn shown(d: &Decl, texts: &Texts, lang: Lang) -> Shown {
    let t = |k: &str| texts.get(lang, k).unwrap_or(k).to_string();
    Shown {
        k: d.k.clone(),
        ty: d.ty,
        init: d.init.clone(),
        of: d.of.clone(),
        min: d.min,
        max: d.max,
        label: t(&d.label),
        hint: d.hint.as_deref().map(t),
        options: d
            .of
            .iter()
            .map(|o| ShownOption {
                v: o.clone(),
                label: t(&format!("{}_{o}", d.label)),
                hint: texts
                    .get(lang, &format!("{}_{o}_h", d.label))
                    .map(str::to_string),
            })
            .collect(),
    }
}
