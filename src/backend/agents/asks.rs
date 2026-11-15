//! 待答请求的**成品**（会话 `needs.ask`）与它怎么写字：通用、不认是哪一家（那一家的 hook 输入由各家适配层翻成 [`super::AskIn`]）。
//! 住 `agents/` 这一层（同过程里那张提问卡 [`super::StepAsk`]）：登记表（`control::asks`）与会话事实（`observe::recognize::facts` 的 `needs`）都要它，
//! 而那两层彼此不引。

use super::{AskIn, AskKind};
pub use cmd_wire::needs::{
    Ask, AskRun, Choice, ChoiceLook, ChoiceRow, NoteNeed, Question, QuestionOption,
};

/// 一条挂着的请求，后端自己用的那一份：线上那一份成品 ＋ 何时挂上（epoch ms，这台的钟；算已等多久用，不上线）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked {
    pub ask: Ask,
    pub since_ms: u64,
}

impl Asked {
    /// **界面答不答得了的唯一判法**：成品一项都不给选（挡着终端、那一家又认不得终端里那个框）⇒ 答不了、那一句「在终端里答」（会话事实照它不交 `needs.ask`、只交 `manualText`）；
    /// 给得出选项 ⇒ `None`（界面里答）。
    pub fn manual(&self) -> Option<Words> {
        self.ask
            .choices
            .is_empty()
            .then(|| Words(copy_text("beAsk.manual.inTerminal", &[])))
    }
}

/// 会话级答题 hook 叫 ccm 的那个子命令（`control::asks::hook`；那一家的 hook 设置里写的就是它）。
pub const ANSWER_HOOK_FLAG: &str = "--answer-hook";
/// 那一家的 hook 叫 ccm 的那几个词（打头的 `--`：后面是后端的词），后面再接那一家的 kind。
pub(crate) const ANSWER_HOOK_WORDS: [&str; 2] = ["--", ANSWER_HOOK_FLAG];

/// ccm 起会话时**挂不上**答题 hook（用户自己给的会话级设置读不出 / 不是那一家认的形）⇒ 起的那个进程环境里带这个变量；
/// 会话事实读那个进程的环境认出「这条会话只能在终端里答批准」（后端内部的 `answerOff`，经 `needs.manualText` 出）。
pub const ANSWER_HOOK_OFF_ENV: &str = "CCM_ANSWER_HOOK_OFF";

/// 答题 hook 为什么没挂上（ccm 起会话时定，经 [`ANSWER_HOOK_OFF_ENV`] 的值带到那个进程里，会话事实读回来写那一句）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookOff {
    /// 用户自带的会话级设置（`--settings`）并不进答题 hook：为什么见 [`SettingsWhy`]。
    Settings(SettingsWhy),
    /// 会话由自定义启动器起（不是这一家默认的那一个），而经环境交插件那一路这一家没有 / 这台判不了子进程与接没接上（读不出别的进程的环境）。
    Launcher,
    /// 换了启动器，本机 managed 策略不许加载会话插件（`disableSideloadFlags` · `allowManagedHooksOnly`）。
    Policy,
    /// 装着的那一家版本够不着（或认不出版本）：换了启动器 ⇒ 低于认插件目录变量的那一版；默认启动器 ⇒ 低于那一家答题口的版本门（`AskFace::gate`）。
    Version,
    /// 换了启动器、ccm 经环境交了插件，持着会话的进程环境里却没有它：启动器没把环境传下去（会话事实读出来的，ccm 不写这个值）。
    EnvLost,
    /// 会话不是 ccm 起的：持着它的进程身上看不出挂过答题口（[`started_elsewhere`]；会话事实读出来的，ccm 不写这个值）。
    Elsewhere,
    /// 这一家对这类请求不给经钩子答的口（`AskFace::cannot` 认的那一种，比如某一家的提问）：请求挂不上、照常在终端里问
    /// （待答登记表那一侧记的，ccm 不写这个值）。
    NoAnswer,
    /// 透传参数里用户自己写了整张钩子表、盖掉我们垫的那一张（`AskFace::overridden` 认的那一形）：照旧起、不动他的参数，这一趟当没挂。
    Overridden,
}

