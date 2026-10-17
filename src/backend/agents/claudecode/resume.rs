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
        text: || copy_core::copy_text("beSessionNew.model.sonnet", &[]),
    },
    Choice {
        value: "opus",
        text: || copy_core::copy_text("beSessionNew.model.opus", &[]),
    },
    Choice {
        value: "haiku",
        text: || copy_core::copy_text("beSessionNew.model.haiku", &[]),
    },
    Choice {
        value: "sonnet[1m]",
        text: || copy_core::copy_text("beSessionNew.model.sonnetLong", &[]),
    },
    Choice {
        value: "opusplan",
        text: || copy_core::copy_text("beSessionNew.model.opusPlan", &[]),
    },
];

/// 换模型抽屉的「默认」那一项：`/model default` 回到那一家自己的缺省模型（官方别名表里的 `default`）。起会话不交它（不写 `--model` 就是默认）。
pub(crate) const MODEL_DEFAULT: Choice = Choice {
    value: "default",
    text: || copy_core::copy_text("beSessionNew.model.default", &[]),
};

/// 换模型顺带改了什么（抽屉底那一行）：`/model` 也把它存成这个号以后新会话的默认（读回那一句同样照实说）。
pub(crate) fn model_note() -> String {
    copy_core::copy_text("beSessionModel.note.savesDefault", &[])
}

/// `claude --permission-mode` 收的、起会话框给的档（不给越过批准那一档）。字与答批计划那一组同字。
pub(crate) const PERMISSION_MODES: &[Choice] = &[
    Choice {
        value: "default",
        text: || copy_core::copy_text("beAsk.choice.planManual", &[]),
    },
    Choice {
        value: "acceptEdits",
        text: || copy_core::copy_text("beAsk.choice.planEdits", &[]),
    },
    Choice {
        value: "plan",
        text: || copy_core::copy_text("beSessionNew.mode.plan", &[]),
    },
];

/// 会话会处在、起会话时不给选的档（记录里写的 · 屏底认得出的）：越过批准的两档 · `auto` · 旧名 `manual`（＝ `default`，同一句）。
pub(crate) const SHOWN_MODES: &[Choice] = &[
    Choice {
        value: "auto",
        text: || copy_core::copy_text("beAsk.choice.planAuto", &[]),
    },
    Choice {
        value: "bypassPermissions",
        text: || copy_core::copy_text("beSessionMode.mode.bypassPermissions", &[]),
    },
    Choice {
        value: "dontAsk",
        text: || copy_core::copy_text("beSessionMode.mode.dontAsk", &[]),
    },
    Choice {
        value: "manual",
        text: || copy_core::copy_text("beAsk.choice.planManual", &[]),
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

/// 记录里的模型原值（`claude-opus-4-5-20251101` · 旧式 `claude-3-5-sonnet-20241022`）⇒ 写给人看的名字（「Opus 4.5」）：
/// 系列名取起会话那张表（[`MODEL_ALIASES`]）里同名那一项的字，版本取原值里一两位的数段（八位的日期段不算）。
/// 不是 `claude-` 开头 · 认不出系列 · 没有版本段 ⇒ `None`（不猜）。
pub(crate) fn model_text(raw: &str) -> Option<String> {
    let (family, parts) = family_of(raw)?;
    let version: Vec<&str> = parts
        .iter()
        .copied()
        .filter(|p| (1..=2).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()))
        .collect();
    if version.is_empty() {
        return None;
    }
    Some(copy_core::copy_text(
        "beSessionModel.name.versioned",
        &[
            ("family", &(family.text)()),
            ("version", &version.join(".")),
        ],
    ))
}

/// 记录里的模型原值 ⇒ 系列（[`MODEL_ALIASES`] 里同名那一项）＋ 拆开的各段；不是 `claude-` 开头 · 认不出系列 ⇒ `None`。
fn family_of(raw: &str) -> Option<(&'static Choice, Vec<&str>)> {
    let parts: Vec<&str> = raw.strip_prefix("claude-")?.split('-').collect();
    let family = parts
        .iter()
        .find_map(|p| MODEL_ALIASES.iter().find(|c| c.value == *p))?;
    Some((family, parts))
}

/// 记录里的模型原值 ⇒ 别名表里**唯一**对得上的那一项（`claude-opus-4-1-20250805` ⇒ `opus`）：系列同 [`model_text`] 那样认；
/// 那个系列在表里不止一项（`sonnet` · `sonnet[1m]`：记录里分不出带没带长上下文）· 认不出 ⇒ `None`（不猜）。
pub(crate) fn model_pick(raw: &str) -> Option<&'static str> {
    let (family, _) = family_of(raw)?;
    let mut same = MODEL_ALIASES.iter().filter(|c| {
        c.value
            .strip_prefix(family.value)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('['))
    });
    match (same.next(), same.next()) {
        (Some(c), None) => Some(c.value),
        _ => None,
    }
}

/// 会话开着时换模型送的那一行：`/model <值>`（它也把这个模型存成这个号以后新会话的默认，那一家自己说的）。
pub(crate) fn model_command(value: &str) -> String {
    format!("/model {value}")
}

/// 本地命令输出那一对标签（`/model` 之后那一条记录里装着那一句）。
const LOCAL_STDOUT: (&str, &str) = ("<local-command-stdout>", "</local-command-stdout>");

/// 换模型读回：记录里一条本地命令输出，里面是 ``Set model to `X` …``（后面也许接 `` with `Y` effort``）或 ``Kept model as `X` ``。
/// 输出在顶层 `content`，或在 `message.content` 里（串）。认不出 ⇒ `None`（不猜）。
pub(crate) fn model_said(v: &serde_json::Value) -> Option<crate::agents::ModelSaid> {
    let text = v
        .get("content")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            v.pointer("/message/content")
                .and_then(serde_json::Value::as_str)
        })?;
    let inner = text
        .split_once(LOCAL_STDOUT.0)?
        .1
        .split_once(LOCAL_STDOUT.1)?
        .0;
    let quoted = |rest: &str| -> Option<(String, String)> {
        let (name, tail) = rest.strip_prefix('`')?.split_once('`')?;
        Some((name.to_string(), tail.to_string()))
    };
    let (kept, rest) = match (
        inner.strip_prefix("Set model to "),
        inner.strip_prefix("Kept model as "),
    ) {
        (Some(r), _) => (false, r),
        (_, Some(r)) => (true, r),
        _ => return None,
    };
    let (name, tail) = quoted(rest)?;
    let effort = tail
        .split_once(" with `")
        .and_then(|(_, e)| e.split_once("` effort"))
        .map(|(e, _)| e.to_string());
    Some(crate::agents::ModelSaid { name, kept, effort })
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/resume_tests.rs"]
mod tests;
