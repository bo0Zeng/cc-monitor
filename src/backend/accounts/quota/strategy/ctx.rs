//! 拼上下文 `ctx`（E4）：事实 ＋ 会话打开的那份配置（有副本用副本）的参数值 ＋ 上一判的备忘。只有事实与参数值，没有线、没有偏好。
//!
//! 形状（`轮换-v3.md` §2.3 原样）：`{now, clock, current, stint: {since, used}, rate, memo, params, accounts: [{id, kind, ok, why_not,
//! back, w5h: {open, used, resets}, w7d, others, params}]}`。参数缺的格按声明 `init` 补齐；`ordered` 的策略号按写下的先后，别的按号名。

use super::form::{self, Decl};
use super::Strategy;
use serde::Serialize;
use serde_json::{Map, Value};

/// 一个号事实上为什么不能用（引擎判的；线不在这里）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum WhyNot {
    Refused,
    Full,
    Overage,
    Login,
}

impl WhyNot {
    pub(crate) fn word(self) -> &'static str {
        match self {
            WhyNot::Refused => "refused",
            WhyNot::Full => "full",
            WhyNot::Overage => "overage",
            WhyNot::Login => "login",
        }
    }
}

/// 一个窗口：在计时吗 · 用了几点（0–100）· 几点重置（没开 ⇒ 用量 0、重置缺）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "WindowFact"
    )
)]
pub struct Win {
    pub open: bool,
    pub used: f64,
    #[cfg_attr(test, ts(type = "number | null"))]
    pub resets: Option<u64>,
}

impl Win {
    pub(crate) const CLOSED: Win = Win {
        open: false,
        used: 0.0,
        resets: None,
    };
}

/// 一个号此刻的事实（引擎从额度账 · 账号库 · 登录拼好交进来）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "AccountFacts"
    )
)]
pub struct Facts {
    pub id: String,
    pub kind: crate::accounts::quota::decide::Kind,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub why_not: Option<WhyNot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub back: Option<u64>,
    pub w5h: Win,
    pub w7d: Win,
    /// 此刻也钉在它上面的别的会话数。
    pub others: u32,
}

/// 换进在用号以来：起点 · 卡人那一窗用了几点。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub(crate) struct Stint {
    pub(crate) since: u64,
    pub(crate) used: f64,
}

#[derive(Serialize)]
struct AcctCtx<'a> {
    id: &'a str,
    kind: &'static str,
    ok: bool,
    why_not: Option<&'static str>,
    back: Option<u64>,
    w5h: &'a Win,
    w7d: &'a Win,
    others: u32,
    params: &'a Map<String, Value>,
}

#[derive(Serialize)]
struct CtxWire<'a> {
    now: u64,
    clock: u32,
    current: &'a str,
    stint: Stint,
    rate: Option<f64>,
    memo: &'a Value,
    params: &'a Map<String, Value>,
    accounts: Vec<AcctCtx<'a>>,
}

/// 拼一判的上下文要的全部：事实 ＋ 参数值（已按声明补齐）＋ 备忘。
pub(crate) struct Input<'a> {
    pub(crate) now: u64,
    /// 配置时区的本地钟：一天里第几分钟。
    pub(crate) clock: u32,
    pub(crate) current: &'a str,
    pub(crate) stint: Stint,
    pub(crate) rate: Option<f64>,
    pub(crate) memo: &'a Value,
    pub(crate) params: &'a Map<String, Value>,
    /// 参与的号（已按写下的先后）：事实 ＋ 这一号的参数值。
    pub(crate) accounts: &'a [(Facts, Map<String, Value>)],
}

/// ★ 拼 `ctx`（Rhai 的值）：`ordered` 的策略按写下的先后，别的按号名。
pub(crate) fn build(s: &Strategy, i: &Input<'_>) -> Result<rhai::Dynamic, String> {
    let mut accts: Vec<AcctCtx<'_>> = i
        .accounts
        .iter()
        .map(|(f, p)| AcctCtx {
            id: &f.id,
            kind: match f.kind {
                crate::accounts::quota::decide::Kind::Sub => "sub",
                crate::accounts::quota::decide::Kind::Api => "api",
            },
            ok: f.ok,
            why_not: f.why_not.map(WhyNot::word),
            back: f.back,
            w5h: &f.w5h,
            w7d: &f.w7d,
            others: f.others,
            params: p,
        })
        .collect();
    if !s.ordered {
        accts.sort_by(|a, b| a.id.cmp(b.id));
    }
    let w = CtxWire {
        now: i.now,
        clock: i.clock,
        current: i.current,
        stint: i.stint,
        rate: i.rate,
        memo: i.memo,
        params: i.params,
        accounts: accts,
    };
    rhai::serde::to_dynamic(&w).map_err(|e| e.to_string())
}

/// 一行的参数值照这份策略的每号声明补齐（缺的格先按 `acct_init`，再按声明 `init`；读不懂的格按 `init`）。
pub(crate) fn acct_params(
    decls: &[Decl],
    given: &Map<String, Value>,
    acct_init: Option<&Map<String, Value>>,
) -> Map<String, Value> {
    form::fill_checked(decls, given, acct_init, "", &mut Vec::new())
}

/// 整份参数值照声明补齐。
pub(crate) fn whole_params(decls: &[Decl], given: &Map<String, Value>) -> Map<String, Value> {
    form::fill_checked(decls, given, None, "", &mut Vec::new())
}
