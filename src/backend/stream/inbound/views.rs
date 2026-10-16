//! 请求信封里的 `view`（出口的声明）：登记处统一解、统一拒、统一投影。帧面与 CLI 面同一处。
//!
//! 每条命令的应答里**哪几处住着哪件成品**登记在 [`PLACES`]（与 [`super::caps`] 的总期限表同一种「按命令名一张表」）；
//! 声明点了这条命令不出的成品 ⇒ `bad_args`（不静默当没说）。没登记的命令带 `view` 一样拒。
//! 挑格去格的本体在 `faces/project.rs`（只那一处）。

use super::spec::Fail;
use crate::faces::project::{parse_view, project_reply, View};
use crate::wire::Request;
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

/// 命令 → 应答里**续算令牌**那几格：下一问把它们交回来接着算（`history-facts` 的 `end` · `rev` ⇒ 入参 `prior`）。
/// 这几格**投影不去**（`cells` 没点 · `omit` 点了都照样在）：出口去了它们就续不上了。
pub(crate) const TOKENS: &[(&str, &[&str])] = &[("history-facts", &["end", "rev"])];

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
    /// 这条命令的续算令牌那几格（[`TOKENS`]）。
    token: &'static [&'static str],
}

impl Plan {
    /// 成功的应答照声明裁好；续算令牌那几格裁完放回去（不受投影管）。
    pub(crate) fn apply(&self, data: Option<Value>) -> Option<Value> {
        match (&self.view, data) {
            (Some((view, places)), Some(mut v)) => {
                let kept: Vec<(&str, Value)> = self
                    .token
                    .iter()
                    .filter_map(|k| v.get(*k).map(|x| (*k, x.clone())))
                    .collect();
                project_reply(&mut v, places, view);
                if let Some(o) = v.as_object_mut() {
                    for (k, x) in kept {
                        o.insert(k.to_string(), x);
                    }
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
        token: TOKENS
            .iter()
            .find(|(c, _)| *c == cmd)
            .map_or(&[], |(_, k)| *k),
    })
}

/// 推送帧 kind → 帧里成品住的那几处（同 [`PLACES`] 的写法；空串 ＝ 整帧就是那件成品）。这条流的声明只许点这几件。
pub(crate) const STREAM_PLACES: &[(&str, &[(&str, &str)])] = &[
    ("line", &[("record", "record")]),
    ("session_added", &[("", "session_added")]),
    ("session_status", &[("", "session_status")]),
    ("session_state", &[("", "session_state")]),
    ("session_removed", &[("", "session_removed")]),
];

/// **一条流的出口声明**（流模式起参 `--view`）：与请求信封的 `view` 同一套词、同一处解（[`parse_view`]）、
/// 同一处裁（[`project_reply`]）；只许点推送帧里住着的那几件成品（[`STREAM_PLACES`]）。应答不受它管（应答各按各的请求信封）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamView(View);

impl StreamView {
    /// 解、校验。缺 / `null` ⇒ `Ok(None)`（全量）；认不出 · 点了流上不出的成品 ⇒ `Err(那几处)`。
    pub fn parse(v: &Value) -> Result<Option<StreamView>, String> {
        let Some(view) = parse_view(v)? else {
            return Ok(None);
        };
        let stray: Vec<&str> = view
            .products()
            .into_iter()
            .filter(|p| {
                !STREAM_PLACES
                    .iter()
                    .any(|(_, places)| places.iter().any(|(_, q)| q == p))
            })
            .collect();
        if !stray.is_empty() {
            return Err(format!(
                "view names {} but the stream carries record, session_added, session_status, session_state, session_removed",
                stray.join(", ")
            ));
        }
        Ok(Some(StreamView(view)))
    }

    /// 这一帧（已序列化成一团）照声明裁好。帧的 `kind` 不受裁（挑格只挑成品的格，不挑帧的标签）。
    pub fn apply(&self, frame: &mut Value) {
        let Some(kind) = frame
            .get("kind")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        let Some((_, places)) = STREAM_PLACES.iter().find(|(k, _)| *k == kind) else {
            return;
        };
        if !places.iter().any(|(_, q)| self.0.products().contains(q)) {
            return;
        }
        project_reply(frame, places, &self.0);
        if let Some(o) = frame.as_object_mut() {
            o.insert("kind".to_string(), Value::String(kind));
        }
    }
}

/// [`crate::wire::to_line`] 照这条流的出口声明裁过（[`StreamView`]；`None` ＝ 全量，字节与 `to_line` 同）。流的写者对每一帧调它。
pub fn to_line_viewed(
    frame: &crate::wire::Frame,
    view: Option<&StreamView>,
) -> serde_json::Result<String> {
    let Some(view) = view else {
        return crate::wire::to_line(frame);
    };
    let mut v = serde_json::to_value(frame)?;
    view.apply(&mut v);
    let mut s = serde_json::to_string(&v)?;
    s.push('\n');
    Ok(s)
}

#[cfg(test)]
#[path = "../../../../tests/backend/stream/views_tests.rs"]
mod tests;

/// 一条流的客户端要了什么：时区 ＋ 出口的声明。缺 ＝ 全量（bg 会话照宣告、`session_added` 带 `pid`；历史一律不重放，客户端按骨架 / 尾段补）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StreamWants {
    /// [`crate::wire::cli_words::TZ_FLAG`] `<IANA 名>`：看的那一台的时区 —— 这条流推出去的「几点」（`line.record.timeText` 一类）按它写；
    /// 一次性 CLI 面的回包同样按它写。缺 · 认不得 ⇒ UTC。
    pub tz: crate::Tz,
    /// 这条流的出口声明（attach 行的 `view` · 流模式起参的 [`crate::wire::cli_words::VIEW_FLAG`]，与请求信封的 `view` 同一套词）：推出去的帧照它裁格
    /// （[`StreamView`]）。缺 ＝ 全量。
    pub view: Option<StreamView>,
}
