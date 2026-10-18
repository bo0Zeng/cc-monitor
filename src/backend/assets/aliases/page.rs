//! 设置窗「别名与配置文件」那一页的五条命令：`profiles-read` · `profiles-resolve` · `profiles-impact` · `profiles-bases` ·
//! `profiles-write`（入口在下面 `answer_*`）。
//!
//! - **合并只有一处**：每一格合下来是什么、来自哪一段、被谁盖掉，都从 [`profile::resolve`]（→ `argv::parse_layered`）拿；
//!   「等于」那一行与 `ccm @名 -- --ccm-print` 同一个计划函数（`control::ccm::preview_resolved`）。界面只排版。
//! - **同一套词**：表单 · 树里那一行的摘要 · 合并表的「项」三处说同一件事用 [`SLOTS`] 里同一个标签；值的说法只住 [`said_of`]。
//! - **表单那几格**（[`ProfileForm`]）：一格 `null` ＝ 继承；写了才进配置文件那一段。开关只能打开（`detach` 等写 `true` 才算写了）。
//!   表单 ⇄ 一段配置的互转只住这里（[`edit_of_form`] / [`form_of`]），界面一个 ccm 选项都不认。
//! - **按条目改**：`profiles-write` 的改动折成 [`profile::Change`]，没动的段一个字节不动、手写的注释留着；
//!   盘上那份在读之后被别处改过 ⇒ `stale`，一个字节不写。改完不许多出坏处（原来就坏的那几段不挡别的段改）。

use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::profile::{self, Book, Change, Profile, ProfileEdit};
use super::{door, links, Door, Shell};
use crate::ccm_line::argv::{self, flag, Origin};

// ═══════════════════════════════════════════════════════════════════════════
// 表单那几格 · 标签
// ═══════════════════════════════════════════════════════════════════════════

/// 表单的哪一格（[`ProfileForm`] 里那一格的名字）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "ProfileSlotId"
    )
)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SlotId {
    Account,
    Tmux,
    CwdIf,
    Cwd,
    Agent,
    Args,
    Launcher,
    TmuxSize,
    Detach,
    BusRegister,
    BusNote,
    Tab,
}

/// 表单的一格：`id`（[`ProfileForm`] 里那一格的名字）· 标签（文案键）· 落在配置文件里的哪几个键。
pub(crate) struct Slot {
    pub id: SlotId,
    /// 标签（文案表里那一句）。
    pub label: fn() -> String,
    /// ccm 选项（去掉 `--` 就是配置文件里的键）；空 ＝ 交给 agent 的那串（键 `args`）。
    flags: &'static [&'static str],
}

impl Slot {
    fn label_text(&self) -> String {
        (self.label)()
    }
}

/// 表单的各格，按表单里从上到下的顺序（合并表、树里摘要也按这个顺序）。
pub(crate) const SLOTS: &[Slot] = &[
    Slot {
        id: SlotId::Account,
        label: || copy_text("beProfile.slot.account", &[]),
        flags: &[flag::ACCOUNT, flag::BASE],
    },
    Slot {
        id: SlotId::Tmux,
        label: || copy_text("beProfile.slot.tmux", &[]),
        flags: &[flag::TMUX, flag::TMUX_BASE],
    },
    Slot {
        id: SlotId::CwdIf,
        label: || copy_text("beProfile.slot.cwdIf", &[]),
        flags: &[flag::CWD_IF],
    },
    Slot {
        id: SlotId::Cwd,
        label: || copy_text("beProfile.slot.cwd", &[]),
        flags: &[flag::CWD],
    },
    Slot {
        id: SlotId::Agent,
        label: || copy_text("beProfile.slot.agent", &[]),
        flags: &[flag::AGENT],
    },
    Slot {
        id: SlotId::Args,
        label: || copy_text("beProfile.slot.args", &[]),
        flags: &[],
    },
    Slot {
        id: SlotId::Launcher,
        label: || copy_text("beProfile.slot.launcher", &[]),
        flags: &[flag::LAUNCHER],
    },
    Slot {
        id: SlotId::TmuxSize,
        label: || copy_text("beProfile.slot.tmuxSize", &[]),
        flags: &[flag::TMUX_SIZE],
    },
    Slot {
        id: SlotId::Detach,
        label: || copy_text("beProfile.slot.detach", &[]),
        flags: &[flag::DETACH],
    },
    Slot {
        id: SlotId::BusRegister,
        label: || copy_text("beProfile.slot.busRegister", &[]),
        flags: &[flag::BUS_REGISTER],
    },
    Slot {
        id: SlotId::BusNote,
        label: || copy_text("beProfile.slot.busNote", &[]),
        flags: &[flag::BUS_NOTE],
    },
    Slot {
        id: SlotId::Tab,
        label: || copy_text("beProfile.slot.tab", &[]),
        flags: &[flag::TAB],
    },
];

/// 一个 ccm 选项（或 `None` ＝ 交给 agent 的那串）归哪一格。
fn slot_of(f: Option<&str>) -> &'static Slot {
    SLOTS
        .iter()
        .find(|s| match f {
            None => s.flags.is_empty(),
            Some(f) => s.flags.contains(&f),
        })
        .expect("every profile flag has a slot (page_tests::the_form_slots_cover_exactly_the_profile_keys)")
}

/// 一项的值用人话怎么说（表单的继承值 · 树里摘要 · 合并表 · 连带表同一句）。
pub(crate) fn said_of(f: Option<&str>, vals: &[String]) -> String {
    let v = |i: usize| vals.get(i).cloned().unwrap_or_default();
    match f {
        None => super::form::join_box(vals),
        Some(flag::BASE) => copy_text("beProfile.val.base", &[]),
        Some(flag::TMUX) if vals.is_empty() => copy_text("beProfile.val.tmuxAuto", &[]),
        Some(flag::TMUX) => copy_text("beProfile.val.tmuxNamed", &[("name", &v(0))]),
        Some(flag::TMUX_BASE) => copy_text("beProfile.val.tmuxBase", &[("name", &v(0))]),
        Some(flag::CWD_IF) => copy_text("beProfile.val.cwdIf", &[("at", &v(0)), ("to", &v(1))]),
        Some(flag::CWD) => copy_text("beProfile.val.cwd", &[("dir", &v(0))]),
        Some(flag::DETACH | flag::BUS_REGISTER) => copy_text("beProfile.val.on", &[]),
        Some(flag::TAB) => match vals.first().and_then(|v| argv::tab_of(v)) {
            Some(cmd_wire::session::Tab::Hide) => copy_text("beProfile.val.tabHide", &[]),
            _ => copy_text("beProfile.val.tabShow", &[]),
        },
        Some(_) => vals.join(" "),
    }
}

/// 号那一格：哪个号，或显式不用账号。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "ProfileAccountPick"
    )
)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum AccountPick {
    Account { name: String },
    Base,
}