/// 用户自带的会话级设置并不进去的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsWhy {
    /// 读不出：文件不在 / 打不开，或旗标后面没给值。
    Unread,
    /// 读出来不是 JSON。
    NotJson,
    /// 是 JSON，但不是那一家认的会话级设置的形状（并不进去）。
    Shape,
}

impl HookOff {
    /// 写进那个进程环境的值（ASCII 短码；会话事实按它认原因）。
    pub fn env_value(self) -> &'static str {
        match self {
            HookOff::Settings(SettingsWhy::Unread) => "settings-unread",
            HookOff::Settings(SettingsWhy::NotJson) => "settings-json",
            HookOff::Settings(SettingsWhy::Shape) => "settings-shape",
            HookOff::Launcher => "launcher",
            HookOff::Policy => "policy",
            HookOff::Version => "version",
            HookOff::EnvLost => "env-lost",
            HookOff::Elsewhere => "elsewhere",
            HookOff::NoAnswer => "no-answer",
            HookOff::Overridden => "overridden",
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
            HookOff::NoAnswer,
            HookOff::Overridden,
        ]
        .into_iter()
        .find(|o| o.env_value() == v)
    }
}

/// 我们那个答题插件目录（`<家>/.cc-monitor/…`，绝对路径；ccm 起会话那一下写进去，起会话 · hook · 会话事实都按这一处算）。
/// 家说不出 ⇒ 照实拒（判只在 `platform::paths::under_home`），不拼成相对路径；家名不是 UTF-8 ⇒ 原样留着那几个字节。
pub fn plugin_dir_under(home: &std::path::Path) -> Result<std::path::PathBuf, String> {
    crate::platform::paths::under_home(Some(home), relay_route_core::ANSWER_PLUGIN_DIR_REL)
}

/// 插件目录在环境变量里那一项的写法（交下去 · 认得出交没交下去都按这一处）：家说不出（[`plugin_dir_under`] 拒）
/// 或家名不是 UTF-8（环境变量那一串装不下）⇒ 没有。
pub fn plugin_dir_in_env(home: &std::path::Path) -> Option<String> {
    plugin_dir_under(home).ok()?.to_str().map(str::to_owned)
}

/// 插件目录清单的分隔符（这台的路径清单分隔符；那一家照 `--plugin-dir` 那样一项一项加载）。
const DIRS_SEP: char = if cfg!(windows) { ';' } else { ':' };

/// 一串插件目录清单里有没有 `dir`。
pub fn dirs_have(list: &str, dir: &str) -> bool {
    list.split(DIRS_SEP).any(|p| p == dir)
}

/// 用户已有的清单后面接上 `dir`（已有就不重复加）。
pub fn dirs_with(list: Option<&str>, dir: &str) -> String {
    match list.filter(|l| !l.is_empty()) {
        None => dir.to_string(),
        Some(l) if dirs_have(l, dir) => l.to_string(),
        Some(l) => format!("{l}{DIRS_SEP}{dir}"),
    }
}

/// 持着会话的那个进程（环境里没有挂不上的那个变量）是不是别处起的（纯函数）：那一家有答题口，而它身上一样 ccm 挂过的痕迹都没有 ——
/// 命令行里没有 hook 客户端那个旗标（`--settings` 那一路垫进去的）· 插件目录变量里没有我们那个目录（换了启动器那一路）· ccm 没给它留过便条。
/// 命令行读不出（非 Linux · 进程刚没了）⇒ 不下这个判断（`false`：只说只能在终端里答，不编原因）。
pub fn started_elsewhere(
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

/// 「这条会话在这里答不了」那一句（后端内部的 `answerOff`，经 `needs.manualText` 出）：照环境里那个值说原因。
pub fn hook_off_words(env_value: &str) -> Words {
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
        Some(HookOff::NoAnswer) => copy_text("beAsk.manual.offNoAnswer", &[]),
        Some(HookOff::Overridden) => copy_text("beAsk.manual.offOverridden", &[]),
        None => copy_text("beAsk.manual.offUnknown", &[]),
    })
}
use crate::common::cells::Words;
use copy_core::copy_text;
use serde_json::Value;

