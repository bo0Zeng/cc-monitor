//! resolve RPC（advisor · ADR-01）：一次性 exec `--resolve`，读 **stdin** 的 ResumeSpec JSON、
//! 输出 **stdout** 的 CommandPlan JSON（camelCase，字段名与 aterm 严格一致）。
//!
//! 契约定死（cc-bus 与 `android-terminal_cc` 对齐 2026-07-18，见 `daemon-协议-v1 §3`）：
//! - **入**：stdin `ResumeSpec{sessionId, launchCandidates:[String?], claudeDir, fallbackCwd,
//!   alreadyInTmux}`（走 stdin 非 argv——`launchCandidates` 可多条、避 argv 长度限）。
//! - **出**：stdout `CommandPlan{command, mode:"PtyInject"|"ExecOnce",
//!   capabilities{supportsSendKeys,supportsCapture,supportsMultiClient,supportsMultiWindow},
//!   sessionName?, launchLabel?, substitutedFrom?}`，exit 0。
//!   ★ caps 4 名**复用 aterm `SessionCapabilities`（`SessionBackend.kt:13`）**——两端 parity 免映射。
//! - **错误**：exit 2 + stderr 出轻结构化 `{code, message}` JSON（aterm 要 resume 失败可诊断；
//!   `runCatching` 也兜 exit2+stderr，取结构化）。
//! - **exec 模型**：1 exec = 1 请求 1 响应 1 退出、天然 1:1，**无 request-id**；超时 = 客户端杀 exec。
//!
//! **advisory not owning（§5④）**：只返命令串、daemon 零 handle、绝不执行后端。
//! **B2 纪律**：daemon 是权威也**保留本地 `is_valid_session_id` 校验**（对 daemon 自己产出的 plan
//! 也过一遍——sessionId 会进 command 串，注入防线）。
//!
//! ★ **MVP 范围**（aterm 现走 β TailTransport、DaemonTransport 未建、**暂不消费 resolve**）：本轮锁
//! **wire 信封**（stdin/stdout/错误/字段名）。command 构建 = 首个可用 `launchCandidate` + `--resume
//! <sid>`（无候选→默认 `claude`），`substitutedFrom` 记来源——合理 MVP 默认；pidfile-based sid 消解
//! （post-/branch 正确 sid + kind，daemon 深层权威）与 aterm `ResumePlan` 模板精确对齐**留 aterm 接
//! DaemonTransport 时联调**（那时才真消费）。caps 用 tmux/pty 典型档、待后续 backend 探测细化。

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;

/// stdin 入参（camelCase 对齐 aterm `ResumeSpec`）。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResumeSpec {
    session_id: String,
    #[serde(default)]
    launch_candidates: Vec<Option<String>>,
    #[allow(dead_code)] // MVP 未用（daemon 用自身 claude_dir 做 pidfile 查，留字段兼容）
    #[serde(default)]
    claude_dir: String,
    #[allow(dead_code)] // MVP 未用（both-down→local 回退是客户端侧决策，见 §3 三态）
    #[serde(default)]
    fallback_cwd: String,
    #[allow(dead_code)] // MVP 未据此分支（PtyInject 对 alreadyInTmux 与否一致，留字段兼容）
    #[serde(default)]
    already_in_tmux: bool,
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

/// stdout 出参（camelCase 对齐 aterm `ResumePlan` + 加 mode/capabilities）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandPlan {
    command: String,
    /// "PtyInject"（resume 走 pty send-keys 注入 §5④）| "ExecOnce"（未来）。MVP 恒 PtyInject。
    mode: String,
    capabilities: Capabilities,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    launch_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    substituted_from: Option<String>,
}

/// 错误信封（exit 2 + stderr 出此 JSON）。
#[derive(Debug, Serialize)]
struct ResolveError {
    code: &'static str,
    message: String,
}

/// `--resolve` 入口。stdin 读 ResumeSpec、stdout 写 CommandPlan、exit 0；出错 exit 2 + stderr JSON。
/// `_claude_dir` 现未用（MVP 不做 pidfile 消解）；留参数与其余 query::run 一致、后续联调用。
pub fn run(_claude_dir: &Path, _args: &[String]) -> i32 {
    let mut input = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut input) {
        return emit_err("stdin_read_failed", format!("read stdin failed: {e}"));
    }
    let spec: ResumeSpec = match serde_json::from_str(input.trim()) {
        Ok(s) => s,
        Err(e) => return emit_err("bad_request", format!("ResumeSpec JSON parse failed: {e}")),
    };
    match resolve(&spec) {
        Ok(plan) => {
            // stdout 一行 JSON（同其余 wire——紧凑、无内嵌裸换行）。
            match serde_json::to_string(&plan) {
                Ok(s) => {
                    println!("{s}");
                    0
                }
                Err(e) => emit_err(
                    "serialize_failed",
                    format!("CommandPlan serialize failed: {e}"),
                ),
            }
        }
        Err((code, message)) => emit_err(code, message),
    }
}