/// 「在哪起」那一格（子那一段只能写 tmux 的几种：当前终端是没写 tmux，继承来的 tmux 关不掉）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "ProfileTmuxMode"
    )
)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TmuxMode {
    /// tmux，名字自动取。
    Auto,
    /// tmux，就用这个名（撞了不让）。
    Fixed,
    /// tmux，以这个名为底取名（撞了换一个）。
    Base,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "ProfileTmuxPick"
    )
)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TmuxPick {
    pub mode: TmuxMode,
    #[serde(default)]
    pub name: String,
}

/// 按目录的一条：在 `at` 敲 ⇒ 进 `to`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "ProfileCwdCase"
    )
)]
#[serde(deny_unknown_fields)]
pub(crate) struct CwdCase {
    pub at: String,
    pub to: String,
}

/// 一段配置的表单：名字 · 基于谁 · 各格（`None` / `false` ＝ 继承，没写进这一段）。
/// 入参里缺的格当继承（`null` / `false`）；回包里格格都在。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProfileForm {
    pub name: String,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub account: Option<AccountPick>,
    #[serde(default)]
    pub tmux: Option<TmuxPick>,
    #[serde(default)]
    pub cwd_if: Option<Vec<CwdCase>>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    /// 交给 agent 的那串（一格文本，写法同 shell：空白分词、引号里原样）。
    #[serde(default)]
    pub args: Option<String>,
    #[serde(default)]
    pub launcher: Option<String>,
    #[serde(default)]
    pub tmux_size: Option<String>,
    #[serde(default)]
    pub detach: bool,
    #[serde(default)]
    pub bus_register: bool,
    #[serde(default)]
    pub bus_note: Option<String>,
    /// 标签页：显示 / 隐藏（`null` ＝ 继承）。
    #[serde(default)]
    pub tab: Option<cmd_wire::session::Tab>,
}

/// 表单 ⇒ 配置文件里那一段要写成的样子（ccm 选项按 [`SLOTS`] 的顺序）。一格文本的引号没配对 ⇒ 那一句。
pub(crate) fn edit_of_form(f: &ProfileForm) -> Result<ProfileEdit, String> {
    let s = |v: &Option<String>| {
        v.as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };
    let mut ccm: Vec<String> = Vec::new();
    let pair = |k: &str, v: Option<String>, ccm: &mut Vec<String>| {
        if let Some(v) = v {
            ccm.extend([k.to_string(), v]);
        }
    };
    match &f.account {
        Some(AccountPick::Account { name }) if !name.trim().is_empty() => {
            ccm.extend([flag::ACCOUNT.to_string(), name.trim().to_string()])
        }
        Some(AccountPick::Base) => ccm.push(flag::BASE.to_string()),
        _ => {}
    }
    if let Some(t) = &f.tmux {
        let n = t.name.trim();
        match t.mode {
            TmuxMode::Auto => ccm.push(flag::TMUX.to_string()),
            TmuxMode::Fixed if !n.is_empty() => ccm.push(format!("{}={n}", flag::TMUX)),
            TmuxMode::Base if !n.is_empty() => {
                ccm.extend([flag::TMUX_BASE.to_string(), n.to_string()])
            }
            _ => return Err(copy_text("beProfile.form.tmuxName", &[])),
        }
    }
    for c in f.cwd_if.iter().flatten() {
        let (at, to) = (c.at.trim(), c.to.trim());
        if at.is_empty() && to.is_empty() {
            continue;
        }
        ccm.extend([flag::CWD_IF.to_string(), at.to_string(), to.to_string()]);
    }
    pair(flag::CWD, s(&f.cwd), &mut ccm);
    pair(flag::AGENT, s(&f.agent), &mut ccm);
    pair(flag::LAUNCHER, s(&f.launcher), &mut ccm);
    pair(flag::TMUX_SIZE, s(&f.tmux_size), &mut ccm);
    if f.detach {
        ccm.push(flag::DETACH.to_string());
    }
    if f.bus_register {
        ccm.push(flag::BUS_REGISTER.to_string());
    }
    pair(flag::BUS_NOTE, s(&f.bus_note), &mut ccm);
    if let Some(t) = f.tab {
        ccm.extend([flag::TAB.to_string(), argv::tab_word(t).to_string()]);
    }
    let agent = match &f.args {
        Some(a) => super::form::split_box(a)
            .map_err(|_| copy_text("beAliasForm.passthru.unbalanced", &[]))?,
        None => Vec::new(),
    };
    Ok(ProfileEdit {
        name: f.name.trim().to_string(),
        from: s(&f.from),
        agent,
        ccm,
    })
}

/// `profiles-from-line` 的入参：手机原来存的那一条启动配置（名字 · 命令 · 在哪个目录起 · 固定的会话名）。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProfilesFromLineReq {
    pub(crate) name: String,
    /// 那一条命令原样（写法同 shell：空白分词、引号里原样）。
    pub(crate) line: String,
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) cwd: Option<String>,
    /// 起在这个固定名的 tmux 会话里（缺 ⇒ 照命令行）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) session_name: Option<String>,
}

/// 一条自由命令行 ⇒ 别名表单（[`ProfilesFromLineReq`]）：头一个词是哪一家的启动器 ⇒ 后面那串原样交那一家；
/// 是 `ccm` ⇒ 照 ccm 自己的解析器切两半、折成表单（与旧别名清单一次性迁移同一条路：[`super::edit_of_alias`]）；
/// 别的 ⇒ 那一句。命令行里没写目录 · tmux ⇒ 照给的那两格填。
pub(crate) fn form_of_line(req: &ProfilesFromLineReq) -> Answer<ProfileForm> {
    let words = super::form::split_box(&req.line)
        .map_err(|()| refused(copy_text("beAliasForm.passthru.unbalanced", &[])))?;
    let Some((head, rest)) = words.split_first() else {
        return Err(refused(copy_text("beProfile.line.notALaunch", &[])));
    };
    let name = req.name.trim().to_string();
    let mut f = if head == crate::ccm_line::SUBCOMMAND_WORD {
        let a = super::Alias::new(&name, rest.to_vec());
        let text = profile::apply_changes(Some(""), &[Change::Set(super::edit_of_alias(&a, None))])
            .map_err(refused)?;
        let book = profile::parse_book(&text);
        book.find(&name)
            .map(form_of)
            .ok_or_else(|| refused(copy_text("beProfile.line.notALaunch", &[])))?
    } else if let Some(a) = crate::agents::REGISTRY
        .iter()
        .find(|a| a.launch.is_some_and(|l| l.default_launcher == head))
    {
        ProfileForm {
            name: name.clone(),
            agent: (a.kind != crate::agents::default_kind()).then(|| a.kind.to_string()),
            args: (!rest.is_empty()).then(|| super::form::join_box(rest)),
            ..ProfileForm::default()
        }
    } else {
        return Err(refused(copy_text("beProfile.line.notALaunch", &[])));
    };
    let given = |v: &Option<String>| {
        v.as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };
    if f.cwd.is_none() {
        f.cwd = given(&req.cwd);
    }
    if f.tmux.is_none() {
        f.tmux = given(&req.session_name).map(|name| TmuxPick {
            mode: TmuxMode::Fixed,
            name,
        });
    }
    Ok(f)
}