/// 一个选项排在哪一段（唯一一处）：批计划 · 有题的提问 ⇒ 全在照做那一段（一行）；
/// 别的：拒绝一族 ⇒ 不照做那一段；其余照做。
pub(crate) fn row_of(a: &AskIn, id: &str) -> ChoiceRow {
    if a.kind == AskKind::Plan || !a.questions.is_empty() {
        return ChoiceRow::Main;
    }
    match id {
        "deny" | "deny-note" | "deny-stop" => ChoiceRow::Hold,
        _ => ChoiceRow::Main,
    }
}

/// 一道题的题头那一句（[`Question::head`] 的唯一一处）：多选带「多选」那一说法，单选照题头原样。
fn head_of(header: Option<&str>, multi: bool) -> Option<String> {
    match (header, multi) {
        (Some(h), true) => Some(copy_text("beAsk.question.multi", &[("header", h)])),
        (None, true) => Some(copy_text("beAsk.question.multiBare", &[])),
        (h, false) => h.map(str::to_string),
    }
}

/// 子运行发的那一条：会话记录 `record` 底下 id 为 `run` 的那个子运行派出时的标签（子运行面 `sources` ＋ `spawned`，哪一家都经它）。
/// 那一家没有子运行面 · 还没落盘 · 对不上 ⇒ `None`。挂上那一刻查一次（[`product_of`] 的 `label`）。
pub fn run_label(record: &std::path::Path, run: &str) -> Option<String> {
    let ch = super::record_face_of(record)?.children?;
    (ch.sources)(record)
        .iter()
        .filter_map(|p| (ch.spawned)(p))
        .find(|l| l.run.as_deref() == Some(run))
        .and_then(|l| l.label)
}

/// 谁要批那一句（[`AskRun::text`] 的唯一一处）：agent「名」· 类别，缺哪样少哪样。
fn run_words(label: Option<&str>, kind: Option<&str>) -> Words {
    Words(match (label, kind) {
        (Some(l), Some(k)) => format!(
            "{}{}{k}",
            copy_text("speaker.agent.title", &[("label", l)]),
            copy_text("kit.text.sep", &[])
        ),
        (Some(l), None) => copy_text("speaker.agent.title", &[("label", l)]),
        (None, Some(k)) => copy_text("askBlock.tag.run", &[("kind", k)]),
        (None, None) => copy_text("beProfile.slot.agent", &[]),
    })
}

