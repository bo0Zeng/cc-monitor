//! Codex 的答题口（注册表 `Adapter.asks` 那一格）：起会话时经 `-c` 给这一个会话挂上会话级钩子（连信任哈希一起交），
//! 钩子交来的「提交了一句 · 一轮结束 · 会话起来了」翻成统一事件（[`heard`]：会话在干什么先照它，记录只兜底）。
//!
//! 依据（codex-rs，核到 tag `rust-v0.167.0`；`rust-v0.159.2` 同形）：
//! - 钩子特性 Stable、默认开；事件名与入参形同 Claude Code（四种都带 `session_id` · `hook_event_name`）。
//!   钩子命令经 `sh -lc` / `cmd /C` 跑。
//! - `-c` 进的是会话层，不算托管来源 ⇒ 要信任才跑；信任状态读用户层与会话层的 `hooks.state."<键>".trusted_hash`
//!   ⇒ 同一发里一起交。不用 `--dangerously-bypass-hook-trust`：它连用户自己没信任过的钩子也放行。
//! - 信任键 · 信任哈希 · 版本门是 Codex 内部的规则，住 [`super::hook_trust`]。
//! - `-c` 的键按 `.` 硬切，会话层路径里带 `config.toml` 的点 ⇒ 信任那几格不能写成点路径，
//!   只能和钩子装进同一个 `-c hooks={…}` 内联表。
//!
//! 答题那一种（`PermissionRequest`）：入参读成一条待答的批准（[`read`]），人答了出允许 / 拒绝那一份（[`decide`]）；
//! 到点没人答 ⇒ 钩子不给决定，Codex 照常在终端里弹批准框。
//!
//! 这一家挂钩子的字段名 · 旗标只住这里（信任键里的来源路径住 [`super::hook_trust`]）。

use super::hook_trust::{hook_key, trusted_hash};
use crate::agents::{AgentEvent, AskFace, AskIn, Verdict};
use serde_json::{json, Map, Value};

/// 注册表 `Adapter.asks` 那一格。
pub(crate) const FACE: AskFace = AskFace {
    launch_args,
    launch_flag: CONFIG_FLAG,
    // `-c` 可重复、各写各的键：用户自己写的那几条不读不并，我们那两个词照垫。
    merge: None,
    gate: Some(super::hook_trust::GATE),
    // 用户自己写 `-c hooks=…` 会整格盖掉我们那张表（[`overridden`]）。
    overridden: Some(overridden),
    read,
    decide,
    cannot,
    // 停不下：`PermissionRequest` 的回包只认 `behavior` ＋ 可缺的 `message`，带 `interrupt` Codex 当这道钩子失败
    // （codex-rs `rust-v0.167.0` permission-request 输出 schema）⇒ 「拒绝并停下」不出。
    stop_on_deny: false,
    heard,
    plugin: None,
    // 批准框的排法 · 键位没核过（只抽到字串）：不认框，这一种在 app 里只写「在终端里答」。
    on_screen: None,
};

/// 会话级覆盖那个旗标（可重复，每一发各写各的键）。
pub(crate) const CONFIG_FLAG: &str = "-c";
/// 钩子那一张表的键（`-c hooks={…}` 整张交）。
const HOOKS_KEY: &str = "hooks";
/// 信任状态那一格（`hooks.state`）。
const STATE_KEY: &str = "state";

const PROMPT_EVENT: &str = "UserPromptSubmit";
const STOP_EVENT: &str = "Stop";
const START_EVENT: &str = "SessionStart";
/// 答题那一种。
const ASK_EVENT: &str = "PermissionRequest";

/// 挂的那几种：（事件名, 键里那一段, 期限秒）。答题那一种等人答，别的交了就走。
const HOOKS: [(&str, &str, u64); 4] = [
    (PROMPT_EVENT, "user_prompt_submit", EVENT_HOOK_TIMEOUT_SECS),
    (STOP_EVENT, "stop", EVENT_HOOK_TIMEOUT_SECS),
    (ASK_EVENT, "permission_request", HOOK_TIMEOUT_SECS),
    (START_EVENT, "session_start", EVENT_HOOK_TIMEOUT_SECS),
];
/// 一轮的那几种 hook 的期限（秒）：交给常驻就走，常驻卡住也不拖住 Codex。
pub(crate) const EVENT_HOOK_TIMEOUT_SECS: u64 = 10;
/// 答题那一种的期限（秒）：到点被杀、不给决定 ⇒ 框照常在终端里。
pub(crate) const HOOK_TIMEOUT_SECS: u64 = 3600;

