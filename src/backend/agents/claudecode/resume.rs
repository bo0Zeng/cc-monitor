//! Claude 的 **resume 调用形状** —— 与 [`crate::agents::codex::resume`] 严格对称。
//!
//! 两边摆在一起才看得出差别（这也是 `D4` 要求"接口由现有能力反推"的用意）：
//!
//! | | Claude | Codex |
//! |---|---|---|
//! | 默认命令 | `claude` | `codex` |
//! | resume 形 | `<base> --resume <sid>`（**flag**） | `<base> resume <sid>`（**子命令**） |
//! | 会话名前缀 | `cc-` | `cx-` |
//!
//! golden-parity 手机端。⚠ 两边**只有这三件事不同** —— sid 校验、base 的 shell-safe 校验、
//! `CommandPlan` 骨架、错误出口全都共用，留在 `control/resolve_query.rs`。

use crate::agents::Choice;

/// 无 `launchCandidate` 时的默认命令基底。
pub(crate) const DEFAULT_COMMAND: &str = "claude";

/// 用户的 shell 集成 wrapper（含代理 / env）：探得到就先用它，探不到回退 [`DEFAULT_COMMAND`]。
pub(crate) const LAUNCHER_ALIAS: &str = "cc";

/// resume 那个字面量：**flag 形**（`--` 开头）。[`resume_command`] 与画像表（`agents::LaunchFace`）同一份。
pub(crate) const RESUME_TOKEN: &str = "--resume";
/// 起新会话时先定好 sid 的旗标（`claude --session-id <uuid>`；起会话框带规则时后端按它先写好来源）。
pub(crate) const SESSION_ID_FLAG: &str = "--session-id";

/// 起会话时选权限档的旗标（`claude --permission-mode <档>`）。
pub(crate) const PERMISSION_MODE_FLAG: &str = "--permission-mode";
/// 会话级设置那个旗标（`--settings <文件或 JSON 串>`：hooks 与各层设置合并、不替换用户自己的；答题口经它挂 hook，`asks.rs`）。
pub(crate) const SETTINGS_FLAG: &str = "--settings";

/// claude 导给它起的每个子进程的「我是哪个会话」（会话血缘：`ccm` 在一个会话的 shell 里被调用时读它当父）。也是嵌套标记之一。
pub(crate) const SELF_SID_ENV: &str = "CLAUDE_CODE_SESSION_ID";

/// 起会话 / resume 之前要清掉的嵌套会话标记（否则 claude 自认嵌套子会话、不注册 pidfile、不写 jsonl）。
/// ⚠ **顺序不是随手排的**：载荷按这个序 unset（`launch_render`），它直接决定送到那台的那条命令的字节。
pub(crate) const NESTED_ENV: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    SELF_SID_ENV,
    "CLAUDE_CODE_CHILD_SESSION",
];

/// resume 会话名前缀（Codex 是 `cx`）。
pub(crate) const SESSION_NAME_PREFIX: &str = "cc";

/// `claude --model` 认得的别名与字（不写 ＝ 它自己的默认）。全名（`claude-sonnet-…`）也收，这里只列别名。
pub(crate) const MODEL_ALIASES: &[Choice] = &[
    Choice {
        value: "sonnet",
        text: "beSessionNew.model.sonnet",
    },
    Choice {
        value: "opus",
        text: "beSessionNew.model.opus",
    },
    Choice {
        value: "haiku",
        text: "beSessionNew.model.haiku",
    },
    Choice {
        value: "sonnet[1m]",
        text: "beSessionNew.model.sonnetLong",
    },
    Choice {
        value: "opusplan",
        text: "beSessionNew.model.opusPlan",
    },
];

/// `claude --permission-mode` 收的、起会话框给的档（不给越过批准那一档）。字与答批计划那一组同字。
pub(crate) const PERMISSION_MODES: &[Choice] = &[
    Choice {
        value: "default",
        text: "beAsk.choice.planManual",
    },
    Choice {
        value: "acceptEdits",
        text: "beAsk.choice.planEdits",
    },
    Choice {
        value: "plan",
        text: "beSessionNew.mode.plan",
    },
];

/// Shift+Tab 那一圈最多几档：`default → acceptEdits → plan`，开了的 `bypassPermissions` · `auto` 接在 `plan` 后面。
pub(crate) const MODE_CYCLE_MAX: usize = 5;

/// 屏底那一行写的档（官方文档：`⏸ manual mode on` · `⏵⏵ accept edits on` · `⏸ plan mode on` · `⏵⏵ auto mode on`；越过批准那一档写 `bypass permissions on`）。
const MODE_MARKS: &[(&str, &str)] = &[
    ("accept edits on", "acceptEdits"),
    ("plan mode on", "plan"),
    ("auto mode on", "auto"),
    ("bypass permissions on", "bypassPermissions"),
    ("manual mode on", "default"),
];

/// 从此刻那一屏认档：自下而上找第一行带档标的（屏底那一行在最下面；正文里引了同样的字也盖不过它）。认不出 ⇒ `None`（不按「没有标就是默认」猜）。
pub(crate) fn mode_on_screen(lines: &[String]) -> Option<&'static str> {
    lines.iter().rev().find_map(|l| {
        let l = l.to_ascii_lowercase();
        MODE_MARKS
            .iter()
            .find(|(mark, _)| l.contains(mark))
            .map(|(_, mode)| *mode)
    })
}

/// resume 命令：**flag 形**，与 Codex 的子命令形不同。
pub(crate) fn resume_command(base: &str, session_id: &str) -> String {
    format!("{base} {RESUME_TOKEN} {session_id}")
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/resume_tests.rs"]
mod tests;
