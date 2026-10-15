//! 待答请求的**成品**（会话 `needs.ask`）与它怎么写字：通用、不认是哪一家（那一家的 hook 输入由各家适配层翻成 [`super::AskIn`]）。
//! 住 `agents/` 这一层（同过程里那张提问卡 [`super::StepAsk`]）：登记表（`control::asks`）与会话事实（`observe::facts_query` 的 `needs`）都要它，
//! 而那两层彼此不引。

use super::{AskIn, AskKind};
pub(crate) use cmd_wire::needs::{
    Ask, AskRun, Choice, ChoiceRow, NoteNeed, Question, QuestionOption,
};

/// 会话级答题 hook 叫 ccm 的那个子命令（`control::asks::hook`；那一家的 hook 设置里写的就是它）。
pub const ANSWER_HOOK_FLAG: &str = "--answer-hook";
/// 那一家的 hook 叫 ccm 的那几个词（打头的 `--`：后面是后端的词），后面再接那一家的 kind。
pub const ANSWER_HOOK_WORDS: [&str; 2] = ["--", ANSWER_HOOK_FLAG];

/// ccm 起会话时**挂不上**答题 hook（用户自己给的会话级设置读不出 / 不是那一家认的形）⇒ 起的那个进程环境里带这个变量；
/// 会话事实读那个进程的环境认出「这条会话只能在终端里答批准」（`answerOff`）。
pub(crate) const ANSWER_HOOK_OFF_ENV: &str = "CCM_ANSWER_HOOK_OFF";

/// 答题 hook 为什么没挂上（ccm 起会话时定，经 [`ANSWER_HOOK_OFF_ENV`] 的值带到那个进程里，会话事实读回来写那一句）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HookOff {
    /// 用户自带的会话级设置（`--settings`）并不进答题 hook：为什么见 [`SettingsWhy`]。
    Settings(SettingsWhy),
    /// 会话由自定义启动器起（不是这一家默认的那一个），而经环境交插件那一路这一家没有 / 这台判不了子进程与接没接上（读不出别的进程的环境）。
    Launcher,
    /// 换了启动器，本机 managed 策略不许加载会话插件（`disableSideloadFlags` · `allowManagedHooksOnly`）。
    Policy,
    /// 换了启动器，装着的那一家版本低于认插件目录变量的那一版（或认不出版本）。
    Version,
    /// 换了启动器、ccm 经环境交了插件，持着会话的进程环境里却没有它：启动器没把环境传下去（会话事实读出来的，ccm 不写这个值）。
    EnvLost,
    /// 会话不是 ccm 起的：持着它的进程身上看不出挂过答题口（[`started_elsewhere`]；会话事实读出来的，ccm 不写这个值）。
    Elsewhere,
}

/// 用户自带的会话级设置并不进去的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsWhy {
    /// 读不出：文件不在 / 打不开，或旗标后面没给值。
    Unread,
    /// 读出来不是 JSON。
    NotJson,
    /// 是 JSON，但不是那一家认的会话级设置的形状（并不进去）。
    Shape,
}

impl HookOff {
    /// 写进那个进程环境的值（ASCII 短码；会话事实按它认原因）。
    pub(crate) fn env_value(self) -> &'static str {
        match self {
            HookOff::Settings(SettingsWhy::Unread) => "settings-unread",
            HookOff::Settings(SettingsWhy::NotJson) => "settings-json",
            HookOff::Settings(SettingsWhy::Shape) => "settings-shape",
            HookOff::Launcher => "launcher",
            HookOff::Policy => "policy",
            HookOff::Version => "version",
            HookOff::EnvLost => "env-lost",
            HookOff::Elsewhere => "elsewhere",
        }
    }

    /// 环境里读回来的值 ⇒ 原因；认不出 ⇒ `None`（只说没接上，不编原因）。
    pub(crate) fn from_env_value(v: &str) -> Option<HookOff> {
        [
            HookOff::Settings(SettingsWhy::Unread),
            HookOff::Settings(SettingsWhy::NotJson),
            HookOff::Settings(SettingsWhy::Shape),
            HookOff::Launcher,
            HookOff::Policy,
            HookOff::Version,
            HookOff::EnvLost,
            HookOff::Elsewhere,
        ]
        .into_iter()
        .find(|o| o.env_value() == v)
    }
}

