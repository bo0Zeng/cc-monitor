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
    /// DG3（#2D，additive）：会话属哪 agent kind → DG6 据此构 `codex resume <uuid>` vs `claude --resume`。
    /// camelCase（rename_all）→ wire `agentKind`。缺/`""`/`"claude"`=claude、`"codex"`=codex（DG6 消费）。
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

/// stdout 出参（camelCase 对齐 aterm `ResumePlan` + 加 mode/capabilities）。
///
/// # ★ E71：**这里面哪些是探测出来的、哪些是派生的**
///
/// 三个字段的可信度不一样，而字段名读起来一模一样 —— 消费方（已经有一个了）很容易
/// 把派生值当事实用。跨项目那份说明在 `doc/IPC-PROTOCOL.md` §10.1
///（**外部消费方不会读这份 Rust 源码**，所以那边才是正本）。
///
/// | 字段 | 可信度 |
/// |---|---|
/// | `command` | **可信**：调用方给的候选 + `--resume <调用方给的 sid>` |
/// | `session_name` | **派生**：纯从 sid 拼（`cc-<sid8>`），没读过 pidfile、没查过 tmux |
/// | `capabilities` | **典型档**：硬编码的常见组合，不是这台机器此刻的实测能力 |
///
/// 实现上留着痕迹：`run(_claude_dir, …)` 的参数带下划线 —— 它**手上有 `claude_dir` 却没用**。
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
    /// 用 `tmux_sessions` 帧（那是真 `tmux ls`）。
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

/// stdin 上限（审计 security-重要②）：ResumeSpec 极小，无界 `read_to_string` 遇超大 stdin →
/// 无界堆分配（Pi 级设备 OOM 风险）。`.take()` 兜底；超限 → 截断 → 后续 parse 失败 → bad_request。
const MAX_RESOLVE_STDIN: u64 = 1 << 20; // 1 MiB

/// `--resolve` 入口。stdin 读 ResumeSpec、stdout 写 CommandPlan、exit 0；出错 exit 2 + stderr JSON。
/// `_claude_dir` 现未用（MVP 不做 pidfile 消解）；留参数与其余 query::run 一致、后续联调用。
pub fn run(_claude_dir: &Path, _args: &[String]) -> i32 {
    let mut input = String::new();
    if let Err(e) = std::io::stdin()
        .take(MAX_RESOLVE_STDIN)
        .read_to_string(&mut input)
    {
        return emit_err("stdin_read_failed", format!("read stdin failed: {e}"));
    }
    match resolve_from_json(&input) {
        Ok(json) => {
            println!("{json}"); // stdout 一行 JSON（紧凑、无内嵌裸换行）
            0
        }
        Err((code, message)) => emit_err(code, message),
    }
}

/// U6b-3：给**流通道上的 `resolve` 命令**用的入口。
///
/// 与一次性 `--resolve` 复用**同一个** [`resolve_from_json`] —— 这是「吸收」的全部含义。
///
/// # 为什么不把一次性那条路搬走
///
/// 它的契约与仓外 aterm **冻结在 2026-07-18**（`daemon-协议-v1 §3`），且本文件头注写着
/// aterm 现走 β TailTransport、DaemonTransport 未建、**暂不消费 resolve**
/// —— 也就是说这条契约**随时可能开始被消费**。现在拆掉它是拿别人的集成期赌。
///
/// 两条路的差别只在信封：一次性那条是「1 exec = 1 请求 1 响应 1 退出、天然 1:1、无 request-id」；
/// 流通道那条有 `id`、可取消、不用为一次极小的 RPC 单开一整条 SSH exec。
pub fn resolve_json_for_inbound(input: &str) -> Result<serde_json::Value, (&'static str, String)> {
    let json = resolve_from_json(input)?;
    serde_json::from_str(&json)
        .map_err(|e| ("serialize_failed", format!("CommandPlan 回读失败：{e}")))
}