/// 一段配置 ⇒ 表单（自己写了的那几格有值，其余继承）。
pub(crate) fn form_of(p: &Profile) -> ProfileForm {
    let mut f = ProfileForm {
        name: p.name.clone(),
        from: p.from.clone(),
        args: (!p.agent.is_empty()).then(|| super::form::join_box(&p.agent)),
        ..ProfileForm::default()
    };
    for (fl, vals) in words_of(p) {
        let one = || vals.first().cloned().unwrap_or_default();
        match fl {
            flag::ACCOUNT => f.account = Some(AccountPick::Account { name: one() }),
            flag::BASE => f.account = Some(AccountPick::Base),
            flag::TMUX if vals.is_empty() => {
                f.tmux = Some(TmuxPick {
                    mode: TmuxMode::Auto,
                    name: String::new(),
                })
            }
            flag::TMUX => {
                f.tmux = Some(TmuxPick {
                    mode: TmuxMode::Fixed,
                    name: one(),
                })
            }
            flag::TMUX_BASE => {
                f.tmux = Some(TmuxPick {
                    mode: TmuxMode::Base,
                    name: one(),
                })
            }
            flag::CWD_IF => f.cwd_if.get_or_insert_with(Vec::new).push(CwdCase {
                at: one(),
                to: vals.get(1).cloned().unwrap_or_default(),
            }),
            flag::CWD => f.cwd = Some(one()),
            flag::AGENT => f.agent = Some(one()),
            flag::LAUNCHER => f.launcher = Some(one()),
            flag::TMUX_SIZE => f.tmux_size = Some(one()),
            flag::DETACH => f.detach = true,
            flag::BUS_REGISTER => f.bus_register = true,
            flag::BUS_NOTE => f.bus_note = Some(one()),
            flag::TAB => f.tab = argv::tab_of(&one()),
            _ => {}
        }
    }
    f
}

/// 一段自己写的 ccm 选项，逐组 `(选项, 值)`（问 ccm 自己的解析器）。
fn words_of(p: &Profile) -> Vec<(&'static str, Vec<String>)> {
    let words = p.ccm_words();
    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let Ok(w) = argv::word_at(&words, i) else {
            break;
        };
        i += w.len;
        out.push((w.flag, w.vals));
    }
    out
}

/// 树里那一行里的一项：值自己说得清的（不用账号 · tmux · 按目录 · 进哪）只写值；开关只写标签；其余「标签 值」。
/// 子那一段写的 tmux 前面带「+」（它是加在父上面的）。
fn phrase_of(f: Option<&'static str>, vals: &[String], child: bool) -> String {
    let slot = slot_of(f);
    let val = said_of(f, vals);
    match f {
        Some(flag::TMUX | flag::TMUX_BASE) if child => {
            copy_text("beProfile.said.plus", &[("val", &val)])
        }
        Some(flag::BASE | flag::TMUX | flag::TMUX_BASE | flag::CWD_IF | flag::CWD) => val,
        Some(flag::DETACH | flag::BUS_REGISTER) => slot.label_text(),
        _ => copy_text(
            "beProfile.said.slot",
            &[("label", &slot.label_text()), ("val", &val)],
        ),
    }
}

/// 树里那一行：这一段自己写的几项，按 [`SLOTS`] 的顺序。没有「基于」、也没写 tmux 的那一段前面说「当前终端」。
fn summary_of(p: &Profile, shell: Shell) -> String {
    let child = p.from.is_some();
    let mut parts: Vec<(usize, String)> = words_of(p)
        .into_iter()
        .map(|(f, vals)| {
            let at = SLOTS
                .iter()
                .position(|s| s.id == slot_of(Some(f)).id)
                .unwrap_or(0);
            (at, phrase_of(Some(f), &vals, child))
        })
        .collect();
    if !p.agent.is_empty() {
        let at = SLOTS
            .iter()
            .position(|s| s.id == slot_of(None).id)
            .unwrap_or(0);
        parts.push((at, phrase_of(None, &p.agent, child)));
    }
    parts.sort_by_key(|(at, _)| *at);
    let mut out: Vec<String> = parts.into_iter().map(|(_, s)| s).collect();
    let has_tmux = words_of(p)
        .iter()
        .any(|(f, _)| slot_of(Some(f)).id == SlotId::Tmux);
    if !child && !has_tmux {
        out.insert(
            0,
            match shell {
                Shell::PowerShell => copy_text("rsAccountAliases.said.hereWindow", &[]),
                Shell::Posix => copy_text("rsAccountAliases.said.hereTerminal", &[]),
            },
        );
    }
    out.join(&copy_text("rsAccountAliases.said.sep", &[]))
}

// ═══════════════════════════════════════════════════════════════════════════
// 成品
// ═══════════════════════════════════════════════════════════════════════════

/// 这台说哪种 shell（链接只在 POSIX；Windows 上全是函数）。
fn here_shell() -> Shell {
    if cfg!(windows) {
        Shell::PowerShell
    } else {
        Shell::Posix
    }
}

/// 一段自己写的一项：配置文件里的键 · 归表单哪一格 · 值 · 第几行。
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "ProfileOwnItem"
    )
)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OwnItem {
    key: String,
    slot: SlotId,
    vals: Vec<String>,
    #[cfg_attr(test, ts(type = "number"))]
    line: usize,
}

/// 一段（或整份文件）的坏处：第几行 · 那一句 · 复制详情。
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "ProfileProblem"
    )
)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProblemOut {
    #[cfg_attr(test, ts(type = "number | null"))]
    line: Option<usize>,
    message: String,
    /// 复制详情（有下层原话时才有：TOML 解析器那一句）；没有 ⇒ `null`。
    detail: Option<String>,
}

/// 这一段装成什么：`~/.cc-monitor/bin` 里的链接 · 终端函数（撞了 PATH 上的同名程序 · Windows 上恒是函数）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "lowercase")]
pub(crate) enum ProfileKind {
    Link,
    Function,
}

/// 「账号那一形」：号与 tmux 按合并下来的算。
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "ProfileAccountShape"
    )
)]
pub(crate) struct AccountShape {
    account: String,
    tmux: bool,
}

/// 树里的一段：自己写的几项 · 能不能用 · 链接还是终端函数 · 树里那一行 · 表单回填。
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "ProfileRow"
    )
)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProfileOut {
    name: String,
    from: Option<String>,
    own: Vec<OwnItem>,
    agent: Vec<String>,
    usable: bool,
    problem: Option<ProblemOut>,
    kind: ProfileKind,
    function_why: Option<String>,
    /// 写成函数时写在哪份文件、写成什么。
    function_line: Option<String>,
    /// 树里那一行（按表单同一套词写好）。
    said: String,
    form: ProfileForm,
    /// 「账号那一形」（自己只写了号、可再加 tmux）⇒ 号与 tmux 按合并下来的算；其余 ⇒ `null`（账号页按它列各号的命令名）。
    account_shape: Option<AccountShape>,
    /// 在这台终端里起它的那一行（[`super::start_line`]）：终端屏照抄、敲进去。
    start: String,
}

