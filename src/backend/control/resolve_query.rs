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
//! **advisory not owning（§5④）**：只返命令串、backend 零 handle、绝不执行后端。
//! **B2 纪律**：backend 是权威也**保留本地 `is_valid_session_id` 校验**（对后端自己产出的 plan
//! 也过一遍——sessionId 会进 command 串，注入防线）。
//!
//! ★ **MVP 范围**（aterm 现走 β TailTransport、DaemonTransport 未建、**暂不消费 resolve**）：本轮锁
//! **wire 信封**（stdin/stdout/错误/字段名）。command 构建 = 首个可用 `launchCandidate` + `--resume
//! <sid>`（无候选→默认 `claude`），`substitutedFrom` 记来源——合理 MVP 默认；pidfile-based sid 消解
//! （post-/branch 正确 sid + kind，backend 深层权威）与 aterm `ResumePlan` 模板精确对齐**留 aterm 接
//! DaemonTransport 时联调**（那时才真消费）。caps 用 tmux/pty 典型档、待后续 backend 探测细化。

use copy_core::copy_text;
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
    /// ⚠ **冻结兼容字段，不许改名**—— 与 `hello` 帧的 `claude_dir` 同族。
    ///
    /// 它是 `--resolve` 的 **stdin 契约**（`rename_all = "camelCase"` ⇒ 线上是 `claudeDir`），
    /// 与仓外 aterm **冻结在 2026-07-18**。Rust 侧标识符改名 = 线上字段名改名
    ///（除非再挂一条 `serde(rename)` —— 那是把一条契约拆成两个源头），
    /// 所以 `S4b` 那轮「通用层标识符去 agent 名」**绕开它**，登记在
    /// `agent_locality_guard::AGENT_NAMED_WIRE_FIELDS`（带解锁条件）。
    ///
    /// ★ `S4` 只盘了 `wire.rs`，**没看见这一条** —— 因为 `S1` 的判据只扫
    /// `CORE_FILES`，而 `control/resolve_query.rs` 不在表里。`S4b` 把它补登记了。
    #[allow(dead_code)] // MVP 未用（backend 用自身 agent_home 做 pidfile 查，留字段兼容）
    #[serde(default)]
    claude_dir: String,
    #[allow(dead_code)] // MVP 未用（both-down→local 回退是客户端侧决策，见 §3 三态）
    #[serde(default)]
    fallback_cwd: String,
    #[allow(dead_code)] // MVP 未据此分支（PtyInject 对 alreadyInTmux 与否一致，留字段兼容）
    #[serde(default)]
    already_in_tmux: bool,
    /// DG3（#2D，additive）：会话属哪 agent kind → DG6 据此构 `codex resume <uuid>` vs `claude --resume`。
    /// camelCase（rename_all）→ wire `agentKind`。注册表里的一家 = 那一家；缺/`""` = 默认那一家；别的名字 ⇒ `bad_request`。
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
/// 把派生值当事实用。跨项目那份说明在 `src/doc/IPC-PROTOCOL.md` §10.1
///（**外部消费方不会读这份 Rust 源码**，所以那边才是正本）。
///
/// | 字段 | 可信度 |
/// |---|---|
/// | `command` | **可信**：调用方给的候选 + `--resume <调用方给的 sid>` |
/// | `session_name` | **派生**：纯从 sid 拼（`cc-<sid8>`），没读过 pidfile、没查过 tmux |
/// | `capabilities` | **典型档**：硬编码的常见组合，不是这台机器此刻的实测能力 |
///
/// 实现上留着痕迹：`run(_agent_home, …)` 的参数带下划线 —— 它**手上有 home 目录却没用**
///（入参 `ResumeSpec.claude_dir` 也一样标着 `#[allow(dead_code)]`）。
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

/// stdin 上限（审计 security-重要②）：ResumeSpec 极小，无界 `read_to_string` 遇超大 stdin →
/// 无界堆分配（Pi 级设备 OOM 风险）。`.take()` 兜底；超限 → 截断 → 后续 parse 失败 → bad_request。
const MAX_RESOLVE_STDIN: u64 = 1 << 20; // 1 MiB

/// `--resolve` 入口。stdin 读 ResumeSpec、stdout 写 CommandPlan、exit 0；出错 exit 2 + stderr JSON。
/// `_agent_home` 现未用（MVP 不做 pidfile 消解）；留参数与其余 query::run 一致、后续联调用。
pub fn run(_agent_home: &Path, _args: &[String]) -> i32 {
    let mut input = String::new();
    if let Err(e) = std::io::stdin()
        .take(MAX_RESOLVE_STDIN)
        .read_to_string(&mut input)
    {
        return emit_err("stdin_read_failed", format!("read stdin failed: {e}"));
    }
    match resolve_from_json(crate::agents::REGISTRY, &input) {
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

/// 纯：ResumeSpec JSON 串 → CommandPlan JSON 串（或 `(code,message)`）。`run()` 与单测共用——
/// 审计 quality-阻塞：让 stdin→响应 的分发逻辑（bad_request / serialize / happy）**可测**，
/// 不必真接 stdin/stdout（此前 `run()` 零覆盖、commit「端到端 smoke」实为手工一次性验证、无测件）。
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
    // B2 纪律：sessionId 会进 command 串 → 先过本地校验（注入防线，backend 自产也过）。
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
    // 审计 security-重要①：B2 纪律**对称化**——`base` 同样进 command 串、由客户端 pty 执行，
    // 原只校验 sid、base 零校验（端到端两侧都没人查 base：客户端 B2 复校也只覆盖 sid）。补 base
    // 的 shell-safe 校验，兑现模块 doc 自称的 B2 注入防线（defense-in-depth：advisory 不执行 +
    // 同信任域下当前不可利用，但污染 ResumeSpec 会被洗成带后端权威的可注入 CommandPlan）。
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
        // substitutedFrom：aterm 语义（`TmuxBackend.resume` 核实，2026-07-18 回）=「被替换掉的原命令」
        // = `intended?.takeIf { it != launch }`——仅当解析出的 launch ≠ 用户原意首候选时非空。MVP backend
        // 不做「解析可能异于原意」的候选消解（直接用首候选/默认，launch==intended），恒无替换 → None（省略）。
        // 待后端有真候选消解（候选不可用回退 / post-/branch sid 变更）再填原值。**修正 backend-04 此前
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
/// backend advisory 不执行，但不应产出一份可注入的 CommandPlan（客户端 pty-inject 它）。
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
/// —— 它是给 aterm 的跨仓承诺的一部分（`IPC-PROTOCOL §10`「`resolve`」跨仓承诺小节），
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
fn emit_err(code: &'static str, message: String) -> i32 {
    eprintln!("{}", error_envelope(code, message));
    ERROR_EXIT
}

#[cfg(test)]
#[path = "../../../tests/backend/control/resolve_query_tests.rs"]
mod tests;
