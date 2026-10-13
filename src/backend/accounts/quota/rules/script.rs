//! 策略文件（`轮换-v5-分层.md` §2.1）：标题 ＝ 文件名；文件最开头连着的 `//` 行是说明；接着一段块注释是 JSON 头（格式 · `timeVarying` · 时区 · 设置 · 声明），
//! 后面是 Rhai 代码。文件仍是合法的 Rhai；存设置只重写头那一块，说明和代码一个字不动。代码里一个函数 `decide(ctx)`，跑在沙箱里（同第三版的上限），挂着只读的积木库 `lib`（`src/shared/rotation/lib.rhai`）。

use super::decide::{Ctx, Strategy, Verdict, WakeWhen};
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
    Langs {
        zh: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        en: Option<String>,
    },
}

/// JSON 头。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Head {
    pub(crate) format: u32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) time_varying: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tz: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) tz_unknown: bool,
    /// 这份策略自己的设置（形状它自己定，后端原样交给它）。
    pub(crate) settings: serde_json::Value,
    /// 设置怎么画、怎么校验（`轮换-v5-分层.md` §6.4.1）：界面照它画，后端照它校验形状，不认意思。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) form: Vec<super::form::Cell>,
}

/// 这一版的格式号。
pub(crate) const FORMAT: u32 = 5;

/// 一份读懂了的策略文件：说明 ＋ 头 ＋ 编好的代码。
pub(crate) struct File {
    /// 文件开头那段 `//` 注释：去掉行首的 `// `，一行一行用 `\n` 接起来，空行是分段。界面只显示它。
    pub(crate) about: String,
    pub(crate) head: Head,
    ast: AST,
}

impl std::fmt::Debug for File {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("File")
            .field("about", &self.about)
            .field("head", &self.head)
            .finish_non_exhaustive()
    }
}

fn fail(kind: FailKind, line: Option<usize>, msg: impl Into<String>) -> Fail {
    Fail {
        why: if kind == FailKind::Timeout {
            crate::accounts::quota::strategy::FailWhy::Timeout
        } else {
            crate::accounts::quota::strategy::FailWhy::Other
        },
        kind,
        line,
        col: None,
        msg: msg.into(),
    }
}

/// 拆成 说明那一段原文（含它后面的空行）· 头 JSON 原文 · 代码。说明只能是连着的 `//` 行，跟头之间只许空行。
fn split(source: &str) -> Result<(&str, &str, &str), Fail> {
    let mut at = 0;
    let mut line_no = 1;
    for l in source.split_inclusive('\n') {
        let s = l.trim_end_matches(['\n', '\r']);
        if s.starts_with("//") || s.trim().is_empty() {
            at += l.len();
            line_no += 1;
            continue;
        }
        break;
    }
    let rest = source[at..].strip_prefix(HEAD_OPEN).ok_or_else(|| {
        fail(
            FailKind::BadShape,
            Some(line_no),
            "after the leading `//` lines the file must go on with the `/*--- rotation` head",
        )
    })?;
    let end = rest.find(HEAD_CLOSE).ok_or_else(|| {
        fail(
            FailKind::BadShape,
            Some(line_no),
            "the head is not closed with `---*/`",
        )
    })?;
    Ok((&source[..at], &rest[..end], &rest[end + HEAD_CLOSE.len()..]))
}

/// 说明那一段原文 ⇒ 显示用的字：去行首 `// `（或 `//`），首尾的空行去掉，连着几个空行算一个。
fn about_of(lead: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for l in lead.lines() {
        let Some(c) = l.trim_end().strip_prefix("//") else {
            continue;
        };
        let c = c.strip_prefix(' ').unwrap_or(c);
        if c.trim().is_empty() && out.last().is_none_or(|x| x.is_empty()) {
            continue;
        }
        out.push(if c.trim().is_empty() { "" } else { c });
    }
    while out.last().is_some_and(|x| x.is_empty()) {
        out.pop();
    }
    out.join("\n")
}

/// 这一份是不是第五版的策略文件（说明之后是那一段头）：不是的（第三版脚本）升级时挪开。
pub(crate) fn has_head(source: &str) -> bool {
    split(source).is_ok()
}

