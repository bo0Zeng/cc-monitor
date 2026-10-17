//! 策略的代码：编一次 · 在沙箱里调 `decide(ctx)`（挂着只读的积木库 `lib`，`src/shared/rotation/lib.rhai`）· 回值照形状收 · 一趟推演的总时限。
//! 文件的格式（说明 · 头 · 只重写头）归配置编辑层；这里只认代码。

use super::decide::{Ctx, Strategy, Verdict, WakeWhen};
use crate::accounts::quota::strategy::{Fail, FailKind};
use rhai::{Dynamic, Map, AST};

/// 编好的代码（要有 `fn decide(ctx)`）。
pub(crate) struct Code {
    ast: AST,
}

pub(crate) fn fail(kind: FailKind, line: Option<usize>, msg: impl Into<String>) -> Fail {
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

/// ★ 编一份代码：`code` 前面已垫好空行（报第几行时与文件对得上）。
pub(crate) fn compile(code: &str) -> Result<Code, Fail> {
    let e = engine().map_err(|m| fail(FailKind::Broke, None, m))?;
    let ast = e.compile(code).map_err(|x| {
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
    Ok(Code { ast })
}

/// 进程里至多缓存几份编好的代码（超了整份清掉重来）。
const CACHED: usize = 32;

/// ★ 副本的代码按版本编一次、之后照缓存（同一个版本 ⇒ 同一份文件 ⇒ 同一段代码）。
pub(crate) fn cached(rev: &str, code: &str) -> Result<std::sync::Arc<Code>, Fail> {
    type Map = std::collections::HashMap<String, Result<std::sync::Arc<Code>, Fail>>;
    static MAP: std::sync::Mutex<Option<Map>> = std::sync::Mutex::new(None);
    let mut g = MAP.lock().unwrap_or_else(|e| e.into_inner());
    let m = g.get_or_insert_with(Map::new);
    if let Some(r) = m.get(rev) {
        return r.clone();
    }
    if m.len() >= CACHED {
        m.clear();
    }
    let r = compile(code).map(std::sync::Arc::new);
    m.insert(rev.to_string(), r.clone());
    r
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
            m.insert(
                "tokensPerPct".into(),
                pair(opt(a.per_pct[0]), opt(a.per_pct[1])),
            );
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

impl Code {
    /// 在给定的沙箱里判。
    pub(crate) fn decide_in(&self, e: &rhai::Engine, ctx: &Ctx<'_>) -> Result<Verdict, Fail> {
        let (r, _) =
            crate::accounts::quota::strategy::call_in(e, &self.ast, "decide", (ctx_dynamic(ctx),));
        shape(r.map_err(|x| Fail::of_eval(&x))?)
    }

    /// 在进程里那一份 [`engine`] 上判。
    pub(crate) fn decide(&self, ctx: &Ctx<'_>) -> Result<Verdict, Fail> {
        let e = engine().map_err(|m| fail(FailKind::Broke, None, m))?;
        self.decide_in(e, ctx)
    }
}

/// 一趟推演里给策略的总耗时：一趟至多调 200 次，单次的墙钟兜底是秒级，不另限就可能拖到几十秒。用完 ⇒ 推演停在那里。
/// 只是累计耗时的一把尺子（每调一次策略前后读一次钟、加起来比），不等、不睡、不让谁醒来。登记在零定时器的期限表里。
#[allow(
    clippy::disallowed_methods,
    reason = "一趟推演里给策略的总耗时（`rules::code::SIMULATE_BUDGET`）：每调一次策略前后读一次钟、把耗时加起来跟它比，到了就不再调、推演停在那里"
)]
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
