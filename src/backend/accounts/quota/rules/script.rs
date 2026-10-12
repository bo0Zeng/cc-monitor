//! 策略文件（`轮换-v5-分层.md` §2.1）：开头一段块注释是 JSON 头（名字 · 一行灰字 · `timeVarying` · 时区 · 设置），后面是 Rhai 代码。
//! 文件仍是合法的 Rhai；存设置只重写头那一段，代码一个字不动。代码里一个函数 `score(ctx)`，跑在沙箱里（同第三版的上限）。

use super::decide::{Ctx, Score, Scored};
use super::settings::Settings;
use crate::accounts::quota::strategy::{Fail, FailKind};
use rhai::{Dynamic, Map, AST};
use serde::{Deserialize, Serialize};

/// 头那一段的开头与结尾。
const HEAD_OPEN: &str = "/*--- rotation\n";
const HEAD_CLOSE: &str = "\n---*/";

/// 头里能写成一句、也能分语言写的字。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "StrategyText"
    )
)]
pub enum Text {
    One(String),
    Langs { zh: String, en: String },
}

impl Text {
    /// 照语言挑（`en` 才挑英文）。
    pub(crate) fn pick(&self, en: bool) -> &str {
        match self {
            Text::One(s) => s,
            Text::Langs { zh, en: e } => {
                if en {
                    e
                } else {
                    zh
                }
            }
        }
    }
}

/// JSON 头。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Head {
    pub(crate) format: u32,
    pub(crate) name: Text,
    pub(crate) line: Text,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) time_varying: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tz: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) tz_unknown: bool,
    pub(crate) settings: Settings,
}

/// 这一版的格式号。
pub(crate) const FORMAT: u32 = 5;

/// 一份读懂了的策略文件：头 ＋ 编好的代码。
pub(crate) struct File {
    pub(crate) head: Head,
    ast: AST,
}

impl std::fmt::Debug for File {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("File")
            .field("head", &self.head)
            .finish_non_exhaustive()
    }
}

fn fail(kind: FailKind, line: Option<usize>, msg: impl Into<String>) -> Fail {
    Fail {
        kind,
        line,
        col: None,
        msg: msg.into(),
    }
}

/// 拆成 头 JSON 原文 · 代码。
fn split(source: &str) -> Result<(&str, &str), Fail> {
    let rest = source.strip_prefix(HEAD_OPEN).ok_or_else(|| {
        fail(
            FailKind::BadShape,
            Some(1),
            "the file must start with the `/*--- rotation` head",
        )
    })?;
    let end = rest.find(HEAD_CLOSE).ok_or_else(|| {
        fail(
            FailKind::BadShape,
            Some(1),
            "the head is not closed with `---*/`",
        )
    })?;
    Ok((&rest[..end], &rest[end + HEAD_CLOSE.len()..]))
}

/// ★ 读一份策略文件：头照形状解、代码编一次、要有 `score`。
pub(crate) fn parse(source: &str) -> Result<File, Fail> {
    let (head, code) = split(source)?;
    let head: Head = serde_json::from_str(head)
        .map_err(|e| fail(FailKind::BadShape, Some(e.line() + 1), e.to_string()))?;
    if head.format != FORMAT {
        return Err(fail(
            FailKind::BadShape,
            Some(2),
            format!("format {} is not {FORMAT}", head.format),
        ));
    }
    let e = crate::accounts::quota::strategy::bare_engine();
    // 代码的行号接在头后面（报第几行时与文件对得上）。
    let lead = source.len() - code.len();
    let padded = format!(
        "{}{code}",
        "\n".repeat(source[..lead].matches('\n').count())
    );
    let ast = e.compile(&padded).map_err(|x| {
        let p = x.position();
        fail(FailKind::Broke, p.line(), x.to_string())
    })?;
    if !ast
        .iter_functions()
        .any(|f| f.name == "score" && f.params.len() == 1)
    {
        return Err(fail(
            FailKind::BadShape,
            None,
            "the code has no `fn score(ctx)`",
        ));
    }
    Ok(File { head, ast })
}

/// ★ 换掉头那一段、代码原样接回去（存设置那一下）。
pub(crate) fn with_head(source: &str, head: &Head) -> Result<String, Fail> {
    let (_, code) = split(source)?;
    let json = serde_json::to_string_pretty(head)
        .map_err(|e| fail(FailKind::BadShape, None, e.to_string()))?;
    Ok(format!("{HEAD_OPEN}{json}{HEAD_CLOSE}{code}"))
}