/// ★ 读一份策略文件：头照形状解、代码编一次、要有 `decide`。
pub(crate) fn parse(source: &str) -> Result<File, Fail> {
    let (lead, head, code) = split(source)?;
    let about = about_of(lead);
    let head: Head = serde_json::from_str(head).map_err(|e| {
        let base = lead.matches('\n').count() + 1;
        fail(FailKind::BadShape, Some(base + e.line()), e.to_string())
    })?;
    if head.format != FORMAT {
        return Err(fail(
            FailKind::BadShape,
            Some(2),
            format!("format {} is not {FORMAT}", head.format),
        ));
    }
    super::form::check_form(&head.form).map_err(|m| fail(FailKind::BadShape, Some(2), m))?;
    if let Some(b) = super::form::check(&head.form, &head.settings).first() {
        if !head.form.is_empty() {
            return Err(fail(
                FailKind::BadShape,
                Some(2),
                format!("settings.{} does not fit the form ({})", b.cell, b.code),
            ));
        }
    }
    let e = engine().map_err(|m| fail(FailKind::Broke, None, m))?;
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
        .any(|f| f.name == "decide" && f.params.len() == 1)
    {
        return Err(fail(
            FailKind::BadShape,
            None,
            "the code has no `fn decide(ctx)`",
        ));
    }
    Ok(File { about, head, ast })
}

/// ★ 换掉头那一块；说明和代码原样接回去，一个字节不动（存设置那一下）。
pub(crate) fn with_head(source: &str, head: &Head) -> Result<String, Fail> {
    let (lead, _, code) = split(source)?;
    let json = serde_json::to_string_pretty(head)
        .map_err(|e| fail(FailKind::BadShape, None, e.to_string()))?;
    Ok(format!("{lead}{HEAD_OPEN}{json}{HEAD_CLOSE}{code}"))
}

/// 积木库的源码（随包、只读）。
pub(crate) const LIB_SOURCE: &str = include_str!("../../../../shared/rotation/lib.rhai");

/// 挂好积木库 `lib` 的那一个沙箱（进程里一份）。
pub(crate) fn engine() -> Result<&'static rhai::Engine, String> {
    static E: std::sync::OnceLock<Result<rhai::Engine, String>> = std::sync::OnceLock::new();
    E.get_or_init(|| with_lib(crate::accounts::quota::strategy::fresh_engine()))
        .as_ref()
        .map_err(Clone::clone)
}

/// 在沙箱 `e` 上挂积木库 `lib`（[`engine`] 那一份就是 [`fresh_engine`](crate::accounts::quota::strategy::fresh_engine) 挂上它）。
pub(crate) fn with_lib(mut e: rhai::Engine) -> Result<rhai::Engine, String> {
    let ast = e.compile(LIB_SOURCE).map_err(|x| x.to_string())?;
    let m =
        rhai::Module::eval_ast_as_new(rhai::Scope::new(), &ast, &e).map_err(|x| x.to_string())?;
    e.register_static_module("lib", m.into());
    Ok(e)
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
            m.insert("sendable".into(), a.sendable.into());
            m.insert("used".into(), pair(a.used[0].into(), a.used[1].into()));
            m.insert("resets".into(), pair(opt(a.resets[0]), opt(a.resets[1])));
            m.insert("refusedUntil".into(), opt(a.refused_until));
            m.insert("rate".into(), pair(optf(a.rate[0]), optf(a.rate[1])));
            Dynamic::from(m)
        })
        .collect();
    let json = |v: &serde_json::Value| rhai::serde::to_dynamic(v).unwrap_or(Dynamic::UNIT);
    let mut m = Map::new();
    m.insert(
        "now".into(),
        Dynamic::from(i64::try_from(c.now).unwrap_or(i64::MAX)),
    );
    m.insert("clock".into(), Dynamic::from(i64::from(c.clock)));
    m.insert("start".into(), c.start.to_string().into());
    m.insert("current".into(), c.current.to_string().into());
    m.insert("settings".into(), json(c.settings));
    m.insert("memo".into(), c.memo.map_or(Dynamic::UNIT, json));
    m.insert("accounts".into(), Dynamic::from(accounts));
    Dynamic::from(m)
}

fn num(d: &Dynamic) -> Option<f64> {
    d.as_float()
        .ok()
        .or_else(|| d.as_int().ok().map(|i| i as f64))
}

fn time(d: Option<&Dynamic>, what: &str) -> Result<Option<u64>, Fail> {
    match d {
        None => Ok(None),
        Some(x) if x.is_unit() => Ok(None),
        Some(x) => x
            .as_int()
            .ok()
            .and_then(|n| u64::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| {
                fail(
                    FailKind::BadShape,
                    None,
                    format!("{what} must be an integer time"),
                )
            }),
    }
}