fn profile_out(book: &Book, p: &Profile, shell: Shell, alias_file: &str, ccm: &str) -> ProfileOut {
    let resolved = profile::resolve(book, &p.name, &[]);
    let problem = resolved.as_ref().err().map(|e| ProblemOut {
        line: book
            .problems
            .iter()
            .find(|x| x.profile.as_deref() == Some(p.name.as_str()))
            .and_then(|x| x.line)
            .or_else(|| chain_problem_line(book, &p.name)),
        message: e.clone(),
        detail: None,
    });
    let mut own: Vec<OwnItem> = p
        .items
        .iter()
        .map(|i| OwnItem {
            key: i.key.clone(),
            slot: slot_of(profile::flag_of(&i.key)).id,
            vals: i
                .groups
                .iter()
                .flat_map(|g| argv::word_at(g, 0).map(|w| w.vals).unwrap_or_default())
                .collect(),
            line: i.line,
        })
        .collect();
    if !p.agent.is_empty() {
        own.push(OwnItem {
            key: profile::ARGS_KEY.to_string(),
            slot: slot_of(None).id,
            vals: p.agent.clone(),
            line: p.line,
        });
    }
    let fun = super::wants_function(&p.name, shell);
    ProfileOut {
        name: p.name.clone(),
        from: p.from.clone(),
        own,
        agent: p.agent.clone(),
        usable: resolved.is_ok(),
        problem,
        kind: if fun {
            ProfileKind::Function
        } else {
            ProfileKind::Link
        },
        function_why: fun.then(|| {
            super::collision_note(&p.name, shell)
                .unwrap_or_else(|| copy_text("beProfile.function.always", &[]))
        }),
        function_line: fun.then(|| {
            copy_text(
                "beProfile.function.at",
                &[
                    ("path", alias_file),
                    ("line", &super::render_line(&p.name, shell)),
                ],
            )
        }),
        said: summary_of(p, shell),
        form: form_of(p),
        account_shape: super::accounts::shape_of(book, p)
            .map(|(account, tmux)| AccountShape { account, tmux }),
        start: super::start_line(&p.name, shell, ccm),
    }
}

/// 一段因为链上别的段坏了而不能用：报那一段坏处所在的行（自己没写错时）。
fn chain_problem_line(book: &Book, name: &str) -> Option<usize> {
    let mut cur = book.find(name)?;
    let mut seen = vec![cur.name.as_str()];
    loop {
        if let Some(p) = book
            .problems
            .iter()
            .find(|x| x.profile.as_deref() == Some(cur.name.as_str()))
        {
            return p.line;
        }
        let f = cur.from.as_deref()?;
        match book.find(f) {
            Some(next) if !seen.contains(&next.name.as_str()) => {
                seen.push(&next.name);
                cur = next;
            }
            Some(_) => return Some(cur.from_line),
            None => return Some(cur.from_line),
        }
    }
}

type Answer<T> = Result<T, (&'static str, String)>;

fn refused(said: String) -> (&'static str, String) {
    ("refused", said)
}

/// 盘上那份配置文件此刻的修改时间（秒）；不在 / 问不到 ⇒ `None`。
fn modified_of(d: &dyn Door, path: &str) -> Option<u64> {
    door::stat(d, path)
        .ok()
        .flatten()
        .and_then(|v| v.mtime_secs)
}

/// 上次 cc-monitor 写过之后有人改过 ⇒ 那份的修改时刻（按看的那一台的时区写好，界面照排）；没改过 ⇒ `None`。
fn edited_at(d: &dyn Door, store: &super::Store, path: &str, tz: &crate::Tz) -> Option<String> {
    if !super::edited_since_written(d, store) {
        return None;
    }
    let now = crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |x| x.as_secs());
    let at = modified_of(d, path).unwrap_or(now);
    Some(crate::common::time::fmt_at(
        i64::try_from(at).unwrap_or(i64::MAX),
        i64::try_from(now).unwrap_or(i64::MAX),
        tz,
    ))
}

/// 旧别名清单一次性转进配置文件之后的那张说明（写的那一头 [`super::migrate`]，读的这一头页首说一次）。
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "ProfilesMigrated"
    )
)]
pub(crate) struct Migrated {
    /// 转进来几段。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) count: usize,
    /// 配置文件的路径。
    pub(crate) path: String,
    /// 没能转的那几行（原样）。
    pub(crate) skipped: Vec<String>,
}

/// 一次性迁移的说明（在 ⇒ 页首说一次）。
fn migrated_note(d: &dyn Door, home: &str) -> Option<Migrated> {
    let p = door::peek(d, home, relay_route_core::PROFILES_MIGRATED_REL).ok()?;
    serde_json::from_str(&p.text?).ok()
}

/// `profiles-read` 的应答：配置文件整份。
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProfilesBook {
    /// 这台的家目录（界面拿它把路径写成 `~/…`）。
    home: String,
    /// 配置文件的路径。
    path: String,
    /// 配置文件在不在。
    exists: bool,
    /// 盘上那份的指纹（不在 ⇒ `null`），存的时候交回 `profiles-write`。
    fingerprint: Option<String>,
    /// 上次 cc-monitor 写过之后有人改过 ⇒ 那份的修改时刻（按看的那一台的时区写好）；没改过 ⇒ `null`。
    edited_at: Option<String>,
    /// TOML 本身写坏 ⇒ 那一处（这时 `profiles` 为空、不能按条目改）；否则 `null`。
    file_problem: Option<ProblemOut>,
    profiles: Vec<ProfileOut>,
    /// 配置文件不在时首建那几条的预览（在 ⇒ 空）：两条（`cc` · `cct`），没有 tmux 的那台只一条。
    seed: Vec<ProfileOut>,
    /// 空态那一块的说明与按钮字，随 `seed` 的条数（没有 tmux 的那台只建一条）；`seed` 空 ⇒ 空串。
    seed_hint: String,
    seed_create: String,
    /// 旧别名清单一次性转进来之后那张说明（「知道了」之后 `null`）。
    migrated: Option<Migrated>,
    /// 链接住的目录（`~/.cc-monitor/bin`）。
    bin_dir: String,
    /// 这台的账号表（具名号，按账号库的顺序；表单「账号」那一格的选项）。
    accounts: Vec<String>,
    /// 这台有没有 tmux（「在哪起」那一格选不选得了 tmux 看它）；探不出 ⇒ `null`。
    tmux: Option<bool>,
}