fn ctx_dynamic(c: &Ctx<'_>) -> Dynamic {
    let opt = |v: Option<u64>| {
        v.map_or(Dynamic::UNIT, |x| {
            Dynamic::from(i64::try_from(x).unwrap_or(i64::MAX))
        })
    };
    let optf = |v: Option<f64>| v.map_or(Dynamic::UNIT, Dynamic::from);
    let pair = |a: Dynamic, b: Dynamic| {
        let mut m = Map::new();
        m.insert("w5h".into(), a);
        m.insert("w7d".into(), b);
        Dynamic::from(m)
    };
    let accounts: rhai::Array = c
        .accounts
        .iter()
        .map(|a| {
            let mut m = Map::new();
            m.insert("id".into(), a.id.clone().into());
            m.insert("pos".into(), Dynamic::from(a.pos as i64));
            m.insert("kind".into(), (if a.api { "api" } else { "sub" }).into());
            m.insert("ok".into(), a.ok.into());
            m.insert(
                "H".into(),
                Dynamic::from(if a.h.is_finite() { a.h } else { 100.0 }),
            );
            m.insert("used".into(), pair(a.used[0].into(), a.used[1].into()));
            m.insert("resets".into(), pair(opt(a.resets[0]), opt(a.resets[1])));
            m.insert("cap".into(), pair(a.cap[0].into(), a.cap[1].into()));
            m.insert("refusedUntil".into(), opt(a.refused_until));
            m.insert("rate".into(), pair(optf(a.rate[0]), optf(a.rate[1])));
            Dynamic::from(m)
        })
        .collect();
    let mut m = Map::new();
    m.insert(
        "now".into(),
        Dynamic::from(i64::try_from(c.now).unwrap_or(i64::MAX)),
    );
    m.insert("clock".into(), Dynamic::from(i64::from(c.clock)));
    m.insert("current".into(), c.current.to_string().into());
    m.insert(
        "temp".into(),
        Dynamic::from(
            c.temp
                .iter()
                .map(|s| Dynamic::from(s.clone()))
                .collect::<rhai::Array>(),
        ),
    );
    m.insert("accounts".into(), Dynamic::from(accounts));
    Dynamic::from(m)
}

fn num(d: &Dynamic) -> Option<f64> {
    d.as_float()
        .ok()
        .or_else(|| d.as_int().ok().map(|i| i as f64))
}

/// 回值照形状收：`#{scores: #{号: #{score?, why}}, next_change_at?}`。`why` 是一句（≤ 80 字）。
fn shape(v: Dynamic) -> Result<Scored, Fail> {
    let bad = |m: &str| fail(FailKind::BadShape, None, m.to_string());
    let top = v
        .try_cast::<Map>()
        .ok_or_else(|| bad("score(ctx) must return a map"))?;
    let mut out = Scored::default();
    if let Some(n) = top.get("next_change_at") {
        if !n.is_unit() {
            let n = n
                .as_int()
                .map_err(|_| bad("next_change_at must be an integer time"))?;
            out.next_change_at = u64::try_from(n).ok();
        }
    }
    let scores = match top.get("scores") {
        None => return Ok(out),
        Some(s) => s
            .clone()
            .try_cast::<Map>()
            .ok_or_else(|| bad("scores must be a map"))?,
    };
    for (k, v) in scores {
        let row = v
            .try_cast::<Map>()
            .ok_or_else(|| bad("each score must be a map"))?;
        let score = match row.get("score") {
            None => None,
            Some(s) if s.is_unit() => None,
            Some(s) => Some(
                num(s)
                    .filter(|x| x.is_finite())
                    .ok_or_else(|| bad("score must be a finite number"))?,
            ),
        };
        let why = row
            .get("why")
            .map(|w| {
                w.clone()
                    .into_string()
                    .map_err(|_| bad("why must be a string"))
            })
            .transpose()?
            .unwrap_or_default();
        if why.chars().count() > 80 {
            return Err(bad("why is longer than 80 characters"));
        }
        out.scores.insert(k.to_string(), (score, why));
    }
    Ok(out)
}

impl Score for File {
    fn score(&self, ctx: &Ctx<'_>) -> Result<Scored, Fail> {
        let e = crate::accounts::quota::strategy::bare_engine();
        let (r, _) =
            crate::accounts::quota::strategy::call_in(e, &self.ast, "score", (ctx_dynamic(ctx),));
        shape(r.map_err(|x| Fail::of_eval(&x))?)
    }

    fn time_varying(&self) -> bool {
        self.head.time_varying
    }
}

/// 一趟推演里给策略的总耗时：一趟至多调 200 次，单次的墙钟兜底是秒级，不另限就可能拖到几十秒。用完 ⇒ 推演停在那里。
/// 只是累计耗时的一把尺子（每调一次策略前后读一次钟、加起来比），不等、不睡、不让谁醒来。登记在零定时器的期限表里。
pub(crate) const SIMULATE_BUDGET: std::time::Duration = std::time::Duration::from_millis(500);

/// 给一趟推演用的策略：累计耗时到了 [`SIMULATE_BUDGET`] 就不再调、报超时。
pub(crate) struct Budgeted<'a> {
    inner: &'a dyn Score,
    spent: std::cell::Cell<std::time::Duration>,
}

impl<'a> Budgeted<'a> {
    pub(crate) fn new(inner: &'a dyn Score) -> Self {
        Self {
            inner,
            spent: std::cell::Cell::new(std::time::Duration::ZERO),
        }
    }
}

impl Score for Budgeted<'_> {
    fn score(&self, ctx: &Ctx<'_>) -> Result<Scored, Fail> {
        if self.exhausted() {
            return Err(fail(
                FailKind::Timeout,
                None,
                format!(
                    "strategy ran past {} ms in this simulation",
                    SIMULATE_BUDGET.as_millis()
                ),
            ));
        }
        let t0 = crate::common::time::now();
        let r = self.inner.score(ctx);
        let took = crate::common::time::now()
            .duration_since(t0)
            .unwrap_or_default();
        self.spent.set(self.spent.get() + took);
        r
    }

    fn time_varying(&self) -> bool {
        self.inner.time_varying()
    }

    fn exhausted(&self) -> bool {
        self.spent.get() >= SIMULATE_BUDGET
    }
}

#[cfg(test)]
#[path = "../../../../../tests/backend/accounts/quota/rules/script_tests.rs"]
mod tests;