/// 我们那个答题插件目录（`<家>/.cc-monitor/…`，绝对路径；ccm 起会话那一下写进去，起会话 · hook · 会话事实都按这一处算）。
pub(crate) fn plugin_dir_under(home: &str) -> String {
    let mut p = std::path::PathBuf::from(home);
    for seg in relay_route_core::ANSWER_PLUGIN_DIR_REL.split('/') {
        p.push(seg);
    }
    p.to_string_lossy().into_owned()
}

/// 插件目录清单的分隔符（这台的路径清单分隔符；那一家照 `--plugin-dir` 那样一项一项加载）。
const DIRS_SEP: char = if cfg!(windows) { ';' } else { ':' };

/// 一串插件目录清单里有没有 `dir`。
pub(crate) fn dirs_have(list: &str, dir: &str) -> bool {
    list.split(DIRS_SEP).any(|p| p == dir)
}

/// 用户已有的清单后面接上 `dir`（已有就不重复加）。
pub(crate) fn dirs_with(list: Option<&str>, dir: &str) -> String {
    match list.filter(|l| !l.is_empty()) {
        None => dir.to_string(),
        Some(l) if dirs_have(l, dir) => l.to_string(),
        Some(l) => format!("{l}{DIRS_SEP}{dir}"),
    }
}

/// 持着会话的那个进程（环境里没有挂不上的那个变量）是不是别处起的（纯函数）：那一家有答题口，而它身上一样 ccm 挂过的痕迹都没有 ——
/// 命令行里没有 hook 客户端那个旗标（`--settings` 那一路垫进去的）· 插件目录变量里没有我们那个目录（换了启动器那一路）· ccm 没给它留过便条。
/// 命令行读不出（非 Linux · 进程刚没了）⇒ 不下这个判断（`false`：只说只能在终端里答，不编原因）。
pub(crate) fn started_elsewhere(
    has_ask_face: bool,
    cmdline: Option<&str>,
    plugin_here: bool,
    noted: bool,
) -> bool {
    let Some(cmdline) = cmdline else {
        return false;
    };
    has_ask_face
        && !plugin_here
        && !noted
        && !cmdline.contains(crate::agents::asks::ANSWER_HOOK_FLAG)
}

/// 「这条会话在这里答不了」那一句（会话事实的 `answerOff` · `needs.manualText`）：照环境里那个值说原因。
pub(crate) fn hook_off_words(env_value: &str) -> Words {
    Words(match HookOff::from_env_value(env_value) {
        Some(HookOff::Settings(w)) => {
            let why = match w {
                SettingsWhy::Unread => copy_text("beAsk.manual.whyUnread", &[]),
                SettingsWhy::NotJson => copy_text("beAsk.manual.whyJson", &[]),
                SettingsWhy::Shape => copy_text("beAsk.manual.whyShape", &[]),
            };
            copy_text("beAsk.manual.offSettings", &[("why", &why)])
        }
        Some(HookOff::Launcher) => copy_text("beAsk.manual.offLauncher", &[]),
        Some(HookOff::Policy) => copy_text("beAsk.manual.offPolicy", &[]),
        Some(HookOff::Version) => copy_text("beAsk.manual.offVersion", &[]),
        Some(HookOff::EnvLost) => copy_text("beAsk.manual.offEnvLost", &[]),
        Some(HookOff::Elsewhere) => copy_text("beAsk.manual.offElsewhere", &[]),
        None => copy_text("beAsk.manual.offUnknown", &[]),
    })
}
use crate::common::cells::{Tone, Words};
use copy_core::copy_text;
use serde_json::Value;

/// 一个选项排在哪一段（唯一一处）：拒绝一族 · 继续规划 ⇒ 不照做那一段；改到终端里答 · 有题时的不回答 ⇒ 放开那一段；其余照做。
pub(crate) fn row_of(a: &AskIn, id: &str) -> ChoiceRow {
    match id {
        "release" => ChoiceRow::Release,
        "dismiss" if !a.questions.is_empty() => ChoiceRow::Release,
        "deny" | "deny-note" | "deny-stop" | "plan-keep" => ChoiceRow::Hold,
        _ => ChoiceRow::Main,
    }
}