/// 回值照形状收：`#{order: [号], why?: #{号: 一句}, hold_until?, wake?: #{at?, when?: [#{acct, w, used_at_least}]}, memo?}`。
fn shape(v: Dynamic) -> Result<Verdict, Fail> {
    let bad = |m: &str| fail(FailKind::BadShape, None, m.to_string());
    let top = v
        .try_cast::<Map>()
        .ok_or_else(|| bad("decide(ctx) must return a map"))?;
    let mut out = Verdict::default();
    let order = top.get("order").ok_or_else(|| bad("order is missing"))?;
    let order = order
        .clone()
        .try_cast::<rhai::Array>()
        .ok_or_else(|| bad("order must be an array"))?;
    for a in order {
        out.order.push(
            a.into_string()
                .map_err(|_| bad("order must list account ids"))?,
        );
    }
    if let Some(w) = top.get("why").filter(|w| !w.is_unit()) {
        let w = w
            .clone()
            .try_cast::<Map>()
            .ok_or_else(|| bad("why must be a map"))?;
        for (k, v) in w {
            let s = v.into_string().map_err(|_| bad("why must be a string"))?;
            if s.chars().count() > 80 {
                return Err(bad("why is longer than 80 characters"));
            }
            out.why.push((k.to_string(), s));
        }
    }
    out.hold_until = time(top.get("hold_until"), "hold_until")?;
    if let Some(w) = top.get("wake").filter(|w| !w.is_unit()) {
        let w = w
            .clone()
            .try_cast::<Map>()
            .ok_or_else(|| bad("wake must be a map"))?;
        out.wake_at = time(w.get("at"), "wake.at")?;
        if let Some(list) = w.get("when").filter(|x| !x.is_unit()) {
            let list = list
                .clone()
                .try_cast::<rhai::Array>()
                .ok_or_else(|| bad("wake.when must be an array"))?;
            for x in list {
                let m = x
                    .try_cast::<Map>()
                    .ok_or_else(|| bad("each wake.when must be a map"))?;
                let acct = m
                    .get("acct")
                    .and_then(|a| a.clone().into_string().ok())
                    .ok_or_else(|| bad("wake.when.acct"))?;
                let w = match m
                    .get("w")
                    .and_then(|a| a.clone().into_string().ok())
                    .as_deref()
                {
                    Some("5h") => 0,
                    Some("7d") => 1,
                    _ => return Err(bad("wake.when.w must be \"5h\" or \"7d\"")),
                };
                let n = m
                    .get("used_at_least")
                    .and_then(num)
                    .ok_or_else(|| bad("wake.when.used_at_least"))?;
                out.wake_when.push(WakeWhen {
                    acct,
                    w,
                    used_at_least: n,
                });
            }
        }
    }
    if let Some(m) = top.get("memo").filter(|m| !m.is_unit()) {
        out.memo = Some(rhai::serde::from_dynamic(m).map_err(|e| bad(&e.to_string()))?);
    }
    Ok(out)
}

impl File {
    /// 在给定的沙箱里判（[`Strategy::decide`] 给的是进程里那一份 [`engine`]）。
    pub(crate) fn decide_in(&self, e: &rhai::Engine, ctx: &Ctx<'_>) -> Result<Verdict, Fail> {
        let (r, _) =
            crate::accounts::quota::strategy::call_in(e, &self.ast, "decide", (ctx_dynamic(ctx),));
        shape(r.map_err(|x| Fail::of_eval(&x))?)
    }
}

impl Strategy for File {
    fn decide(&self, ctx: &Ctx<'_>) -> Result<Verdict, Fail> {
        let e = engine().map_err(|m| fail(FailKind::Broke, None, m))?;
        self.decide_in(e, ctx)
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
    inner: &'a dyn Strategy,
    spent: std::cell::Cell<std::time::Duration>,
}

impl<'a> Budgeted<'a> {
    pub(crate) fn new(inner: &'a dyn Strategy) -> Self {
        Self {
            inner,
            spent: std::cell::Cell::new(std::time::Duration::ZERO),
        }
    }
}

impl Strategy for Budgeted<'_> {
    fn decide(&self, ctx: &Ctx<'_>) -> Result<Verdict, Fail> {
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
        let r = self.inner.decide(ctx);
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