/// `profiles-read` → 整份：每段的成品 ＋ 文件级错误 ＋ 指纹 ＋ 手改过没有 ＋ 迁移说明 ＋ 空态那两条的预览 ＋ 这台有没有 tmux。
/// `tz` ＝ 看的那一台的时区：手改时刻 `editedAt` 按它写。有没有 tmux 由宿主探好交进来（`faces::profiles_face`；判据喂定值）。
pub(crate) fn read_with(d: &dyn Door, tmux: Option<bool>, tz: &crate::Tz) -> Answer<ProfilesBook> {
    let store = super::load(d).map_err(refused)?;
    let shell = here_shell();
    let path = profile::path_in(&store.home);
    let alias_file = super::alias_file_in(&store.home, shell);
    let ccm = door::join_under(&store.home, relay_route_core::BACKEND_LANDING_REL);
    let profiles: Vec<ProfileOut> = store
        .book
        .profiles
        .iter()
        .map(|p| profile_out(&store.book, p, shell, &alias_file, &ccm))
        .collect();
    let file_problem = store
        .book
        .problems
        .iter()
        .find(|p| p.profile.is_none())
        .map(|p| ProblemOut {
            line: p.line,
            message: p.message.clone(),
            detail: p
                .raw
                .as_deref()
                .map(|r| crate::wire::detail::of(Some("profiles-read"), "syntax", Some(r))),
        });
    let seed: Vec<ProfileOut> = if store.text.is_none() {
        let text = profile::apply_changes(Some(""), &seed_changes()).map_err(refused)?;
        let b = profile::parse_book(&text);
        b.profiles
            .iter()
            .map(|p| profile_out(&b, p, shell, &alias_file, &ccm))
            .collect()
    } else {
        Vec::new()
    };
    let (seed_hint, seed_create) = match seed.len() {
        0 => (String::new(), String::new()),
        1 => (
            copy_text("beProfile.seedHint.one", &[]),
            copy_text("beProfile.seedCreate.one", &[]),
        ),
        _ => (
            copy_text("beProfile.seedHint.two", &[]),
            copy_text("beProfile.seedCreate.two", &[]),
        ),
    };
    Ok(ProfilesBook {
        exists: store.text.is_some(),
        fingerprint: super::fingerprint_of(store.text.as_deref()),
        edited_at: edited_at(d, &store, &path, tz),
        file_problem,
        profiles,
        seed,
        seed_hint,
        seed_create,
        migrated: migrated_note(d, &store.home),
        bin_dir: door::join_under(&store.home, links::bin_rel()),
        accounts: super::account_table(&store.home),
        tmux,
        path,
        home: store.home,
    })
}

/// 首建那几条（`cc` · `cct` 基于 `cc`，没有 tmux 的那台只 `cc`；名字由默认那一家的 wrapper 名派生）。
fn seed_changes() -> Vec<Change> {
    let first = super::first_aliases(here_shell());
    let base = first.first().map(|a| a.name.clone());
    first
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let mut e = super::edit_of_alias(a, None);
            if i > 0 {
                e.from = base.clone();
                e.ccm.retain(|w| w != flag::END);
            }
            Change::Set(e)
        })
        .collect()
}

// ── 合并表 ──

/// 合并表的一行：哪一项 · 归哪一格 · 标签 · 值与它的说法 · 来自哪一段 · 被哪一段盖掉（留下来的 ⇒ `null`）。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "ProfileMergeRow"
    )
)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MergeRow {
    key: String,
    slot: SlotId,
    label: String,
    vals: Vec<String>,
    said: String,
    from: String,
    overridden_by: Option<String>,
}

/// `name` 合下来的每一项（父 → 子，层内照写的顺序）：留下来的与被后来那一层盖掉的都在，盖掉的带 `overriddenBy`。
fn rows_of(book: &Book, chain: &[String], origins: &[Origin]) -> Vec<MergeRow> {
    let mut rows = Vec::new();
    for (li, name) in chain.iter().enumerate() {
        let Some(p) = book.find(name) else { continue };
        for (f, vals) in words_of(p) {
            let slot = slot_of(Some(f));
            let kept = origins
                .iter()
                .any(|o| o.layer == *name && o.flag == f && o.vals == vals);
            let overridden_by = (!kept).then(|| {
                chain[li + 1..]
                    .iter()
                    .rev()
                    .find(|later| {
                        book.find(later).is_some_and(|lp| {
                            words_of(lp)
                                .iter()
                                .any(|(lf, _)| slot_of(Some(lf)).id == slot.id)
                        })
                    })
                    .cloned()
                    .unwrap_or_default()
            });
            rows.push(MergeRow {
                key: f.trim_start_matches('-').to_string(),
                slot: slot.id,
                label: slot.label_text(),
                said: said_of(Some(f), &vals),
                vals,
                from: name.clone(),
                overridden_by,
            });
        }
        if !p.agent.is_empty() {
            let slot = slot_of(None);
            rows.push(MergeRow {
                key: profile::ARGS_KEY.to_string(),
                slot: slot.id,
                label: slot.label_text(),
                said: said_of(None, &p.agent),
                vals: p.agent.clone(),
                from: name.clone(),
                overridden_by: None,
            });
        }
    }
    rows
}

/// 改动 ⇒ 盘上那份改完的全文（只在内存里）。
fn text_after(text: Option<&str>, changes: &[Change]) -> Result<String, String> {
    profile::apply_changes(text.or(Some("")), changes)
}

/// `profiles-resolve` 的入参：哪一段 · 未存的表单（`null` ＝ 按盘上那份算）· 假设在这个目录敲（`null` ＝ 家目录）。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProfilesResolveReq {
    /// 哪一段（带 `edit` 时是正在改的那一段原来的名字，新增写表单里的名字）。
    pub(crate) name: String,
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) edit: Option<ProfileForm>,
    /// `~` 打头按这台家目录展开。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) at: Option<String>,
}

/// `profiles-resolve` 的应答：继承链 · 合并表 · 「等于」那一行（整条与短形）· 合不下来的那一句。
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "ProfileResolved"
    )
)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Resolved {
    /// 继承链（父 → 子）。
    chain: Vec<String>,
    rows: Vec<MergeRow>,
    /// 这台后端算的「等于」那一行（同 `ccm @名 -- --ccm-print`）；算不出 ⇒ `null`。
    line: Option<String>,
    /// 「等于」那一行的短形（设置页那一格照抄）；算不出 ⇒ `null`。
    line_short: Option<String>,
    /// 算不出那一行时 ccm 的原话。
    line_error: Option<String>,
    /// 合不下来 ⇒ 那一句（同终端里敲这个名字）；否则 `null`。
    problem: Option<String>,
}

impl Resolved {
    fn unmerged(problem: String) -> Self {
        Resolved {
            chain: Vec::new(),
            rows: Vec::new(),
            line: None,
            line_short: None,
            line_error: None,
            problem: Some(problem),
        }
    }
}

