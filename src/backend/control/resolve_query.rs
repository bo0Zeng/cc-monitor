//! `resolve`：读 `ResumeSpec`，出 `CommandPlan`（camelCase）。帧命令，CLI 面照派生规则有 `--resolve`（信封与别的命令同一形，`cli_control`）。
//!
//! - 入：`ResumeSpec{sessionId, launchCandidates:[String?], agentKind?}`（别的格不认、不报错，照 serde 缺省忽略）。
//! - 出：`CommandPlan{command, mode:"PtyInject"|"ExecOnce",
//!   capabilities{supportsSendKeys,supportsCapture,supportsMultiClient,supportsMultiWindow},
//!   sessionName?, launchLabel?, substitutedFrom?}`。
//! - 错：命令级码 `bad_args` · `invalid_session_id` · `unsafe_launch_candidate` · `serialize_failed`（登记在 `inbound::REGISTRY` 的 `resolve`）。
//!
//! 只返命令串：backend 零 handle、绝不执行。sessionId 会进 command 串 ⇒ 后端自己产出的 plan 也过一遍本地 `is_valid_session_id`。
//! command = 首个可用 `launchCandidate` + `--resume <sid>`（无候选 ⇒ 默认 `claude`）；caps 是 tmux/pty 的典型档，不是探测值。

use copy_core::copy_text;
use serde::{Deserialize, Serialize};

/// 入参（camelCase 对齐手机端 `ResumeSpec`）。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResumeSpec {
    session_id: String,
    #[serde(default)]
    launch_candidates: Vec<Option<String>>,
    /// 会话属哪个 agent kind ⇒ 据此构 `codex resume <uuid>` 或 `claude --resume`（additive，线上 `agentKind`）。
    /// 注册表里的一家 = 那一家；缺 / `""` = 默认那一家；别的名字 ⇒ `bad_args`。
    #[serde(default)]
    agent_kind: String,
}

/// 出参 caps（4 名**逐字复用手机端 `SessionCapabilities`**，camelCase 免映射）。
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Capabilities {
    supports_send_keys: bool,
    supports_capture: bool,
    supports_multi_client: bool,
    supports_multi_window: bool,
}

/// 出参（camelCase 对齐手机端 `ResumePlan`，另加 mode/capabilities）。
///
/// 三个字段的可信度不一样，而字段名读起来一模一样：
///
/// | 字段 | 可信度 |
/// |---|---|
/// | `command` | 可信：调用方给的候选 + `--resume <调用方给的 sid>` |
/// | `session_name` | 派生：纯从 sid 拼（`cc-<sid8>`），没读过 pidfile、没查过 tmux |
/// | `capabilities` | 典型档：硬编码的常见组合，不是这台机器此刻的实测能力 |
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

/// `resolve` 命令的入口（帧面与派生的 CLI 面同一个）。
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

/// 纯：ResumeSpec JSON 串 → CommandPlan JSON 串（或 `(code,message)`）。入口与单测共用。
fn resolve_from_json(
    registry: &[crate::agents::Adapter],
    input: &str,
) -> Result<String, (&'static str, String)> {
    let spec: ResumeSpec = serde_json::from_str(input.trim())
        .map_err(|e| ("bad_args", format!("ResumeSpec JSON parse failed: {e}")))?;
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
    // 哪一家：缺 / `""` ⇒ 注册表里声明默认的那一家；注册表里没有的名字 ⇒ `bad_args`，那句话列出认得的几家（不落默认）。
    // **大小写敏感**：`.trim()` 仅容空白、不容大小写。
    let (_, face) = crate::agents::pick_kind_among(registry, Some(&spec.agent_kind))
        .map_err(|say| ("bad_args", say))?;
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
        mode: "PtyInject".to_string(), // MVP 恒 PtyInject（手机端今隐含亦此）
        capabilities: Capabilities {
            // MVP tmux/pty 典型档（PtyInject 注入 tmux 会话）；待 backend 探测细化。
            supports_send_keys: true,
            supports_capture: true,
            supports_multi_client: true,
            supports_multi_window: true,
        },
        session_name: Some(session_name_for(&spec.session_id, face.session_name_prefix)),
        launch_label: None, // MVP 不产 label（手机端那一侧自算）
        // substitutedFrom（手机端语义）=「被替换掉的原命令」：仅当解析出的 launch ≠ 用户原意首候选时非空。
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

#[cfg(test)]
#[path = "../../../tests/backend/control/resolve_query_tests.rs"]
mod tests;