/// 成品：选项按种类写好字。
pub(crate) fn product_of(id: &str, a: &AskIn, since_ms: u64) -> Ask {
    let c = |id: &str, text: String, tone: Tone, note: NoteNeed| Choice {
        id: id.to_string(),
        text: Words(text),
        tone,
        note,
        row: row_of(a, id),
    };
    let mut choices = Vec::new();
    match a.kind {
        AskKind::Approve => {
            choices.push(c(
                "allow",
                copy_text("beAsk.choice.allow", &[]),
                Tone::Plain,
                NoteNeed::None,
            ));
            for (i, s) in a.suggestions.iter().enumerate() {
                choices.push(c(
                    &format!("always-{i}"),
                    copy_text("beAsk.choice.allowAlways", &[("rule", &s.text)]),
                    Tone::Plain,
                    NoteNeed::None,
                ));
            }
            choices.push(c(
                "deny",
                copy_text("beAsk.choice.deny", &[]),
                Tone::Warn,
                NoteNeed::None,
            ));
            choices.push(c(
                "deny-note",
                copy_text("beAsk.choice.denyNote", &[]),
                Tone::Warn,
                NoteNeed::Required,
            ));
            choices.push(c(
                "deny-stop",
                copy_text("beAsk.choice.denyStop", &[]),
                Tone::Warn,
                NoteNeed::None,
            ));
        }
        AskKind::Plan => {
            choices.push(c(
                "plan-auto",
                copy_text("beAsk.choice.planAuto", &[]),
                Tone::Plain,
                NoteNeed::None,
            ));
            choices.push(c(
                "plan-edits",
                copy_text("beAsk.choice.planEdits", &[]),
                Tone::Plain,
                NoteNeed::None,
            ));
            choices.push(c(
                "plan-manual",
                copy_text("beAsk.choice.planManual", &[]),
                Tone::Plain,
                NoteNeed::None,
            ));
            choices.push(c(
                "plan-keep",
                copy_text("beAsk.choice.planKeep", &[]),
                Tone::Warn,
                NoteNeed::Optional,
            ));
        }
        AskKind::Question => {
            if !a.questions.is_empty() {
                choices.push(c(
                    "answer",
                    copy_text("beAsk.choice.answer", &[]),
                    Tone::Plain,
                    NoteNeed::None,
                ));
            }
            choices.push(c(
                "dismiss",
                copy_text("beAsk.choice.dismiss", &[]),
                Tone::Warn,
                NoteNeed::None,
            ));
        }
    }
    if a.blocks_terminal {
        choices.push(c(
            "release",
            copy_text("beAsk.choice.release", &[]),
            Tone::Plain,
            NoteNeed::None,
        ));
    }
    Ask {
        id: id.to_string(),
        kind: a.kind,
        tool: a.tool.clone(),
        run: a.agent_id.clone().map(|id| AskRun {
            id,
            kind: a.agent_type.clone(),
        }),
        what: a.what.clone(),
        detail: a.detail.clone(),
        choices,
        questions: a
            .questions
            .iter()
            .enumerate()
            .map(|(qi, q)| Question {
                id: format!("q{qi}"),
                text: q.question.clone(),
                header: q.header.clone(),
                multi: q.multi,
                options: q
                    .options
                    .iter()
                    .enumerate()
                    .map(|(oi, o)| QuestionOption {
                        id: format!("q{qi}o{oi}"),
                        text: o.label.clone(),
                        detail: o.description.clone(),
                    })
                    .collect(),
                other: true,
            })
            .collect(),
        since_ms,
    }
}

/// 格目录的样本（每一格都有值：选项 · 题 · 选项说明 · 子 agent 那两格）。
pub(crate) fn specimen() -> Ask {
    product_of(
        "ask-1",
        &AskIn {
            sid: "s".into(),
            kind: AskKind::Question,
            tool: "t".into(),
            what: Some("w".into()),
            detail: "d".into(),
            suggestions: Vec::new(),
            questions: vec![crate::agents::AskQuestion {
                header: Some("h".into()),
                question: "q".into(),
                multi: false,
                options: vec![crate::agents::AskOption {
                    label: "l".into(),
                    description: Some("d".into()),
                }],
            }],
            agent_id: Some("a".into()),
            agent_type: Some("t".into()),
            blocks_terminal: true,
            input: Value::Null,
        },
        1,
    )
}

#[cfg(test)]
#[path = "../../../tests/backend/agents/asks_tests.rs"]
mod tests;