/// `SessionStart` 里说「会话闲着等第一句」的那几种来源（`compact` 是一轮中途压完上下文，不算）。
const IDLE_STARTS: [&str; 4] = ["startup", "resume", "clear", "fork"];

/// 起会话时垫的那两个词：`-c` ＋ `hooks={…四种…, state={…各自的 trusted_hash…}}`。
pub fn launch_args(entry: &str) -> Vec<String> {
    let command = command_of(entry);
    let mut hooks = Map::new();
    let mut state = Map::new();
    for (event, label, timeout) in HOOKS {
        hooks.insert(
            event.to_string(),
            json!([{ "hooks": [{ "type": "command", "command": command, "timeout": timeout }] }]),
        );
        state.insert(
            hook_key(label),
            json!({ "trusted_hash": trusted_hash(label, &command, timeout) }),
        );
    }
    hooks.insert(STATE_KEY.to_string(), Value::Object(state));
    vec![
        CONFIG_FLAG.to_string(),
        format!("{HOOKS_KEY}={}", inline_toml(&Value::Object(hooks))),
    ]
}

/// 钩子那一行命令（经那一家的 shell 跑）：ccm 入口加引号 ＋ 入口那几个词（`-- --answer-hook codex`）。
pub(crate) fn command_of(entry: &str) -> String {
    let [end, flag] = crate::agents::asks::ANSWER_HOOK_WORDS;
    let quoted = if cfg!(windows) {
        format!("\"{entry}\"")
    } else {
        shell_quote_core::posix_quote(entry)
    };
    format!("{quoted} {end} {flag} {}", super::AGENT_KIND)
}

/// JSON ⇒ TOML 内联写法（`-c` 右边那一段按 TOML 解析）：键一律加引号；串用 JSON 的转义（TOML 基本串认同一套）。
/// 只出现串 · 整数 · 布尔 · 数组 · 表（我们自己造的那一份）。
fn inline_toml(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            let parts: Vec<String> = m
                .iter()
                .map(|(k, v)| format!("{}={}", Value::String(k.clone()), inline_toml(v)))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        Value::Array(a) => format!(
            "[{}]",
            a.iter().map(inline_toml).collect::<Vec<_>>().join(",")
        ),
        other => other.to_string(),
    }
}

/// hook 的 stdin ⇒（会话 id, 统一事件）（注册表 `AskFace.heard`）。答题那一种 · 中途压缩的那次起来 · 缺会话 id ⇒ `None`。
pub fn heard(v: &Value) -> Option<(String, AgentEvent)> {
    let event = match v.get("hook_event_name").and_then(Value::as_str)? {
        PROMPT_EVENT => AgentEvent::PromptSubmitted,
        STOP_EVENT => AgentEvent::TurnEnded,
        START_EVENT
            if v.get("source")
                .and_then(Value::as_str)
                .is_some_and(|s| IDLE_STARTS.contains(&s)) =>
        {
            AgentEvent::TurnEnded
        }
        _ => return None,
    };
    let sid = v.get("session_id").and_then(Value::as_str)?.trim();
    (!sid.is_empty()).then(|| (sid.to_string(), event))
}

/// `PermissionRequest` 的入参 ⇒ 一条待答的批准（注册表 `AskFace.read`）。别的事件 · 缺会话 id / 工具名 ⇒ `None`。
/// 进这道钩子的全是批准（命令 `Bash` · 改文件 `apply_patch` · 写进跑着的命令 · MCP 工具 · 要权限），一律 `Approve`。
/// Codex 先跑钩子、钩子没给决定才弹批准框 ⇒ 挡着终端；它不给「以后不再问」的建议。
fn read(v: &Value) -> Option<AskIn> {
    if v.get("hook_event_name").and_then(Value::as_str) != Some(ASK_EVENT) {
        return None;
    }
    let sid = nonempty(v.get("session_id"))?;
    let tool = nonempty(v.get("tool_name"))?;
    let input = v.get("tool_input").cloned().unwrap_or(Value::Null);
    Some(AskIn {
        sid,
        kind: crate::agents::AskKind::Approve,
        what: what_of(&tool, &input),
        detail: detail_of(&tool, &input),
        tool,
        suggestions: Vec::new(),
        questions: Vec::new(),
        agent_id: nonempty(v.get("agent_id")),
        agent_type: nonempty(v.get("agent_type")),
        blocks_terminal: true,
        input,
    })
}

