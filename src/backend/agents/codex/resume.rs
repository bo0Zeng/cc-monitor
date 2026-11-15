//! Codex 的起会话 / resume 事实：默认命令叫什么、resume 那个词怎么写、起它时垫什么参数；
//! 权限档与模型怎么交、模型清单怎么读，会话开着时换档 · 换模型 · 压缩 · 清空投的那一句（形照 codex-rs rust-v0.167.0）。
//! 起会话 / resume 那一行由 `ccm` 按适配层这几格拼（`agents::LaunchFace`）：resume 是**子命令形**（`codex resume <sid>`），与 Claude 的 `--resume` flag 不同。
//!
//! 通用层里一个 agent 名字面量都不许有（`agent_locality_guard` 判据②）—— 谁想在别处再开一个 Codex 分支，得先写出 `"codex"`，当场红。

use crate::agents::{Choice, ListedModel, ModelList, ModelSwitch, PermissionMode};

/// 默认启动器。
pub const DEFAULT_COMMAND: &str = "codex";

/// 这一家的 wrapper 名：号别名清单的词干（每个号出 `<号>cx` · `<号>cxt` · `<号>cxh`），不跟 Claude 那一家的 `cc` 撞。
pub(crate) const LAUNCHER_ALIAS: &str = "cx";

/// resume 那个字面量：**子命令形**（不以 `--` 开头）。画像表（`agents::LaunchFace`）读它。
pub const RESUME_TOKEN: &str = "resume";

/// `ccm` 起它（新起与 resume）一律不连共享后台：共享后台沿用第一个拉起它的进程的环境，
/// 这一趟给的账号与中转地址可能不生效，也认不出哪个窗格是哪个会话。
pub(crate) const LAUNCH_ARGS: &[&str] = &["--no-daemon"];

/// Codex 导给它起的每个子 shell 的「我是哪个会话」：这一条线程的号（resume 认的就是它；codex-rs rust-v0.167.0
/// `codex_protocol::shell_environment::CODEX_THREAD_ID_ENV_VAR`，`codex_protocol::shell_environment::create_env` 一律补上）。
/// 会话血缘：`ccm` 在一个 Codex 会话的 shell 里被调用时读它当父。
/// 不拿根会话号（`CODEX_SESSION_ID`）认：子 agent 线程共用它。也是嵌套标记之一。
pub(crate) const SELF_SID_ENV: &str = "CODEX_THREAD_ID";

/// 起会话 / resume 之前要清掉的嵌套会话标记：Codex 给它起的子 shell 导的那几个（codex-rs rust-v0.167.0）——
/// 这一条线程的号（[`SELF_SID_ENV`]）· 根会话号（`codex_protocol::shell_environment::CODEX_SESSION_ID_ENV_VAR`，
/// 由 `codex_core::exec_env::inject_session_env` 写进去）· 这一次工具调用的号（`codex_protocol::shell_environment::CODEX_TOOL_CALL_ID_ENV_VAR`）·
/// 它自己的版本（`codex_core::exec_env::CODEX_VERSION_ENV_VAR`，同在 `codex_core::exec_env::inject_session_env`）。
/// 顺序决定载荷字节（载荷按这个序 unset）。
pub(crate) const NESTED_ENV: &[&str] = &[
    SELF_SID_ENV,
    "CODEX_SESSION_ID",
    "CODEX_TOOL_CALL_ID",
    "CODEX_VERSION",
];

/// 请求压缩上下文那一句（注册表 `Adapter.compact_request` 那一格）。
pub const COMPACT_REQUEST: &str = "/compact";

/// 清空会话那一句（注册表 `Adapter.clear_request` 那一格）：Codex 的 `/clear` 清屏并开一条新对话。
pub const CLEAR_REQUEST: &str = "/clear";

/// 权限档的原值：内置档名（`codex_protocol::models::BUILT_IN_PERMISSION_PROFILE_*`，`/permissions` 认的就是它）。
const READ_ONLY: &str = ":read-only";
const WORKSPACE: &str = ":workspace";
const DANGER_FULL_ACCESS: &str = ":danger-full-access";

/// 可选的档：档名 ⇒ 起会话那一对，形恒是 `[-a, <批准策略>, -s, <沙箱>]`（照 `utils/approval-presets` 的内置那几档：只读 · 工作区内可写都是 on-request；
/// `codex resume` 收同一组旗标）。会话中途换档那一句的策略就取这一组的第二个（[`permission_command`]）。
pub(crate) const PERMISSION_MODES: &[PermissionMode] = &[
    PermissionMode {
        value: READ_ONLY,
        text: || copy_core::copy_text("beSessionNew.mode.readOnly", &[]),
        args: &["-a", "on-request", "-s", "read-only"],
    },
    PermissionMode {
        value: WORKSPACE,
        text: || copy_core::copy_text("beSessionNew.mode.workspace", &[]),
        args: &["-a", "on-request", "-s", "workspace-write"],
    },
];