/// 把合好的意图渲成「等于」那一行：`(意图, 家目录, 假设在哪敲)` → `(整行, 短形)`；渲不出 ⇒ 那一句。
/// 起会话那一整套计划与渲染住控制面（`control::ccm::preview_resolved`），由宿主交进来（`faces::profiles_face`）。
pub(crate) type RenderLine =
    fn(argv::Parsed, &str, Option<&str>) -> Result<(String, String), String>;

/// `profiles-resolve` → [`Resolved`]（`lineShort` ＝ 「等于」那一格的短形）。`edit`：按表单那一刻算（未存）；
/// `at`：假设在这个目录敲（缺省家目录）。`render` 见 [`RenderLine`]。
pub(crate) fn answer_resolve(
    d: &dyn Door,
    req: &ProfilesResolveReq,
    render: RenderLine,
) -> Answer<Resolved> {
    let name = req.name.as_str();
    let store = super::load(d).map_err(refused)?;
    let (book, problem_text) = match &req.edit {
        None => (store.book.clone(), None),
        Some(f) => {
            let e = edit_of_form(f).map_err(refused)?;
            let was = store.book.find(name).map(|p| p.name.clone());
            match changes_of_set(&store.book, was.as_deref(), &e)
                .and_then(|c| text_after(store.text.as_deref(), &c))
            {
                Ok(t) => (profile::parse_book(&t), None),
                Err(e) => (store.book.clone(), Some(e)),
            }
        }
    };
    let target = req.edit.as_ref().map_or(name, |f| f.name.trim());
    if let Some(e) = problem_text {
        return Ok(Resolved::unmerged(e));
    }
    match profile::resolve(&book, target, &[]) {
        Ok(r) => {
            let rows = rows_of(&book, &r.chain, &r.origins);
            let (line, line_short, line_error) =
                match render(r.parsed, &store.home, req.at.as_deref()) {
                    Ok((l, short)) => (Some(l), Some(short), None),
                    Err(e) => (None, None, Some(e)),
                };
            Ok(Resolved {
                chain: r.chain,
                rows,
                line,
                line_short,
                line_error,
                problem: None,
            })
        }
        Err(e) => Ok(Resolved::unmerged(e)),
    }
}

// ── 改动 ──

/// 线上那一种改动。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "ProfileOp"
    )
)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Op {
    /// 新增（`was` ＝ `null`）或改 `was` 那一段；名字变了 ⇒ 改名，基于它的那几段跟着改「基于」。
    Set {
        was: Option<String>,
        form: ProfileForm,
    },
    /// 删一段；还被别的段基于 ⇒ 要给 `children`：`reparent`（改成基于它的父）· `cascade`（一起删）。
    Remove {
        name: String,
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        children: Option<Children>,
    },
    /// 配置文件还不在时建出来：`seed` ⇒ 带首建那两条，否则一段都没有。
    Init { seed: bool },
    /// 迁移说明「知道了」：删掉那张说明。
    AckMigrated {},
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "ProfileChildren"
    )
)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Children {
    Reparent,
    Cascade,
}

/// 改 / 新增一段 ⇒ 配置文件那几处改动（改名 ⇒ 先改名、子那几段的「基于」跟上，再按表单写这一段）。
fn changes_of_set(book: &Book, was: Option<&str>, e: &ProfileEdit) -> Result<Vec<Change>, String> {
    let mut out = Vec::new();
    match was {
        Some(w) if w != e.name => {
            if book.find(&e.name).is_some() {
                return Err(copy_text("beProfile.rename.taken", &[("name", &e.name)]));
            }
            out.push(Change::Rename {
                from: w.to_string(),
                to: e.name.clone(),
            });
            for k in profile::children_of(book, w) {
                out.push(Change::SetFrom {
                    name: k.to_string(),
                    from: Some(e.name.clone()),
                });
            }
        }
        None if book.find(&e.name).is_some() => {
            return Err(copy_text("beProfile.rename.taken", &[("name", &e.name)]));
        }
        _ => {}
    }
    let mut e = e.clone();
    if e.from.as_deref() == Some(e.name.as_str())
        || was.is_some_and(|w| e.from.as_deref() == Some(w))
    {
        return Err(copy_text("beProfile.chain.self", &[("name", &e.name)]));
    }
    if let Some(f) = &e.from {
        if book.find(f).is_none() {
            return Err(copy_text("beProfile.chain.unknown", &[("name", f)]));
        }
    }
    e.name = e.name.trim().to_string();
    out.push(Change::Set(e));
    Ok(out)
}

/// 一段的全部后代（基于它的、再基于它们的 …），按文件里的顺序。
fn descendants(book: &Book, name: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut frontier = vec![name.to_string()];
    while let Some(n) = frontier.pop() {
        for k in profile::children_of(book, &n) {
            if !out.iter().any(|o| o == k) && k != name {
                out.push(k.to_string());
                frontier.push(k.to_string());
            }
        }
    }
    let order: Vec<&str> = book.profiles.iter().map(|p| p.name.as_str()).collect();
    out.sort_by_key(|n| order.iter().position(|o| o == n));
    out
}

fn changes_of_remove(
    book: &Book,
    name: &str,
    children: Option<Children>,
) -> Result<Vec<Change>, String> {
    let p = book
        .find(name)
        .ok_or_else(|| copy_text("beProfile.chain.unknown", &[("name", name)]))?;
    let kids = profile::children_of(book, name);
    let mut out = Vec::new();
    match (kids.is_empty(), children) {
        (true, _) => {}
        (false, None) => {
            return Err(copy_text(
                "beProfile.install.basedOn",
                &[
                    ("name", name),
                    (
                        "children",
                        &kids.join(&copy_text("beProfile.list.sep", &[])),
                    ),
                ],
            ))
        }
        (false, Some(Children::Reparent)) => {
            for k in kids {
                out.push(Change::SetFrom {
                    name: k.to_string(),
                    from: p.from.clone(),
                });
            }
        }
        (false, Some(Children::Cascade)) => {
            for k in descendants(book, name) {
                out.push(Change::Remove(k));
            }
        }
    }
    out.push(Change::Remove(name.to_string()));
    Ok(out)
}

/// 线上那几种改动 ⇒ 配置文件的改动（`AckMigrated` / `Init` 另说，不在这里）。
/// **依次**判：每一条都在「盘上那份 ＋ 前面几条改完」那一份上判（后一条看得见前一条新增 / 改名的段）。
fn changes_of(text: Option<&str>, ops: &[Op]) -> Result<Vec<Change>, String> {
    let mut out = Vec::new();
    let mut cur = text.unwrap_or("").to_string();
    let mut book = profile::parse_book(&cur);
    for op in ops {
        let step = match op {
            Op::Set { was, form } => {
                let e = edit_of_form(form)?;
                changes_of_set(&book, was.as_deref(), &e)?
            }
            Op::Remove { name, children } => changes_of_remove(&book, name, *children)?,
            Op::Init { seed: true } => seed_changes(),
            Op::Init { seed: false } | Op::AckMigrated {} => continue,
        };
        cur = profile::apply_changes(Some(&cur), &step)?;
        book = profile::parse_book(&cur);
        out.extend(step);
    }
    Ok(out)
}

