//! resolve RPC：一次性 exec `--resolve`，读 ResumeSpec JSON，出 stdout 的 CommandPlan JSON
//! （camelCase，字段名与仓外 aterm 严格一致）。
//!
//! 契约（与 aterm 冻结对齐）：
//! - 入：`ResumeSpec{sessionId, launchCandidates:[String?], claudeDir, fallbackCwd, alreadyInTmux, agentKind?}`，
//!   stdin（可带 `--stdin-line`）或 argv `--args-b64 <base64 的 JSON>`，二选一 —— 与别的 CLI 子命令同一处读（`cli_args::read_args`），
//!   同一套上限与码（`args_too_large` · `no_input` · `bad_args`）。
//! - 出：stdout `CommandPlan{command, mode:"PtyInject"|"ExecOnce",
//!   capabilities{supportsSendKeys,supportsCapture,supportsMultiClient,supportsMultiWindow},
//!   sessionName?, launchLabel?, substitutedFrom?}`，exit 0。caps 四个名字复用 aterm 的 `SessionCapabilities`，两端免映射。
//! - 错误：exit 2 + stderr 一行 `{code, message}` JSON。
//! - exec 模型：1 exec = 1 请求 1 响应 1 退出，无 request-id；超时 = 客户端杀 exec。
//!
//! 只返命令串：backend 零 handle、绝不执行。sessionId 会进 command 串 ⇒ 后端自己产出的 plan 也过一遍本地 `is_valid_session_id`。
//! command = 首个可用 `launchCandidate` + `--resume <sid>`（无候选 ⇒ 默认 `claude`）；caps 是 tmux/pty 的典型档，不是探测值。

use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::Path;

/// stdin 入参（camelCase 对齐 aterm `ResumeSpec`）。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResumeSpec {
    session_id: String,
    #[serde(default)]
    launch_candidates: Vec<Option<String>>,
    /// 冻结兼容字段，不许改名（与 `hello` 帧的 `claude_dir` 同族）：它是 `--resolve` 的 stdin 契约（线上是 `claudeDir`），
    /// 与仓外 aterm 冻结对齐。Rust 侧改名 = 线上字段名改名，所以通用层去 agent 名时绕开它，
    /// 登记在 `agent_locality_guard::AGENT_NAMED_WIRE_FIELDS`（带解锁条件）。
    #[allow(dead_code)] // MVP 未用（backend 用自身 agent_home 做 pidfile 查，留字段兼容）
    #[serde(default)]
    claude_dir: String,
    #[allow(dead_code)] // MVP 未用（both-down→local 回退是客户端侧决策，见 §3 三态）
    #[serde(default)]
    fallback_cwd: String,
    #[allow(dead_code)] // MVP 未据此分支（PtyInject 对 alreadyInTmux 与否一致，留字段兼容）
    #[serde(default)]
    already_in_tmux: bool,
    /// 会话属哪个 agent kind ⇒ 据此构 `codex resume <uuid>` 或 `claude --resume`（additive，线上 `agentKind`）。
    /// 注册表里的一家 = 那一家；缺 / `""` = 默认那一家；别的名字 ⇒ `bad_request`。
    #[serde(default)]
    agent_kind: String,
}

/// stdout 出参 caps（4 名**逐字复用 aterm `SessionCapabilities`**，camelCase 免映射）。
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Capabilities {
    supports_send_keys: bool,
    supports_capture: bool,
    supports_multi_client: bool,
    supports_multi_window: bool,
}

/// stdout 出参（camelCase 对齐 aterm `ResumePlan`，另加 mode/capabilities）。
///
/// 三个字段的可信度不一样，而字段名读起来一模一样（跨项目的说明正本在 `src/doc/IPC-PROTOCOL.md` §8「`--resolve`」那一条）：
///
/// | 字段 | 可信度 |
/// |---|---|
/// | `command` | 可信：调用方给的候选 + `--resume <调用方给的 sid>` |
/// | `session_name` | 派生：纯从 sid 拼（`cc-<sid8>`），没读过 pidfile、没查过 tmux |
/// | `capabilities` | 典型档：硬编码的常见组合，不是这台机器此刻的实测能力 |
///
/// `run(_agent_home, …)` 的参数带下划线：手上有 home 目录却没用（`ResumeSpec.claude_dir` 也标着 `#[allow(dead_code)]`）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandPlan {
    /// **唯一可信的一项**。
    command: String,
    /// "PtyInject"（resume 走 pty send-keys 注入 §5④）| "ExecOnce"（未来）。MVP 恒 PtyInject。
    mode: String,
    /// **典型档，不是探测结果**（见结构体头注）。
    capabilities: Capabilities,
    /// **纯从 sid 派生，不是探测结果**（见结构体头注）——
    /// 拿它去 attach 一个「并不存在」的 tmux 会话是现实风险。要判断会话是否真的存在，
    /// 问 `terminals-list`（那是真 `tmux list-panes`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    session_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    launch_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    substituted_from: Option<String>,
}