/// 会话会处在、不给选的档：越过批准、不设沙箱的那一档（`-a never -s danger-full-access`）。
pub(crate) const SHOWN_MODES: &[Choice] = &[Choice {
    value: DANGER_FULL_ACCESS,
    text: || copy_core::copy_text("beSessionMode.mode.dangerFullAccess", &[]),
}];

/// 会话开着时换到 `mode` 那一档投的那一句（`ModeSwitch::Command`）：`/permissions <档名> --approval-policy=<策略>`，
/// 策略是 [`PERMISSION_MODES`] 里那一档 `[-a, 策略, -s, 沙箱]` 的第二个（判据逐档钉着这一形）。通用层只拿可选的档问它（`PermissionFace::switch_line`）。
pub(crate) fn permission_command(mode: &str) -> String {
    match PERMISSION_MODES
        .iter()
        .find(|m| m.value == mode)
        .map(|m| m.args)
    {
        Some(["-a", policy, "-s", _]) => format!("/permissions {mode} --approval-policy={policy}"),
        _ => format!("/permissions {mode}"),
    }
}

/// 起会话选了 `model` 交给 codex 的那两个参数（`-m <模型>`）；选了推理档 ⇒ 再接 `-c model_reasoning_effort=<档>`
/// （这一趟盖 `config.toml` 的同名键，不写盘）。
pub(crate) fn model_args(model: &str, effort: Option<&str>) -> Vec<String> {
    let mut out = vec!["-m".to_string(), model.to_string()];
    if let Some(e) = effort {
        out.extend(["-c".to_string(), format!("model_reasoning_effort={e}")]);
    }
    out
}

/// 列模型清单那一条（接在启动器后面）：`codex debug models --bundled` 把这一版自带的模型目录打成一行 JSON（带它不联网、不用登录）。
pub(crate) const MODEL_LIST: ModelList = ModelList {
    args: &["debug", "models", "--bundled"],
    read: models_of,
};

/// [`MODEL_LIST`] 的输出 ⇒ 模型清单（`ModelsResponse`：`{"models":[…]}`）：只列 `visibility = list` 的（`/model <模型>` 只认这几个），
/// 照 `priority` 升序（它的选择框就这么排；缺 ⇒ 排在后面），各带 `supported_reasoning_levels[].effort`（原序）与 `default_reasoning_level`（可缺）。
/// 不是这一形 ⇒ `None`；缺 `slug` 的那一项不出，缺 `display_name` 的字照 slug。
fn models_of(stdout: &str) -> Option<Vec<ListedModel>> {
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).ok()?;
    let str_of =
        |m: &serde_json::Value, k: &str| m.get(k).and_then(|x| x.as_str()).map(str::to_string);
    let mut listed: Vec<(i64, ListedModel)> = v
        .get("models")?
        .as_array()?
        .iter()
        .filter(|m| m.get("visibility").and_then(|x| x.as_str()) == Some("list"))
        .filter_map(|m| {
            let value = str_of(m, "slug")?;
            let model = ListedModel {
                text: str_of(m, "display_name").unwrap_or_else(|| value.clone()),
                efforts: m
                    .get("supported_reasoning_levels")
                    .and_then(|x| x.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|l| str_of(l, "effort"))
                    .collect(),
                default_effort: str_of(m, "default_reasoning_level"),
                value,
            };
            let priority = m
                .get("priority")
                .and_then(|x| x.as_i64())
                .unwrap_or(i64::MAX);
            Some((priority, model))
        })
        .collect();
    listed.sort_by_key(|(p, _)| *p);
    Some(listed.into_iter().map(|(_, m)| m).collect())
}

/// 会话开着时换模型投的那一句：`/model <模型>`，给了推理档 ⇒ `/model <模型> --reasoning=<档>`（不去点选择框）。
fn model_command(model: &str, effort: Option<&str>) -> String {
    match effort {
        Some(e) => format!("/model {model} --reasoning={e}"),
        None => format!("/model {model}"),
    }
}

/// 换模型（注册表 `LaunchFace.model_switch` 那一格）：只投那一句。没有读回：Codex 换模型当场不往 rollout 写记录
/// （新模型要到下一轮开头的 `turn_context` 才出现）⇒ 结果照「已发出」交。不顺带写设置、不弹确认框。
pub(crate) const MODEL_SWITCH: ModelSwitch = ModelSwitch {
    command: model_command,
    read: None,
    saves: None,
    note: None,
    confirm: None,
};

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/resume_tests.rs"]
mod tests;