/// 纯：ResumeSpec → CommandPlan（或 (code,message) 错误）。供单测（不碰 stdin/stdout）。
fn resolve(spec: &ResumeSpec) -> Result<CommandPlan, (&'static str, String)> {
    // B2 纪律：sessionId 会进 command 串 → 先过本地校验（注入防线，daemon 自产也过）。
    if !is_valid_session_id(&spec.session_id) {
        return Err((
            "invalid_session_id",
            format!(
                "sessionId 非法（须非空、仅 [0-9a-zA-Z_-]、≤128）：{:?}",
                spec.session_id
            ),
        ));
    }
    // 首个非空 launchCandidate → command 基底；无 → 默认 `claude`（substitutedFrom=None）。
    let candidate = spec
        .launch_candidates
        .iter()
        .flatten()
        .map(|s| s.trim())
        .find(|s| !s.is_empty());
    let (base, substituted_from) = match candidate {
        Some(c) => (c.to_string(), Some(c.to_string())),
        None => ("claude".to_string(), None),
    };
    // MVP command：`<base> --resume <sid>`（sid 已过 is_valid_session_id、注入安全）。
    let command = format!("{base} --resume {}", spec.session_id);
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
        session_name: Some(session_name_for(&spec.session_id)), // cc-<sid8>
        launch_label: None,                                     // MVP 不产 label（aterm 侧自算）
        substituted_from,
    })
}

/// B2 校验：非空、仅 `[0-9a-zA-Z_-]`（无 shell 元字符/空白 = 注入安全）、长度 ≤128。
/// CC sessionId 实为 UUID（此集的子集），此处放宽到安全字符集、不强求 UUID 形（宽松但仍安全）。
fn is_valid_session_id(sid: &str) -> bool {
    !sid.is_empty()
        && sid.len() <= 128
        && sid
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// aterm 展示约定 `cc-<sid8>`（前 8 字符；不足 8 取全部）。客户端亦自算、daemon 顺带给。
fn session_name_for(sid: &str) -> String {
    let head: String = sid.chars().take(8).collect();
    format!("cc-{head}")
}

/// 错误统一出口：stderr 写 `{code,message}` JSON、返 exit code 2。
fn emit_err(code: &'static str, message: String) -> i32 {
    let err = ResolveError { code, message };
    // stderr（不污染 stdout wire）；序列化失败兜底纯文本。
    match serde_json::to_string(&err) {
        Ok(s) => eprintln!("{s}"),
        Err(_) => eprintln!("{{\"code\":\"{code}\",\"message\":\"<unserializable>\"}}"),
    }
    2
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn spec(sid: &str, candidates: Vec<Option<&str>>) -> ResumeSpec {
        ResumeSpec {
            session_id: sid.to_string(),
            launch_candidates: candidates
                .into_iter()
                .map(|o| o.map(str::to_string))
                .collect(),
            claude_dir: String::new(),
            fallback_cwd: String::new(),
            already_in_tmux: false,
        }
    }

    /// 契约：ResumeSpec → CommandPlan 的 wire 形状 + camelCase + aterm 4 caps 名 + substitutedFrom。
    #[test]
    fn resolve_builds_plan_with_aterm_field_names() {
        let s = spec(
            "abcd1234-5678-90ab-cdef-1234567890ab",
            vec![None, Some("cct"), Some("cc")],
        );
        let plan = resolve(&s).expect("valid");
        // 序列化 → 校验 camelCase 键名（尤其 aterm 4 caps 名逐字对齐、免映射）。
        let v: Value = serde_json::from_str(&serde_json::to_string(&plan).unwrap()).unwrap();
        assert_eq!(v["mode"], "PtyInject");
        assert_eq!(
            v["command"],
            "cct --resume abcd1234-5678-90ab-cdef-1234567890ab"
        );
        assert_eq!(v["substitutedFrom"], "cct"); // 首个非空候选（None 跳过）
        assert_eq!(v["sessionName"], "cc-abcd1234"); // cc-<sid8>
        let caps = &v["capabilities"];
        assert!(
            caps.get("supportsSendKeys").is_some(),
            "aterm caps 名 supportsSendKeys"
        );
        assert!(caps.get("supportsCapture").is_some());
        assert!(caps.get("supportsMultiClient").is_some());
        assert!(caps.get("supportsMultiWindow").is_some());
        // 短名不得出现（否则两端要映射）。
        assert!(caps.get("sendKeys").is_none(), "不得用短名 sendKeys");
    }

    /// 无候选 → 默认 `claude`、substitutedFrom 省略（None → skip_serializing_if）。
    #[test]
    fn resolve_defaults_to_claude_when_no_candidates() {
        let s = spec("sid_123", vec![None, Some("   ")]); // 全空/空白 → 无可用
        let plan = resolve(&s).expect("valid");
        let v: Value = serde_json::from_str(&serde_json::to_string(&plan).unwrap()).unwrap();
        assert_eq!(v["command"], "claude --resume sid_123");
        assert!(
            v.get("substitutedFrom").is_none(),
            "无候选 → substitutedFrom 省略"
        );
    }

    /// B2：非法 sessionId（含 shell 元字符）→ 错误 {code,message}，不进 command（注入防线）。
    #[test]
    fn resolve_rejects_injection_in_session_id() {
        for bad in ["", "a b", "sid;rm -rf /", "$(whoami)", "a`b`", "x/y"] {
            let s = spec(bad, vec![Some("cc")]);
            let err = resolve(&s).expect_err("must reject");
            assert_eq!(err.0, "invalid_session_id", "拒 {bad:?}");
        }
        // 合法集通过。
        assert!(resolve(&spec("abc-DEF_123", vec![Some("cc")])).is_ok());
    }

    /// stdin 畸形 JSON 走 run() 的 bad_request（此处直接测解析失败路径的 shape）。
    #[test]
    fn malformed_resume_spec_is_bad_request() {
        let parsed: Result<ResumeSpec, _> = serde_json::from_str("{not json");
        assert!(parsed.is_err(), "畸形 JSON 解析失败 → run() 出 bad_request");
    }
}
