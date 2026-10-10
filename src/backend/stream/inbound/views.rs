//! 请求信封里的 `view`（出口的声明）：登记处统一解、统一拒、统一投影。帧面与 CLI 面同一处。
//!
//! 每条命令的应答里**哪几处住着哪件成品**登记在 [`PLACES`]（与 [`super::caps`] 的总期限表同一种「按命令名一张表」）；
//! 声明点了这条命令不出的成品 ⇒ `bad_args`（不静默当没说）。没登记的命令带 `view` 一样拒。
//! 挑格去格的本体在 `faces/project.rs`（只那一处）。

use super::spec::Fail;
use crate::faces::project::{parse_view, project_reply, View};
use crate::stream::wire::Request;
use serde_json::Value;

/// 命令 → 应答里成品住的那几处 `(路径, 成品名)`（外层的成品排在它里面那件之前）。路径写法同格目录，空串 ＝ 整份应答。
pub(crate) const PLACES: &[(&str, &[(&str, &str)])] = &[
    (
        "history-read",
        &[("rows[]", "read_row"), ("rows[].record", "record")],
    ),
    ("history-page", &[("lines[].record", "record")]),
    ("history-lines", &[("lines[].record", "record")]),
    ("history-run", &[("rows[].record", "record")]),
    ("history-facts", &[("", "facts")]),
];

/// 命令 → 应答里**续算令牌**那一格的名字：这条命令的应答下一问要原样交回来接着算（`history-facts` 的 `prior`）。
/// 投影会去格，去过格的应答当令牌就缺了那几格，下一次增量算出来是错的 ⇒ 带声明时把**投影之前的整份**放进这一格交出去，
/// 这一格不受 `cells` / `omit` 管（出口当不透明的一团原样交回）。不带声明 ⇒ 没有这一格，整份应答本身就是令牌。
pub(crate) const TOKENS: &[(&str, &str)] = &[("history-facts", "prior")];

/// 这条命令的应答里成品住的那几处（没登记 ⇒ 空）。
pub(crate) fn places_of(cmd: &str) -> &'static [(&'static str, &'static str)] {
    PLACES
        .iter()
        .find(|(name, _)| *name == cmd)
        .map_or(&[], |(_, p)| p)
}

/// 一条请求的投影：校验过的声明 ＋ 这条命令的那几处。没带声明 ⇒ 原样。
#[derive(Debug, Clone, Default)]
pub(crate) struct Plan {
    view: Option<(View, &'static [(&'static str, &'static str)])>,
    /// 这条命令的续算令牌那一格（[`TOKENS`]）。
    token: Option<&'static str>,
}

impl Plan {
    /// 成功的应答照声明裁好；有续算令牌那一格的命令，先把整份放进那一格（不受投影管）。
    pub(crate) fn apply(&self, data: Option<Value>) -> Option<Value> {
        match (&self.view, data) {
            (Some((view, places)), Some(mut v)) => {
                let whole = self.token.map(|_| v.clone());
                project_reply(&mut v, places, view);
                if let (Some(key), Some(whole), Some(o)) = (self.token, whole, v.as_object_mut()) {
                    o.insert(key.to_string(), whole);
                }
                Some(v)
            }
            (_, d) => d,
        }
    }
}

/// 解这条请求的 `view`：认不出的词 · 成品 · 格，或点了这条命令不出的成品 ⇒ `bad_args`（那几处进复制详情）。
pub(crate) fn plan_of(req: &Request) -> Result<Plan, Fail> {
    plan_for(&req.cmd, &req.view)
}

/// [`plan_of`] 的本体（命令名与声明分开给：CLI 面在建请求之前就要拒）。
pub(crate) fn plan_for(cmd: &str, view: &Value) -> Result<Plan, Fail> {
    let refuse = |detail: String| {
        Fail::new(
            "bad_args",
            crate::common::contract::malformed(&format!("{cmd}: {detail}")),
        )
        .with_raw(Some(&detail))
    };
    let Some(view) = parse_view(view).map_err(refuse)? else {
        return Ok(Plan::default());
    };
    let places = places_of(cmd);
    let stray: Vec<&str> = view
        .products()
        .into_iter()
        .filter(|p| !places.iter().any(|(_, q)| q == p))
        .collect();
    if !stray.is_empty() {
        return Err(refuse(format!(
            "view names {} but `{cmd}` carries {}",
            stray.join(", "),
            if places.is_empty() {
                "no product".to_string()
            } else {
                places
                    .iter()
                    .map(|(_, q)| *q)
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        )));
    }
    Ok(Plan {
        view: Some((view, places)),
        token: TOKENS.iter().find(|(c, _)| *c == cmd).map(|(_, k)| *k),
    })
}

#[cfg(test)]
#[path = "../../../../tests/backend/stream/views_tests.rs"]
mod tests;