/// 成品：选项按种类写好字；`stop_on_deny` ＝ 那一家拒绝时写得出停下这一轮（[`super::AskFace::stop_on_deny`]，假 ⇒ 不带「拒绝并停下」）；
/// `label` ＝ 子运行发的那一条派出时的标签（[`run_label`]，主会话 / 查不到 ⇒ `None`）。
/// 挡着终端的那一种（终端里有框）：`screen` ＝ 那一家认得出那个框（[`super::AskFace::answers_on_screen`]）⇒ 只给「允许 · 拒绝」
/// （app 里经读屏按，附话 · 以后不再问 · 停下这一轮按不出来）；认不得 ⇒ 一项都不给选、在终端里答。
pub fn product_of(
    id: &str,
    a: &AskIn,
    stop_on_deny: bool,
    screen: bool,
    label: Option<&str>,
) -> Ask {
    let quiz = !a.questions.is_empty();
    let c = |id: &str, text: String, look: ChoiceLook, note: NoteNeed| Choice {
        id: id.to_string(),
        text: Words(text),
        look,
        note,
        row: row_of(a, id),
        picks: id == "answer",
    };
    // 每一种的第一颗是主按钮（照做那一段第一颗，只这一颗）；停下整轮 ⇒ 危险；有题时的不回答 ⇒ 弱。
    let mut choices = Vec::new();
    match a.kind {
        _ if screen => {
            choices.push(c(
                "allow",
                copy_text("beAsk.choice.allow", &[]),
                ChoiceLook::Main,
                NoteNeed::None,
            ));
            choices.push(c(
                "deny",
                copy_text("beAsk.choice.deny", &[]),
                ChoiceLook::Plain,
                NoteNeed::None,
            ));
        }
        _ if a.blocks_terminal => {}
        AskKind::Approve => {
            choices.push(c(
                "allow",
                copy_text("beAsk.choice.allow", &[]),
                ChoiceLook::Main,
                NoteNeed::None,
            ));
            for (i, s) in a.suggestions.iter().enumerate() {
                choices.push(c(
                    &format!("always-{i}"),
                    copy_text("beAsk.choice.allowAlways", &[("rule", &s.said())]),
                    ChoiceLook::Plain,
                    NoteNeed::None,
                ));
            }
            choices.push(c(
                "deny",
                copy_text("beAsk.choice.deny", &[]),
                ChoiceLook::Plain,
                NoteNeed::None,
            ));
            choices.push(c(
                "deny-note",
                copy_text("beAsk.choice.denyNote", &[]),
                ChoiceLook::Plain,
                NoteNeed::Required,
            ));
            if stop_on_deny {
                choices.push(c(
                    "deny-stop",
                    copy_text("beAsk.choice.denyStop", &[]),
                    ChoiceLook::Danger,
                    NoteNeed::None,
                ));
            }
        }
        AskKind::Plan => {
            choices.push(c(
                "plan-auto",
                copy_text("beAsk.choice.planAuto", &[]),
                ChoiceLook::Main,
                NoteNeed::None,
            ));
            choices.push(c(
                "plan-edits",
                copy_text("beAsk.choice.planEdits", &[]),
                ChoiceLook::Plain,
                NoteNeed::None,
            ));
            choices.push(c(
                "plan-manual",
                copy_text("beAsk.choice.planManual", &[]),
                ChoiceLook::Plain,
                NoteNeed::None,
            ));
            choices.push(c(
                "plan-keep",
                copy_text("beAsk.choice.planKeep", &[]),
                ChoiceLook::Plain,
                NoteNeed::Optional,
            ));
        }
        AskKind::Answer => {
            if quiz {
                choices.push(c(
                    "answer",
                    copy_text("beAsk.choice.answer", &[]),
                    ChoiceLook::Main,
                    NoteNeed::None,
                ));
            }
            choices.push(c(
                "dismiss",
                copy_text("beAsk.choice.dismiss", &[]),
                if quiz {
                    ChoiceLook::Ghost
                } else {
                    ChoiceLook::Main
                },
                NoteNeed::None,
            ));
        }
    }
    Ask {
        id: id.to_string(),
        kind: a.kind,
        tool: a.tool.clone(),
        run: a.agent_id.clone().map(|id| AskRun {
            id,
            text: run_words(label, a.agent_type.as_deref()),
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
                head: head_of(q.header.as_deref(), q.multi),
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
    }
}

/// 格目录的样本（每一格都有值：选项 · 题 · 选项说明 · 子 agent 那两格）。
pub fn specimen() -> Ask {
    product_of(
        "ask-1",
        &AskIn {
            sid: "s".into(),
            kind: AskKind::Answer,
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
            blocks_terminal: false,
            input: Value::Null,
        },
        true,
        false,
        Some("l"),
    )
}

#[cfg(test)]
#[path = "../../../tests/backend/agents/asks_tests.rs"]
mod tests;