/// 错误信封（exit 2 + stderr 出此 JSON）。
#[derive(Debug, Serialize)]
struct ResolveError<'a> {
    code: &'a str,
    message: String,
}

/// `--resolve` 入口。读 ResumeSpec、stdout 写 CommandPlan、exit 0；出错 exit 2 + stderr JSON。
/// `_agent_home` 现未用（MVP 不做 pidfile 消解）；留参数与其余 query::run 一致、后续联调用。
pub fn run(_agent_home: &Path, args: &[String]) -> i32 {
    run_io(
        args.get(1..).unwrap_or_default(),
        std::io::stdin(),
        crate::control::cli_args::STDIN_QUIET,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    )
}

/// [`run`] 的本体（stdin / stdout / stderr 是入参，测试喂替身）。`opts` 是 `--resolve` 之后的那些词。
///
/// 入参交给 CLI 面读入参的那一处读（`cli_args::read_args`）：与别的子命令同一套口 —— stdin（可带 `--stdin-line`）
/// 或 argv `--args-b64 <base64 的 JSON>`（第二个前端写不了 stdin）· 同一个上限（超了 `args_too_large`，不截断）·
/// stdin 开着不写 ⇒ `no_input` · 用法错 ⇒ `bad_args`。这几个码都算进与 aterm 的冻结码全集（金样 `oneshot_only_codes`）。
pub(crate) fn run_io<R: Read + Send + 'static>(
    opts: &[String],
    stdin: R,
    quiet: std::time::Duration,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let got = crate::control::cli_args::read_args(opts, stdin, quiet)
        .and_then(|input| resolve_from_json(crate::agents::REGISTRY, &input));
    match got {
        Ok(json) => {
            // stdout 一行 JSON（紧凑、无内嵌裸换行）。写不进去 ⇒ 调用方已经走了，没有第二个地方可以说。
            let _ = writeln!(out, "{json}");
            0
        }
        Err((code, message)) => emit_err(err, code, message),
    }
}

/// 给流通道上的 `resolve` 命令用的入口；与一次性 `--resolve` 复用同一个 [`resolve_from_json`]。
/// 一次性那条路留着：它与仓外 aterm 的契约冻结着、随时可能开始被消费。两条路只差信封 ——
/// 流通道那条有 `id`、可取消、不用为一次极小的 RPC 单开一整条 SSH exec。
pub fn resolve_json_for_inbound(input: &str) -> Result<serde_json::Value, (&'static str, String)> {
    resolve_json_among(crate::agents::REGISTRY, input)
}

/// [`resolve_json_for_inbound`] 的内核：按哪一家 resume 只问 `registry`（生产走 `agents::REGISTRY`，判据喂含夹具家的合成注册表）。
pub(crate) fn resolve_json_among(
    registry: &[crate::agents::Adapter],
    input: &str,
) -> Result<serde_json::Value, (&'static str, String)> {
    let json = resolve_from_json(registry, input)?;
    serde_json::from_str(&json).map_err(|e| {
        (
            "serialize_failed",
            crate::common::contract::malformed(&format!(
                "reading back the CommandPlan failed: {e}"
            )),
        )
    })
}

/// 纯：ResumeSpec JSON 串 → CommandPlan JSON 串（或 `(code,message)`）。`run()` 与单测共用，分发逻辑不必真接 stdin/stdout 就可测。
fn resolve_from_json(
    registry: &[crate::agents::Adapter],
    input: &str,
) -> Result<String, (&'static str, String)> {
    let spec: ResumeSpec = serde_json::from_str(input.trim())
        .map_err(|e| ("bad_request", format!("ResumeSpec JSON parse failed: {e}")))?;
    let plan = resolve(registry, &spec)?;
    serde_json::to_string(&plan).map_err(|e| {
        (
            "serialize_failed",
            format!("CommandPlan serialize failed: {e}"),
        )
    })
}

