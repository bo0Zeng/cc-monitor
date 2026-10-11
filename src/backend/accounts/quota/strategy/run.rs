//! 跑策略（E5）：沙箱里调 `priority(ctx)`，校验回的形状，取出 `@memo`。出错 / 超时 / 形状不对 ⇒ 这一判照内置策略 1 排
//! （参数取它的 `init`，每行的 `line` · `slots` 取这份配置写的，号的先后取写下的先后），并带上为什么。

use super::words::lib_key_known;
use super::{call, Fail, Strategy};
use serde_json::Value;
use std::collections::BTreeMap;

/// 策略给一个号的：`p`（`None` ＝ `()`，此刻不挑）· `why`（键或 `{k, …}`）· `back`（`p` 是 `()` 时几点能再挑）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Given {
    pub(crate) p: Option<f64>,
    pub(crate) why: Value,
    pub(crate) back: Option<u64>,
}

/// 一判的回值：号 → 那一行 · 备忘（`@memo`，没写 ⇒ `None`）· 用了几次操作。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Priorities {
    pub(crate) rows: BTreeMap<String, Given>,
    pub(crate) memo: Option<Value>,
    pub(crate) ops: u64,
}

/// ★ 调一次 `priority(ctx)`：`accounts` ＝ 参与的号 id（回值里别的键不认）。形状不对 ⇒ `BadShape`。
pub(crate) fn priority(
    s: &Strategy,
    ctx: rhai::Dynamic,
    accounts: &[String],
) -> Result<Priorities, Fail> {
    let Some(ast) = s.ast.as_deref().filter(|_| s.error.is_none()) else {
        return Err(s.error.clone().unwrap_or_else(|| Fail {
            kind: super::FailKind::Gone,
            line: None,
            col: None,
            msg: "strategy is not loaded".into(),
        }));
    };
    let (r, ops) = call(ast, "priority", (ctx,));
    let d = r.map_err(|e| Fail::of_eval(&e))?;
    let v = rhai::serde::from_dynamic::<Value>(&d).map_err(|e| Fail::shape(e.to_string()))?;
    let mut out = shape(s, &v, accounts)?;
    out.ops = ops;
    Ok(out)
}

/// 校验回值的形状：`p` 是数或 `()` · `why` 是键（在 `texts` 里，或是引擎认得的 `@` 键）或 `{k, …}` · `back` 是时刻。
pub(crate) fn shape(s: &Strategy, v: &Value, accounts: &[String]) -> Result<Priorities, Fail> {
    let m = v
        .as_object()
        .ok_or_else(|| Fail::shape("priority() must return a map"))?;
    let memo = match m.get("@memo") {
        None | Some(Value::Null) => None,
        Some(x @ Value::Object(_)) => Some(x.clone()),
        Some(_) => return Err(Fail::shape("`@memo` must be a map")),
    };
    let mut rows = BTreeMap::new();
    for a in accounts {
        let Some(x) = m.get(a) else { continue };
        let o = x
            .as_object()
            .ok_or_else(|| Fail::shape(format!("`{a}` must be #{{p, why, back?}}")))?;
        let p = match o.get("p") {
            None | Some(Value::Null) => None,
            Some(Value::Number(n)) => Some(
                n.as_f64()
                    .filter(|f| f.is_finite())
                    .ok_or_else(|| Fail::shape(format!("`{a}`.p must be a number or ()")))?,
            ),
            Some(_) => return Err(Fail::shape(format!("`{a}`.p must be a number or ()"))),
        };
        let why = o.get("why").cloned().unwrap_or(Value::Null);
        why_ok(s, &why).map_err(|m| Fail::shape(format!("`{a}`.why: {m}")))?;
        let back =
            match o.get("back") {
                None | Some(Value::Null) => None,
                Some(Value::Number(n)) => Some(n.as_u64().ok_or_else(|| {
                    Fail::shape(format!("`{a}`.back must be a time (unix seconds)"))
                })?),
                Some(_) => {
                    return Err(Fail::shape(format!(
                        "`{a}`.back must be a time (unix seconds)"
                    )))
                }
            };
        rows.insert(a.clone(), Given { p, why, back });
    }
    Ok(Priorities { rows, memo, ops: 0 })
}

/// 一句 `why` 能不能写成字：键在这份策略的 `texts` 里（两种语言都有），或是引擎认得的 `@` 键；`then` 递归查。`()` 也收（写成空）。
pub(crate) fn why_ok(s: &Strategy, why: &Value) -> Result<(), String> {
    let k = match why {
        Value::Null => return Ok(()),
        Value::String(k) => k.as_str(),
        Value::Object(m) => {
            if let Some(t) = m.get("then") {
                why_ok(s, t)?;
            }
            m.get("k")
                .and_then(Value::as_str)
                .ok_or("a map why needs `k`")?
        }
        _ => return Err("must be a key or #{k, …}".into()),
    };
    if k.starts_with('@') {
        if lib_key_known(k) {
            Ok(())
        } else {
            Err(format!("`{k}` is not an engine key"))
        }
    } else if s.texts.has(k) {
        Ok(())
    } else {
        Err(format!("`{k}` is not in texts() (zh and en)"))
    }
}