/// 每一段（与整份文件）此刻的坏处：`None` ＝ 文件级。
fn broken(book: &Book) -> std::collections::BTreeMap<Option<String>, String> {
    let mut out = std::collections::BTreeMap::new();
    if let Some(p) = book.problems.iter().find(|p| p.profile.is_none()) {
        out.insert(None, p.message.clone());
        return out;
    }
    for p in &book.profiles {
        if let Err(e) = profile::resolve(book, &p.name, &[]) {
            out.insert(Some(p.name.clone()), e);
        }
    }
    // 写错了、连段都没认出来的那几处（名字不合格 · 不是一张表）。
    for p in &book.problems {
        if let Some(n) = &p.profile {
            out.entry(Some(n.clone()))
                .or_insert_with(|| p.message.clone());
        }
    }
    out
}

/// `profiles-impact` 的入参：同 `profiles-write` 的 `changes`（一个字节不写）。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct ProfilesImpactReq {
    pub(crate) changes: Vec<Op>,
}

/// 连带的一格：哪一格 · 标签 · 改前改后各怎么说。
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "ProfileSlotChange"
    )
)]
pub(crate) struct SlotChange {
    slot: SlotId,
    label: String,
    before: String,
    after: String,
}

/// 改动直接点名的那几段之外、合下来会变的一段（变得合不下来 ⇒ `problem` 是那一句，`changes` 空）。
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "ProfileAffected"
    )
)]
pub(crate) struct Affected {
    name: String,
    changes: Vec<SlotChange>,
    problem: Option<String>,
}

/// `profiles-impact` 的应答。
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct ProfilesImpact {
    affected: Vec<Affected>,
}

/// `profiles-impact` → [`ProfilesImpact`]：
/// 这几处改动会让哪几段（改动直接点名的那几段之外）合下来变，改前改后各怎么说。一个字节不写。
pub(crate) fn answer_impact(d: &dyn Door, req: &ProfilesImpactReq) -> Answer<ProfilesImpact> {
    let ops = &req.changes;
    let store = super::load(d).map_err(refused)?;
    let changes = changes_of(store.text.as_deref(), ops).map_err(refused)?;
    let after_text = text_after(store.text.as_deref(), &changes).map_err(refused)?;
    let after = profile::parse_book(&after_text);
    let named: Vec<String> = changes
        .iter()
        .filter_map(|c| match c {
            Change::Set(e) => Some(e.name.clone()),
            Change::Remove(n) => Some(n.clone()),
            Change::Rename { from, to } => Some(format!("{from}\n{to}")),
            Change::SetFrom { .. } => None,
        })
        .flat_map(|s| s.split('\n').map(str::to_string).collect::<Vec<_>>())
        .collect();
    let slots_of = |b: &Book, n: &str| -> Result<Vec<(SlotId, String)>, String> {
        let r = profile::resolve(b, n, &[])?;
        let rows = rows_of(b, &r.chain, &r.origins);
        Ok(SLOTS
            .iter()
            .map(|s| {
                let said: Vec<String> = rows
                    .iter()
                    .filter(|x| x.slot == s.id && x.overridden_by.is_none())
                    .map(|x| x.said.clone())
                    .collect();
                (
                    s.id,
                    said.join(&copy_text("rsAccountAliases.said.sep", &[])),
                )
            })
            .collect())
    };
    let mut affected = Vec::new();
    for p in &store.book.profiles {
        if named.contains(&p.name) || after.find(&p.name).is_none() {
            continue;
        }
        let before = slots_of(&store.book, &p.name);
        let now = slots_of(&after, &p.name);
        match (before, now) {
            (Ok(b), Ok(a)) => {
                let diffs: Vec<SlotChange> = b
                    .iter()
                    .zip(&a)
                    .filter(|(x, y)| x.1 != y.1)
                    .map(|((id, bs), (_, as_))| {
                        let s = SLOTS.iter().find(|s| s.id == *id).expect("slot listed");
                        SlotChange {
                            slot: *id,
                            label: s.label_text(),
                            before: bs.clone(),
                            after: as_.clone(),
                        }
                    })
                    .collect();
                if !diffs.is_empty() {
                    affected.push(Affected {
                        name: p.name.clone(),
                        changes: diffs,
                        problem: None,
                    });
                }
            }
            (Ok(_), Err(e)) => affected.push(Affected {
                name: p.name.clone(),
                changes: Vec::new(),
                problem: Some(e),
            }),
            _ => {}
        }
    }
    Ok(ProfilesImpact { affected })
}

/// `profiles-bases` 的入参：正在改 / 新建的那一段的名字。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct ProfilesBasesReq {
    pub(crate) name: String,
}

/// 「基于」下拉的一项：名字 · 它基于谁 · 树里那一行 · 选不选得了（自己那一项 `false`）。
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "ProfileBase"
    )
)]
pub(crate) struct Base {
    name: String,
    from: Option<String>,
    said: String,
    selectable: bool,
}

/// `profiles-bases` 的应答。
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct ProfilesBases {
    bases: Vec<Base>,
}

/// `profiles-bases` → [`ProfilesBases`]：「基于」下拉能选的（选了不成圈的）＋ 自己（灰着）。
pub(crate) fn answer_bases(d: &dyn Door, req: &ProfilesBasesReq) -> Answer<ProfilesBases> {
    let name = req.name.as_str();
    let store = super::load(d).map_err(refused)?;
    let shell = here_shell();
    let loops = |cand: &str| -> bool {
        let mut cur = Some(cand.to_string());
        let mut seen: Vec<String> = Vec::new();
        while let Some(c) = cur {
            if c == name {
                return true;
            }
            if seen.contains(&c) {
                return false;
            }
            seen.push(c.clone());
            cur = store.book.find(&c).and_then(|p| p.from.clone());
        }
        false
    };
    let mut bases: Vec<Base> = store
        .book
        .profiles
        .iter()
        .filter(|p| p.name == name || !loops(&p.name))
        .map(|p| Base {
            name: p.name.clone(),
            from: p.from.clone(),
            said: summary_of(p, shell),
            selectable: p.name != name,
        })
        .collect();
    if store.book.find(name).is_none() && !name.is_empty() {
        bases.push(Base {
            name: name.to_string(),
            from: None,
            said: String::new(),
            selectable: false,
        });
    }
    Ok(ProfilesBases { bases })
}

/// `profiles-write` 的入参：依次做的改动 ＋ 读回时的指纹（必给，字符串或 `null`；盘上此刻不是那一份 ⇒ `stale`）。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct ProfilesWriteReq {
    pub(crate) changes: Vec<Op>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub(crate) fingerprint: Option<String>,
}

/// `profiles-write` 的应答：配置文件 / 终端函数文件 / 链接动没动 · 写完那一份的指纹 · 终端函数那份真改了 ⇒ 「已开的终端要重读」那一句。
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct ProfilesWriteDone {
    wrote: bool,
    fingerprint: Option<String>,
    reload: Option<String>,
}