/// 纯：ResumeSpec → CommandPlan（或 (code,message) 错误）。供单测（不碰 stdin/stdout）。
/// 按哪一家 resume（默认命令 · 命令形 · 会话名前缀）只问 `registry` 里那一家的起会话事实，不认名字。
fn resolve(
    registry: &[crate::agents::Adapter],
    spec: &ResumeSpec,
) -> Result<CommandPlan, (&'static str, String)> {
    // sessionId 会进 command 串 ⇒ 先过本地校验（注入防线，backend 自产也过）。
    if !is_valid_session_id(&spec.session_id) {
        return Err((
            "invalid_session_id",
            copy_text(
                "beResolveQuery.resolve.badSessionId",
                &[("id", &format!("{:?}", spec.session_id))],
            ),
        ));
    }
    // 哪一家：缺 / `""` ⇒ 注册表里声明默认的那一家；注册表里没有的名字 ⇒ `bad_request`，那句话列出认得的几家（不落默认）。
    // **大小写敏感**：`.trim()` 仅容空白、不容大小写。
    let (_, face) = crate::agents::pick_kind_among(registry, Some(&spec.agent_kind))
        .map_err(|say| ("bad_request", say))?;
    // 首个非空 launchCandidate → command 基底；无 → 这一家的默认启动器。
    let candidate = spec
        .launch_candidates
        .iter()
        .flatten()
        .map(|s| s.trim())
        .find(|s| !s.is_empty());
    let base = match candidate {
        Some(c) => c.to_string(),
        None => face.default_launcher.to_string(),
    };
    // `base` 同样进 command 串、由客户端 pty 执行 ⇒ 与 sid 对称地校验 shell-safe（advisory 不执行，
    // 但一份被污染的 ResumeSpec 不该被洗成带后端权威的可注入 CommandPlan）。
    if !is_shell_safe_base(&base) {
        return Err((
            "unsafe_launch_candidate",
            copy_text(
                "beResolveQuery.resolve.unsafeLauncher",
                &[("base", &format!("{:?}", base))],
            ),
        ));
    }
    // command（sid 过 is_valid_session_id、base 过 is_shell_safe_base）；形状是这一家自己的（flag 形 / 子命令形）。
    let command = (face.resume_command)(&base, &spec.session_id);
    Ok(CommandPlan {
        command,
        mode: "PtyInject".to_string(), // MVP 恒 PtyInject（aterm 今隐含亦此）
        capabilities: Capabilities {
            // MVP tmux/pty 典型档（PtyInject 注入 tmux 会话）；待 backend 探测细化。
            supports_send_keys: true,
            supports_capture: true,
            supports_multi_client: true,
            supports_multi_window: true,
        },
        session_name: Some(session_name_for(&spec.session_id, face.session_name_prefix)),
        launch_label: None, // MVP 不产 label（aterm 侧自算）
        // substitutedFrom（aterm 语义）=「被替换掉的原命令」：仅当解析出的 launch ≠ 用户原意首候选时非空。
        // 这里不做候选消解（直接用首候选 / 默认，launch == intended）⇒ 恒为 None（省略）。
        substituted_from: None,
    })
}

/// 校验：非空、仅 `[0-9a-zA-Z_-]`（无 shell 元字符 / 空白）、长度 ≤128。CC 的 sessionId 是 UUID（此集的子集），这里不强求 UUID 形。
fn is_valid_session_id(sid: &str) -> bool {
    !sid.is_empty()
        && sid.len() <= 128
        && sid
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// base（launchCandidate 或默认 `claude`）会进 `command` 串、由客户端 pty 执行 ⇒ 拒 shell 注入。
/// 允许 launcher 常见形（字母数字 / 空格 / `- _ . / = : ,`，支持带路径与 flag），拒 shell 元字符
/// `; | & $ ` ( ) < > \ " ' * ? { } !`、换行 / 控制字符（0x00–0x1f、0x7f）。
fn is_shell_safe_base(s: &str) -> bool {
    !s.is_empty()
        && !s.bytes().any(|b| {
            b < 0x20
                || b == 0x7f
                || matches!(
                    b,
                    b';' | b'|'
                        | b'&'
                        | b'$'
                        | b'`'
                        | b'('
                        | b')'
                        | b'<'
                        | b'>'
                        | b'\\'
                        | b'"'
                        | b'\''
                        | b'*'
                        | b'?'
                        | b'{'
                        | b'}'
                        | b'!'
                )
        })
}

/// resume 会话名：`<这一家的前缀>-<sid 前 8 字符>`（不足 8 取全部）。客户端亦自算、backend 顺带给。
fn session_name_for(sid: &str, prefix: &str) -> String {
    let head: String = sid.chars().take(8).collect();
    format!("{prefix}-{head}")
}

/// 错误信封：`{code, message}` 一行紧凑 JSON。**纯**，抽出来是为了判得到
/// —— 它是给 aterm 的跨仓承诺的一部分（`IPC-PROTOCOL §8`「`--resolve`」那一条），
/// 由 `resolve_query_tests.rs` 里带 `` 的那一族钉着。序列化失败兜底纯文本（同形）。
fn error_envelope(code: &str, message: String) -> String {
    let err = ResolveError { code, message };
    match serde_json::to_string(&err) {
        Ok(s) => s,
        Err(_) => format!("{{\"code\":\"{code}\",\"message\":\"<unserializable>\"}}"),
    }
}

/// 一次性那条的错误退出码。跨仓承诺的一格（aterm `runCatching` 按它认结构化失败）。
const ERROR_EXIT: i32 = 2;

/// 错误统一出口：stderr 写 `{code,message}` JSON（不污染 stdout wire）、返 [`ERROR_EXIT`]。
fn emit_err(err: &mut dyn Write, code: &'static str, message: String) -> i32 {
    // 写不进 stderr ⇒ 没有第二个地方可以说（退出码 2 照样回）。
    let _ = writeln!(err, "{}", error_envelope(code, message));
    ERROR_EXIT
}

#[cfg(test)]
#[path = "../../../tests/backend/control/resolve_query_tests.rs"]
mod tests;
