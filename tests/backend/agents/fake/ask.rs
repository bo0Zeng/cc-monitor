//! 假 agent 的答题口（`AskFace`）：与 Claude 每一格都不同形 —— hook 交来的是 `{sess, event, …}`（Claude 是 `session_id` · `hook_event_name`）；
//! 允许回 `{decision: "allow"}`、拒绝回 `{decision: "deny"}`（Claude 是 `hookSpecificOutput.decision.behavior`）；
//! **提问那一种这一家不给经钩子答的口**（[`AskFace::cannot`] 认它）：通用层得照它不挂、即刻退、落需手动；
//! **拒绝时停不下那一轮**（[`AskFace::stop_on_deny`] 为假）：成品里不带「拒绝并停下」那一项；
//! 版本门（[`GATE`]）只声明命令名与包名（与 Claude 不同），插件那一路（[`ASK_PLUGIN`]）带同一个门；
//! 透传里写了 [`TABLE_FLAG`] ⇒ 整格盖掉我们垫的钩子表（[`AskFace::overridden`]）。
//!
//! 夹具形状是编的（这一家不存在），正文不放真东西。

use crate::agents::{AgentEvent, AskFace, AskIn, AskKind, HookGate, PluginFace, Verdict};
use serde_json::{json, Value};

/// 注册表 `Adapter.asks` 那一格（这一家不进 `REGISTRY`，判据直接拿它拼一行交通用层）。
pub const ASK: AskFace = AskFace {
    launch_args,
    launch_flag: LAUNCH_FLAG,
    merge: None,
    gate: Some(GATE),
    overridden: Some(overridden),
    read,
    decide,
    cannot,
    stop_on_deny: false,
    heard,
    plugin: None,
    on_screen: None,
};

/// 版本门：`PATH` 上的 [`super::DEFAULT_COMMAND`]、npm 包名 `@fake/agent`（读法是通用的那一个，这里只声明）。
pub const GATE: HookGate = HookGate {
    min_version: [1, 2, 0],
    command: super::DEFAULT_COMMAND,
    package: "@fake/agent",
};

/// 插件那一路（[`ASK`] 不带它；计划判据要比「两种挂法判出的一样」时拿它拼一格）：与默认启动器那一支同一个 [`GATE`]。
pub const ASK_PLUGIN: PluginFace = PluginFace {
    env: "FAKE_AGENT_PLUGINS",
    gate: GATE,
    files: |_| vec![],
    policy_files: Vec::new,
    policy_blocks: |_| false,
    child_marker: "FAKE_AGENT_CHILD",
};

/// 整格盖掉钩子表的那个旗标（`--ask-table <表>` · `--ask-table=<表>`）：写了它，我们垫的 [`LAUNCH_FLAG`] 那几条就不算数。
pub const TABLE_FLAG: &str = "--ask-table";

fn overridden(args: &[String]) -> bool {
    args.iter().any(|a| {
        a == TABLE_FLAG
            || a.strip_prefix(TABLE_FLAG)
                .is_some_and(|rest| rest.starts_with('='))
    })
}

/// 会话级挂钩子那个旗标（可重复）。
pub const LAUNCH_FLAG: &str = "--on-ask";

fn launch_args(ccm: &str) -> Vec<String> {
    vec![
        LAUNCH_FLAG.to_string(),
        format!("{ccm} -- --answer-hook fake"),
    ]
}

/// 要人答的那一种：`{sess, event: "ask", kind: "permit" | "query", tool, args}`。
fn read(v: &Value) -> Option<AskIn> {
    if v.get("event").and_then(Value::as_str) != Some("ask") {
        return None;
    }
    let sid = v.get("sess").and_then(Value::as_str)?.to_string();
    let kind = match v.get("kind").and_then(Value::as_str)? {
        "permit" => AskKind::Approve,
        "query" => AskKind::Answer,
        _ => return None,
    };
    let tool = v.get("tool").and_then(Value::as_str)?.to_string();
    let input = v.get("args").cloned().unwrap_or(Value::Null);
    Some(AskIn {
        sid,
        kind,
        what: input
            .get("line")
            .and_then(Value::as_str)
            .map(str::to_string),
        detail: input.to_string(),
        tool,
        suggestions: vec![],
        questions: vec![],
        agent_id: None,
        agent_type: None,
        blocks_terminal: false,
        input,
    })
}

/// 允许 ⇒ `{decision: "allow"}`；拒绝 ⇒ `{decision: "deny"}`（这一家不认附言、不认停下）。
fn decide(_: &AskIn, verdict: &Verdict) -> Option<Value> {
    Some(match verdict {
        Verdict::Allow { .. } => json!({"decision": "allow"}),
        Verdict::Deny { .. } => json!({"decision": "deny"}),
    })
}

/// 提问那一种这一家不给经钩子答。
fn cannot(a: &AskIn) -> bool {
    a.kind == AskKind::Answer
}

/// 一轮的那几种：`{sess, event: "said" | "rest"}`。
fn heard(v: &Value) -> Option<(String, AgentEvent)> {
    let event = match v.get("event").and_then(Value::as_str)? {
        "said" => AgentEvent::PromptSubmitted,
        "rest" => AgentEvent::TurnEnded,
        _ => return None,
    };
    let sid = v.get("sess").and_then(Value::as_str)?;
    Some((sid.to_string(), event))
}