/// `profiles-write` → [`ProfilesWriteDone`]。盘上被别处改过 ⇒ `stale`；
/// 改完多出坏处（原来就坏的不算）⇒ `refused`，一个字节不写。
pub(crate) fn answer_write(d: &dyn Door, req: &ProfilesWriteReq) -> Answer<ProfilesWriteDone> {
    let ops = &req.changes;
    let fp = req.fingerprint.as_deref();
    let store = super::load(d).map_err(refused)?;
    let path = profile::path_in(&store.home);
    if ops.iter().any(|o| matches!(o, Op::AckMigrated {})) {
        if migrated_note(d, &store.home).is_some() {
            door::remove(
                d,
                &store.home,
                relay_route_core::PROFILES_MIGRATED_REL,
                false,
            )
            .map_err(refused)?;
        }
    }
    let touches_file = ops.iter().any(|o| !matches!(o, Op::AckMigrated {}));
    if !touches_file {
        return Ok(ProfilesWriteDone {
            wrote: false,
            fingerprint: super::fingerprint_of(store.text.as_deref()),
            reload: None,
        });
    }
    if super::fingerprint_of(store.text.as_deref()).as_deref() != fp {
        return Err((
            "stale",
            copy_text(
                "rsAccountAliases.install.changedElsewhere",
                &[("path", &path)],
            ),
        ));
    }
    let changes = changes_of(store.text.as_deref(), ops).map_err(refused)?;
    let base = match (
        &store.text,
        ops.iter().any(|o| matches!(o, Op::Init { .. })),
    ) {
        (Some(_), true) => {
            return Err(refused(copy_text(
                "beProfile.init.exists",
                &[("path", &path)],
            )))
        }
        (Some(t), false) => Some(t.as_str()),
        (None, _) => None,
    };
    let text = profile::apply_changes(base, &changes).map_err(refused)?;
    let was = broken(&store.book);
    let now = broken(&profile::parse_book(&text));
    let fresh: Vec<String> = now
        .iter()
        .filter(|(k, _)| !was.contains_key(*k))
        .map(|(k, m)| match k {
            Some(n) => copy_text("beProfile.named.say", &[("name", n), ("said", m)]),
            None => m.clone(),
        })
        .collect();
    if !fresh.is_empty() {
        return Err(refused(fresh.join("\n")));
    }
    let wrote = match super::write_profiles(d, &store.home, store.text.as_deref(), &text) {
        Ok(w) => w,
        Err(super::WriteErr::Stale(e)) => return Err(("stale", e)),
        Err(super::WriteErr::Refused(e)) => return Err(refused(e)),
    };
    let out = super::sync_outputs(d, &store.home, &profile::parse_book(&text), None);
    if let Some(p) = out.problems.first() {
        return Err(refused(p.clone()));
    }
    let shell_file = super::alias_file_in(&store.home, here_shell());
    Ok(ProfilesWriteDone {
        wrote: wrote || out.links_changed || !out.rewrote.is_empty(),
        fingerprint: super::fingerprint_of(Some(&text)),
        reload: out
            .rewrote
            .contains(&shell_file)
            .then(|| super::reload_hint(&shell_file)),
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// 帧命令（typed）：经这台的文件管理面读写 ⇒ 阻塞档，带文件面
// ═══════════════════════════════════════════════════════════════════════════

use crate::wire::typed::{of_pair, Command};

crate::wire::typed::codes! {
    /// 配置文件那几条只读的码：读没成 · 表单写不成配置（那一句说清哪一样）。
    pub(crate) enum ProfilesErr {
        Refused = "refused",
    }
}

crate::wire::typed::codes! {
    /// `profiles-write` 的码：读写没成 · 改完多出坏处（那一句说清哪一样）· 盘上那份在读回之后被别处改过（一个字节没写）。
    pub(crate) enum ProfilesWriteErr {
        Refused = "refused",
        Stale = "stale",
    }
}

/// `profiles-impact`：这几处改动会连带哪几段（经这台的文件管理面读写 ⇒ 阻塞档，带文件面）。
pub(crate) struct ProfilesImpactCmd;

impl Command for ProfilesImpactCmd {
    const NAME: &'static str = "profiles-impact";
    type Req = ProfilesImpactReq;
    type Reply = ProfilesImpact;
    type Err = ProfilesErr;
}

impl crate::wire::typed::Handle for ProfilesImpactCmd {
    type Needs = crate::wire::typed::needs::Files;
    fn run(
        req: ProfilesImpactReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<ProfilesImpact, ProfilesErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let files = needs;

            answer_impact(files, &req).map_err(of_pair(ProfilesErr::Refused))
        }
    }
}

/// `profiles-bases`：「基于」下拉能选的几段（经这台的文件管理面读写 ⇒ 阻塞档，带文件面）。
pub(crate) struct ProfilesBasesCmd;

impl Command for ProfilesBasesCmd {
    const NAME: &'static str = "profiles-bases";
    type Req = ProfilesBasesReq;
    type Reply = ProfilesBases;
    type Err = ProfilesErr;
}

impl crate::wire::typed::Handle for ProfilesBasesCmd {
    type Needs = crate::wire::typed::needs::Files;
    fn run(
        req: ProfilesBasesReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<ProfilesBases, ProfilesErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let files = needs;

            answer_bases(files, &req).map_err(of_pair(ProfilesErr::Refused))
        }
    }
}

/// `profiles-write`：按条目改配置文件（经这台的文件管理面读写 ⇒ 阻塞档，带文件面）。
pub(crate) struct ProfilesWrite;

impl Command for ProfilesWrite {
    const NAME: &'static str = "profiles-write";
    type Req = ProfilesWriteReq;
    type Reply = ProfilesWriteDone;
    type Err = ProfilesWriteErr;
}

impl crate::wire::typed::Handle for ProfilesWrite {
    type Needs = crate::wire::typed::needs::Files;
    fn run(
        req: ProfilesWriteReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<ProfilesWriteDone, ProfilesWriteErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let files = needs;

            answer_write(files, &req).map_err(of_pair(ProfilesWriteErr::Refused))
        }
    }
}

/// `profiles-from-line`：一条自由命令行 ⇒ 别名表单（判法 [`form_of_line`]；不读不写盘）。
pub(crate) struct ProfilesFromLine;

impl Command for ProfilesFromLine {
    const NAME: &'static str = "profiles-from-line";
    type Req = ProfilesFromLineReq;
    type Reply = ProfileForm;
    type Err = ProfilesErr;
}

impl crate::wire::typed::Handle for ProfilesFromLine {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: ProfilesFromLineReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<ProfileForm, ProfilesErr>> + Send + 'static {
        async move { form_of_line(&req).map_err(of_pair(ProfilesErr::Refused)) }
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/assets/aliases/page_tests.rs"]
mod tests;