/// 纯：ResumeSpec JSON 串 → CommandPlan JSON 串（或 `(code,message)`）。`run()` 与单测共用——
/// 审计 quality-阻塞：让 stdin→响应 的分发逻辑（bad_request / serialize / happy）**可测**，
/// 不必真接 stdin/stdout（此前 `run()` 零覆盖、commit「端到端 smoke」实为手工一次性验证、无测件）。
fn resolve_from_json(input: &str) -> Result<String, (&'static str, String)> {
    let spec: ResumeSpec = serde_json::from_str(input.trim())
        .map_err(|e| ("bad_request", format!("ResumeSpec JSON parse failed: {e}")))?;
    let plan = resolve(&spec)?;
    serde_json::to_string(&plan).map_err(|e| {
        (
            "serialize_failed",
            format!("CommandPlan serialize failed: {e}"),
        )
    })
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
    // DG6：per-kind——`agent_kind=="codex"` → Codex resume 路（默认基底 `codex`、子命令 `resume`、
    // 会话名 `cx-`）；否则 Claude（默认 `claude`、`--resume` flag、`cc-`）。缺/`""`/其它 = claude。
    // **大小写敏感**：wire 值契约定死小写 `"codex"`（doc §agent_kind + 两端锁定）；`"Codex"` 等非规范
    // 值落 Claude 路（生产方 monitor/aterm 发规范小写故无碍）。`.trim()` 仅容空白、不容大小写。
    let is_codex = spec.agent_kind.trim() == "codex";
    // 首个非空 launchCandidate → command 基底；无 → kind 默认（Codex=codex / Claude=claude）。
    let candidate = spec
        .launch_candidates
        .iter()
        .flatten()
        .map(|s| s.trim())
        .find(|s| !s.is_empty());
    let base = match candidate {
        Some(c) => c.to_string(),
        None => if is_codex { "codex" } else { "claude" }.to_string(),
    };
    // 审计 security-重要①：B2 纪律**对称化**——`base` 同样进 command 串、由客户端 pty 执行，
    // 原只校验 sid、base 零校验（端到端两侧都没人查 base：客户端 B2 复校也只覆盖 sid）。补 base
    // 的 shell-safe 校验，兑现模块 doc 自称的 B2 注入防线（defense-in-depth：advisory 不执行 +
    // 同信任域下当前不可利用，但污染 ResumeSpec 会被洗成带 daemon 权威的可注入 CommandPlan）。
    if !is_shell_safe_base(&base) {
        return Err((
            "unsafe_launch_candidate",
            format!("launchCandidate 含 shell 元字符/控制字符、拒入 command：{base:?}"),
        ));
    }
    // command（sid 过 is_valid_session_id、base 过 is_shell_safe_base）。**golden-parity aterm
    // CodexInvocation.resumeInvocation**：Codex=`<base> resume <sid>`（子命令、**无 `--resume` flag、
    // 无 unset**、真机 `codex resume <SESSION_ID>` 核）；Claude=`<base> --resume <sid>`。
    let command = if is_codex {
        format!("{base} resume {}", spec.session_id)
    } else {
        format!("{base} --resume {}", spec.session_id)
    };
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
        session_name: Some(session_name_for(&spec.session_id, is_codex)), // Codex cx- / Claude cc-
        launch_label: None, // MVP 不产 label（aterm 侧自算）
        // substitutedFrom：aterm 语义（`TmuxBackend.resume` 核实，2026-07-18 回）=「被替换掉的原命令」
        // = `intended?.takeIf { it != launch }`——仅当解析出的 launch ≠ 用户原意首候选时非空。MVP daemon
        // 不做「解析可能异于原意」的候选消解（直接用首候选/默认，launch==intended），恒无替换 → None（省略）。
        // 待 daemon 有真候选消解（候选不可用回退 / post-/branch sid 变更）再填原值。**修正 daemon-04 此前
        // 误设为「被用候选」**（反了 aterm 语义、与 command 冗余；审计 flag、aterm 2026-07-18 确认语义）。
        substituted_from: None,
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

/// base（launchCandidate 或默认 `claude`）会进 `command` 串、由客户端 pty 执行 → 拒 shell 注入。
/// 允许 launcher 常见形（字母数字/空格/`- _ . / = : ,`，支持带路径与 flag），拒 shell 元字符
/// `; | & $ ` ( ) < > \ " ' * ? { } !`、换行/控制字符（0x00–0x1f、0x7f）。defense-in-depth——
/// daemon advisory 不执行，但不应产出一份可注入的 CommandPlan（客户端 pty-inject 它）。
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

/// resume 会话名：Claude `cc-<sid8>` / **Codex `cx-<sid8>`**（前 8 字符；不足 8 取全部）。
/// golden-parity aterm（Claude `cc-`、Codex CodexInvocation.resumeSessionName `cx-`）。客户端亦自算、daemon 顺带给。
fn session_name_for(sid: &str, is_codex: bool) -> String {
    let head: String = sid.chars().take(8).collect();
    let prefix = if is_codex { "cx" } else { "cc" };
    format!("{prefix}-{head}")
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
            agent_kind: String::new(),
        }
    }

    /// 契约：ResumeSpec → CommandPlan 的 wire 形状 + camelCase + aterm 4 caps 名 + substitutedFrom。
    /// DG3（#2D）：ResumeSpec 的 `agent_kind` 从 **camelCase** wire `agentKind` 反序列化（resolve I/O 面
    /// 全 camelCase）；缺省 → `""`（= claude 兼容）。DG6 据此构 codex resume vs claude --resume。
    #[test]
    fn resume_spec_parses_camelcase_agent_kind_default_claude() {
        let spec: ResumeSpec =
            serde_json::from_str(r#"{"sessionId":"s","agentKind":"codex"}"#).expect("parse");
        assert_eq!(spec.agent_kind, "codex");
        // 缺 agentKind → default ""（缺=claude，向后兼容旧 monitor 不发此字段）。
        let spec2: ResumeSpec = serde_json::from_str(r#"{"sessionId":"s"}"#).expect("parse");
        assert_eq!(spec2.agent_kind, "");
    }

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
        // substitutedFrom：MVP 恒省略——daemon 不做候选消解、无「被替换的原命令」（对齐 aterm
        // 语义：仅解析 launch≠原意首候选时才有；见 resolve() 注 + aterm 2026-07-18 确认）。
        assert!(
            v.get("substitutedFrom").is_none(),
            "MVP 无候选消解 → substitutedFrom 省略（不再误设为被用候选）"
        );
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

    /// DG6：agent_kind="codex" → `<base> resume <uuid>`（**子命令、无 --resume**）+ 会话名 `cx-`；
    /// 默认基底 `codex`、自定义候选照用。golden-parity aterm CodexInvocation.resumeInvocation/resumeSessionName。
    #[test]
    fn resolve_codex_builds_resume_subcommand_and_cx_name() {
        let uuid = "019f75dd-875c-7c81-9eda-32f866b2c60f";
        // 无候选 → 默认 codex。
        let s = ResumeSpec {
            agent_kind: "codex".into(),
            ..spec(uuid, vec![])
        };
        let v: Value =
            serde_json::from_str(&serde_json::to_string(&resolve(&s).expect("valid")).unwrap())
                .unwrap();
        assert_eq!(v["command"], format!("codex resume {uuid}"));
        assert_eq!(v["sessionName"], "cx-019f75dd"); // cx-<sid8>（非 cc-）
        assert_eq!(v["mode"], "PtyInject");
        // 自定义 codex 启动候选 → `<base> resume <sid>`。
        let s2 = ResumeSpec {
            agent_kind: "codex".into(),
            ..spec(uuid, vec![Some("mycodex")])
        };
        let v2: Value =
            serde_json::from_str(&serde_json::to_string(&resolve(&s2).expect("valid")).unwrap())
                .unwrap();
        assert_eq!(v2["command"], format!("mycodex resume {uuid}"));
    }

    /// DG6 审计补：非规范 agent_kind（大小写/其它值）→ **落 Claude 路**（`--resume`/`cc-`），
    /// 防 Codex 分支误吞。契约：wire 值定死小写 `"codex"`（大小写敏感）。
    #[test]
    fn resolve_non_codex_agent_kind_falls_back_to_claude() {
        for ak in ["", "claude", "Codex", "CODEX", "foo"] {
            let s = ResumeSpec {
                agent_kind: ak.to_string(),
                ..spec("sid_x", vec![Some("claude")])
            };
            let v: Value =
                serde_json::from_str(&serde_json::to_string(&resolve(&s).expect("valid")).unwrap())
                    .unwrap();
            assert_eq!(
                v["command"], "claude --resume sid_x",
                "agent_kind={ak:?} 应落 Claude"
            );
            assert_eq!(v["sessionName"], "cc-sid_x");
        }
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

    /// 审计 security①：B2 对称化——launchCandidate（base）含 shell 元字符 → unsafe_launch_candidate，
    /// 不进 command（此前 base 零校验、注入透传：`["cc; rm -rf /"]`→`cc; rm -rf / --resume …`）。
    #[test]
    fn resolve_rejects_injection_in_launch_candidate() {
        for bad in [
            "cc; rm -rf /",
            "a$(id)",
            "x`whoami`",
            "a|b",
            "a>b",
            "a\nb",
            "a&b",
        ] {
            let s = spec("abc123", vec![Some(bad)]);
            let err = resolve(&s).expect_err("must reject base");
            assert_eq!(err.0, "unsafe_launch_candidate", "拒 base {bad:?}");
        }
        // 合法 launcher（带 flag/路径/等号）通过。
        assert!(resolve(&spec("abc123", vec![Some("/usr/bin/cc --foo=bar")])).is_ok());
    }

    /// 审计 quality-阻塞：`resolve_from_json`（= `run()` 的分发核）真覆盖 happy 路径——
    /// stdin JSON 串 → CommandPlan JSON 串（含 aterm caps 名）。此前 `run()` 零覆盖。
    #[test]
    fn resolve_from_json_happy_returns_command_plan_json() {
        let out = resolve_from_json(
            r#"{"sessionId":"abcd1234-ef","launchCandidates":["cc"],"claudeDir":"/c","fallbackCwd":"/t","alreadyInTmux":false}"#,
        )
        .expect("ok");
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["command"], "cc --resume abcd1234-ef");
        assert_eq!(v["capabilities"]["supportsSendKeys"], true);
    }

    /// 审计 quality-阻塞：`run()` 的 bad_request 分支真覆盖（此前只测 `serde_json::from_str` 本身、
    /// 不走分发）。畸形 JSON 经 `resolve_from_json` → `(bad_request, _)`。
    #[test]
    fn resolve_from_json_malformed_is_bad_request() {
        let err = resolve_from_json("{not json").expect_err("must err");
        assert_eq!(err.0, "bad_request");
    }

    /// 分发核对非法 sid / 不安全 base 同样短路成对应错误码（端到端错误 taxonomy）。
    #[test]
    fn resolve_from_json_propagates_validation_errors() {
        assert_eq!(
            resolve_from_json(r#"{"sessionId":"a;b"}"#).unwrap_err().0,
            "invalid_session_id"
        );
        assert_eq!(
            resolve_from_json(r#"{"sessionId":"ok1","launchCandidates":["c;d"]}"#)
                .unwrap_err()
                .0,
            "unsafe_launch_candidate"
        );
    }
}
