//! 「现在就换」（`rotation-switch`）的帧面宿主：不重启换交给 [`super::rotation_face`] 那一半（钉号 · 记一条）；
//! 重启换逐个交给 `session-restart`（成了再记一条）。单住一份：只有这一条够得着 tmux（经 `session-restart`）。

use super::rotation_face::{hot_one, outcome, record, Ctx};
use crate::accounts::quota::rotation::{OldSession, RestartOutcome, SwitchOutcome, SwitchWhy};
use crate::accounts::upstream_select::rotate::account_ok;
use serde_json::{json, Map, Value};
use std::sync::Arc;

type Answer = Result<Value, (&'static str, String)>;

fn bad(detail: &str) -> (&'static str, String) {
    ("bad_args", crate::common::contract::malformed(detail))
}

/// 「现在就换」的入参：`{sessions, target, mode}`。不重启换 ⇒ `sessions` 是会话 id；重启换 ⇒ 每项是
/// `session-restart` 的入参（缺 `account`，用 `target`）。
pub(crate) struct SwitchAsk {
    target: String,
    restart: bool,
    items: Vec<Value>,
}

pub(crate) fn switch_ask(args: &Value) -> Result<SwitchAsk, (&'static str, String)> {
    let target = args
        .get("target")
        .and_then(Value::as_str)
        .filter(|s| account_ok(s))
        .ok_or_else(|| bad("`target` must be an account id"))?
        .to_string();
    let restart = match args.get("mode").and_then(Value::as_str) {
        Some("hot") => false,
        Some("restart") => true,
        _ => return Err(bad("`mode` must be \"hot\" or \"restart\"")),
    };
    let items = args
        .get("sessions")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("`sessions` must be an array"))?
        .clone();
    for (i, it) in items.iter().enumerate() {
        let sid = if restart {
            it.get("sid").and_then(Value::as_str)
        } else {
            it.as_str()
        };
        if !sid.is_some_and(shell_quote_core::session_id_ok) {
            return Err(bad(&format!(
                "`sessions[{i}]` must be {}",
                if restart {
                    "an object with a session id `sid`"
                } else {
                    "a session id"
                }
            )));
        }
    }
    Ok(SwitchAsk {
        target,
        restart,
        items,
    })
}

/// 重启换交给 `session-restart` 的那一份：原样 ＋ `account` ＝ 目标号。
pub(crate) fn restart_args(item: &Value, target: &str) -> Value {
    let mut one = item.clone();
    one["account"] = Value::from(target);
    one
}

/// `session-restart` 的结局 ⇒ 重启换那一格：报出了 ⇒ 成；起了没报出 ⇒ `notArrived`（旧的已停）；
/// 失败 ⇒ 码原样，`data.stopped` 说旧的停没停。
pub(crate) fn restart_outcome(
    r: &Result<Value, crate::control::session_restart::Fault>,
) -> RestartOutcome {
    match r {
        Ok(v) if v["started"] == "arrived" => RestartOutcome::Restarted {
            terminal: v["terminal"].as_str().unwrap_or_default().to_string(),
        },
        Ok(_) => RestartOutcome::NotRestarted {
            code: "notArrived".into(),
            old: OldSession::Ended,
        },
        Err((code, _, data)) => RestartOutcome::NotRestarted {
            code: code.clone(),
            old: if data.as_ref().and_then(|d| d["stopped"].as_bool()) == Some(true) {
                OldSession::Ended
            } else {
                OldSession::Kept
            },
        },
    }
}

/// `rotation-switch`：现在就换（`{sessions, target, mode}`）→ 每个会话 `done` / `skipped{code}` / `failed{code}`；
/// 重启换那一格是 [`RestartOutcome`]。
/// 不重启换在这里做完；重启换逐个交给 `session-restart`（成了再记一条；各步都收紧到这一发的截止时刻 `until`）。
pub(crate) async fn answer_switch(
    args: Value,
    until: Option<crate::platform::child::Until>,
    files: super::launch_face::Files,
) -> Answer {
    let ask = switch_ask(&args)?;
    let now = crate::accounts::quota::now_unix();
    let mut out = Map::new();
    if !ask.restart {
        let ask = Arc::new(ask);
        let a = Arc::clone(&ask);
        let done = tokio::task::spawn_blocking(move || {
            let ctx = Ctx::here();
            a.items
                .iter()
                .filter_map(Value::as_str)
                .map(|sid| (sid.to_string(), hot_one(&ctx, sid, &a.target, now)))
                .collect::<Vec<_>>()
        })
        .await
        .map_err(|e| ("failed", e.to_string()))?;
        for (sid, o) in done {
            out.insert(sid, outcome(o)?);
        }
        return Ok(json!({ "sessions": out }));
    }
    for item in ask.items {
        let sid = item["sid"].as_str().unwrap_or_default().to_string();
        let r = crate::faces::session_restart_face::answer(
            restart_args(&item, &ask.target),
            until,
            files,
        )
        .await;
        let mut o = restart_outcome(&r);
        if r.is_ok() {
            // 换过去了（报没报出都算）⇒ 记一条；写不进 ⇒ `ioFailed`（旧的已停）。
            let (s, t) = (sid.clone(), ask.target.clone());
            let kept = tokio::task::spawn_blocking(move || {
                let ctx = Ctx::here();
                let book = ctx.hop.store.now();
                match book.sessions.get(&s).map(|e| e.current.clone()) {
                    Some(from) => record(&ctx, &s, &from, &t, SwitchWhy::ManualRestart, now),
                    None => SwitchOutcome::Switched,
                }
            })
            .await
            .map_err(|e| ("failed", e.to_string()))?;
            if let SwitchOutcome::NotSwitched { code } | SwitchOutcome::Skipped { code } = kept {
                o = RestartOutcome::NotRestarted {
                    code,
                    old: OldSession::Ended,
                };
            }
        }
        out.insert(
            sid,
            serde_json::to_value(o).map_err(|e| ("failed", e.to_string()))?,
        );
    }
    Ok(json!({ "sessions": out }))
}
