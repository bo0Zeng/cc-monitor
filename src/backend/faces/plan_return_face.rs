//! 计划审面退回那一半（`plan-return`）的帧面宿主：拼「人 · {编号} {标题}：{原话}」（对象是一格，或顶块 `project`：标题是片名），先判目标能不能送 ——
//! 在等你（批准 · 回答 · 计划）⇒ 拒；已结束 / 认不出 ⇒ 只给这一行去复制；能送就走 `terminal-input` 的本体（同一道身份门、
//! 同一个「画面变了不送」，不另造路），送到了 / 送达未知 ⇒ 记一条「已退回」（[`crate::plan::review`] 的退回写口只从本文件进），
//! 之后 `plan-read` 里那一格带落没落地（[`crate::plan::needs::landing`]）。

use crate::plan::needs::{self, Returned};
use crate::stream::inbound::spec::Fail;
use copy_core::copy_text;
use serde_json::{json, Map, Value};

type Answer = Result<Value, Fail>;

fn bad(what: &str) -> Fail {
    Fail::new("bad_args", crate::common::contract::malformed(what))
}

fn now_ms() -> u64 {
    crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn str_arg<'a>(args: &'a Value, k: &str) -> Result<&'a str, Fail> {
    args.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| bad(&format!("missing `{k}` (a string)")))
}

fn slice_of<'a>(doc: &'a Value, name: &str) -> Option<&'a Value> {
    doc.get("slices")?
        .as_array()?
        .iter()
        .find(|s| s.get("name").and_then(Value::as_str) == Some(name))
}

/// 送一次（生产 ＝ `terminal-input` 的本体）。
pub(crate) type Send<'a> = &'a dyn Fn(&Value) -> Result<Value, Fail>;
/// 记一次退回（生产 ＝ [`crate::plan::review`] 的写口）。
pub(crate) type Record<'a> = &'a dyn Fn(&str, &str, &str, Returned) -> Result<(), Fail>;

fn reply(line: &str, to: &Value, result: &str, why: Option<&str>, said: Option<String>) -> Value {
    json!({"line": line, "to": to, "result": result, "why": why, "said": said, "screen": null})
}

/// `plan-return` 的本体：`doc` 是此刻现读、标过的那一份。
pub(crate) fn return_with(
    args: &Value,
    doc: &Value,
    send: Send,
    record: Record,
    now: u64,
) -> Answer {
    let (ws, slice, id) = (
        str_arg(args, "workspace")?,
        str_arg(args, "slice")?,
        str_arg(args, "id")?,
    );
    let text = str_arg(args, "text")?;
    if text.trim().is_empty() {
        return Err(bad("`text` is empty"));
    }
    let role = match args.get("to") {
        None => "owner",
        Some(Value::String(s)) if s == "owner" || s == "signer" => s.as_str(),
        Some(_) => return Err(bad("`to` must be `owner` or `signer`")),
    };
    let no_cell = || Fail::new("no_such_cell", copy_text("bePlan.review.noSuchCell", &[]));
    let sl = slice_of(doc, slice).ok_or_else(no_cell)?;
    // 一格，或顶块 `project`（与格同形，[`needs::target`]）。
    let cell = &needs::target(sl, id).ok_or_else(no_cell)?;
    let title = cell
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let line = needs::line(id, title, text);
    let to = cell.get(role).cloned().unwrap_or(Value::Null);
    if to.is_null() {
        return Err(Fail::new(
            "no_target",
            copy_text("bePlan.review.noTarget", &[("role", role)]),
        ));
    }
    let Some(sid) = to.get("sid").and_then(Value::as_str) else {
        return Ok(reply(
            &line,
            &to,
            "copy",
            Some("unknown"),
            Some(copy_text("bePlan.return.unknown", &[])),
        ));
    };
    if to.get("alive").and_then(Value::as_bool) != Some(true) {
        return Ok(reply(
            &line,
            &to,
            "copy",
            Some("ended"),
            Some(copy_text("sessionState.planReturn.ended", &[])),
        ));
    }
    if to.get("activity").and_then(Value::as_str) == Some("needs_you") {
        return Ok(reply(
            &line,
            &to,
            "refused",
            Some("waiting"),
            Some(copy_text("bePlan.return.waiting", &[])),
        ));
    }
    let mut input = Map::new();
    input.insert("sid".into(), json!(sid));
    input.insert("text".into(), json!(line));
    input.insert("enter".into(), json!(true));
    for k in ["client", "seen_screen"] {
        if let Some(v) = args.get(k) {
            input.insert(k.into(), v.clone());
        }
    }
    let got = send(&Value::Object(input))?;
    let result = got
        .get("result")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if result == "delivered" || result == "unsure" {
        let children = cell
            .get("children")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|c| c.as_str().map(str::to_string))
            .collect();
        record(
            ws,
            slice,
            id,
            Returned {
                at: now,
                to: sid.to_string(),
                result: result.to_string(),
                children,
                body: needs::body_digest(cell),
            },
        )?;
    }
    let mut out = reply(
        &line,
        &to,
        result,
        got.get("why").and_then(Value::as_str),
        got.get("said").and_then(Value::as_str).map(str::to_string),
    );
    out["screen"] = got.get("screen").cloned().unwrap_or(Value::Null);
    Ok(out)
}

fn give_back(args: &Value) -> Answer {
    let ws = str_arg(args, "workspace")?;
    // 只拿此刻的接手与状态（不交出去），时刻的字用不上 ⇒ 按 UTC 写的那一份也行。
    let doc = crate::faces::plan_face::read_fresh(ws, &crate::Tz::default())?;
    let send = |a: &Value| crate::control::terminals::input_for_inbound(a);
    let record = |ws: &str, sl: &str, id: &str, r: Returned| {
        crate::plan::review::answer_returned(ws, sl, id, r)
    };
    return_with(args, &doc, &send, &record, now_ms())
}

/// 帧面入口：命令名从 `r.cmd` 来。
pub(crate) fn answer(cmd: &str, args: &Value) -> Answer {
    match cmd {
        "plan-return" => give_back(args)
            .and_then(crate::faces::plan_face::wired::<crate::plan::wire::PlanReturnReply>),
        other => Err(bad(&format!("unknown command `{other}`"))),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/plan_return_face_tests.rs"]
mod tests;