fn nonempty(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
}

/// 命令 · 补丁那一格（两种工具都放在 `command`）。
fn command_in(input: &Value) -> Option<&str> {
    input.get("command").and_then(Value::as_str)
}

/// 补丁里各段的文件头（`*** Add File: ` · `*** Update File: ` · `*** Delete File: `）。
const PATCH_HEADS: [&str; 3] = ["*** Add File: ", "*** Update File: ", "*** Delete File: "];

/// 钉条那一段：命令的头一行 · 补丁里头一个文件；别的说不出 ⇒ `None`。
fn what_of(tool: &str, input: &Value) -> Option<String> {
    let cmd = command_in(input)?;
    let line = match tool {
        "apply_patch" => cmd
            .lines()
            .find_map(|l| PATCH_HEADS.iter().find_map(|h| l.strip_prefix(h)))?,
        "Bash" => cmd.lines().next()?,
        _ => return None,
    };
    Some(line.trim().to_string()).filter(|s| !s.is_empty())
}

/// 要批的东西的全文（不截）：命令 · 补丁原文；别的工具 ⇒ 参数原样排开。
fn detail_of(tool: &str, input: &Value) -> String {
    match (tool, command_in(input)) {
        ("Bash" | "apply_patch", Some(c)) => c.to_string(),
        _ => serde_json::to_string_pretty(input).unwrap_or_default(),
    }
}

/// 用户自己写的 `-c hooks=…` 整格盖掉我们那一张 `-c hooks={…}`（注册表 `AskFace.overridden`）：`-c` 都在会话那一层、后写的胜，
/// 键按 `.` 切 ⇒ 键恰是 `hooks` 的那一条把整张表换掉；`hooks.state.x=…` 这种只动里面一格、不算。
/// 旗标的几种写法照 clap：`-c <值>` · `--config <值>` · `--config=<值>` · `-c<值>` · `-c=<值>`；`--` 之后是第一句，不看。
fn overridden(args: &[String]) -> bool {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let v = match a.as_str() {
            "--" => return false,
            "-c" | "--config" => it.next().map(String::as_str),
            s => s.strip_prefix("--config=").or_else(|| {
                s.strip_prefix("-c")
                    .map(|r| r.strip_prefix('=').unwrap_or(r))
                    .filter(|r| !r.is_empty())
            }),
        };
        if v.is_some_and(|v| {
            v.trim_start()
                .strip_prefix(HOOKS_KEY)
                .is_some_and(|r| r.trim_start().starts_with('='))
        }) {
            return true;
        }
    }
    false
}

/// 经钩子答不了的那几种（注册表 `AskFace.cannot`）：Codex 只有批准那一种（`PermissionRequest`）有钩子回包能答；
/// 提问 · 批计划（Codex 问用户的那个工具）没有 ⇒ 照常在终端里问、记需手动。
fn cannot(a: &AskIn) -> bool {
    a.kind != crate::agents::AskKind::Approve
}

/// 人的答案 ⇒ 钩子打到 stdout 的那一整份（注册表 `AskFace.decide`），形照 permission-request 的输出 schema：
/// 只有 `behavior`（`allow` / `deny`）与可缺的 `message`。`interrupt` · `updatedPermissions` · `updatedInput` 三格
/// Codex 一见就当这一道钩子失败 ⇒ 一格都不带：「拒绝并停下」也只拒绝，选的那条建议（它本来就不给）不带。
fn decide(_: &AskIn, verdict: &Verdict) -> Option<Value> {
    let decision = match verdict {
        Verdict::Allow { .. } => json!({"behavior": "allow"}),
        Verdict::Deny { note, .. } => match note.as_deref().filter(|n| !n.trim().is_empty()) {
            Some(n) => json!({"behavior": "deny", "message": n}),
            None => json!({"behavior": "deny"}),
        },
    };
    Some(json!({"hookSpecificOutput": {"hookEventName": ASK_EVENT, "decision": decision}}))
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/asks_tests.rs"]
mod tests;
