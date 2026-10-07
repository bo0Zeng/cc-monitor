//! 别名表单 ⇄ 一条别名（原样的 ccm argv）：**表单与参数的互转只住这里**，界面只画、只回填
//! （帧命令 `aliases-to-form` · `aliases-from-form`，入口在 [`super::answer_to_form`] / [`super::answer_from_form`]）。
//!
//! 参数是什么意思一律问 ccm 自己的解析器：从哪切（`argv::last_end`）· 一组几个词（`argv::word_at`）·
//! 落成什么意图（`argv::apply_word`）。这里不另写一份 ccm 文法；后端多认一个旗标，表单这边自动认得它是 ccm 的。
//!
//! # 往返不改一个字
//!
//! 表单只有语义上的那几格；参数里还有表单说不出的：写法（`--cwd=/x`）· 顺序 · 重复 · 没有格子的 ccm 选项 · 打头的 `new`。
//! 「改」那一下把原来那条一起交回来（`orig`），按组比：没动过的那一组原样留在原位；动了的那一组换成规范写法、
//! 放在它原来第一次出现的位置；原来没有的追加（ccm 那一侧接在末尾，交给 agent 那一侧的模型放最前）。
//! ⇒ 只改名字，参数逐字不变；认不得的词留在它原来那一边、原来那个位置。
//!
//! # 两格成串的文本：交给 agent 的参数 · 其它 ccm 参数
//!
//! 一格是一串词，写法照 shell：空白分词；`'…'` 里原样；`"…"` 里 `\"` 与 `\\` 是转义；引号外 `\` 只转义空白、引号与它自己，
//! 别的照原样（Windows 路径 `C:\work` 不用改写）。拼回一格时能原样写的原样写、否则加引号 —— 切回来逐字相等。

use copy_core::copy_text;
use serde::{Deserialize, Serialize};

use super::Alias;
use crate::control::ccm::argv::{self, flag, CwdSpec};
use crate::control::launch_render::ccm_invocation::MODEL_FLAG;
use crate::platform::shell::dialect::RestTo;

/// 「在哪起」那一格。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TmuxMode {
    /// 当前终端。
    None,
    /// tmux，名字自动取。
    Auto,
    /// tmux，就用这个名（撞了不让）。
    Named,
    /// tmux，以这个为底取名（撞了换一个）。
    Base,
    /// 接回一个 tmux 会话（会话名调用时跟上）。
    Attach,
}

/// 工作目录的一种情况：在 `at` 敲 ⇒ 进 `to`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CwdCase {
    pub at: String,
    pub to: String,
}

/// 表单那几格。**只是编辑界面** —— 合不合格由 `aliases-render` 判。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AliasForm {
    pub name: String,
    /// 按所在目录分的那几种情况，按序（第一条对上的算）。
    pub cwd_if: Vec<CwdCase>,
    /// 其余情况进哪；空 = 当前目录。
    pub cwd: String,
    /// 账号名；空 = 不指定。
    pub account: String,
    /// 显式不带账号。
    pub base: bool,
    pub tmux: TmuxMode,
    /// `named` / `base` 那个名。
    pub tmux_name: String,
    pub agent: String,
    pub model: String,
    pub launcher: String,
    pub tmux_size: String,
    pub detach: bool,
    pub bus_register: bool,
    pub bus_note: String,
    /// 交给 agent 的其余参数（一串，写法见头注）。
    pub passthru: String,
    /// 表单没有格子的 ccm 参数（一串，写法见头注）：原样留在 `--` 右边。
    pub ccm_other: String,
}

/// 参数里的一组词归表单哪一格。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Group {
    Model,
    Pass,
    CwdIf,
    Cwd,
    Account,
    Tmux,
    Agent,
    Launcher,
    Size,
    Detach,
    Bus,
    BusNote,
    Other,
    /// 打头的 `new`：没有格子、也不进「其它」—— 原样留着（只许打头，放进「其它」会被挪到末尾）。
    New,
}

/// ccm 那一侧原来没有、新加的组按这个顺序追加。
const RIGHT_ORDER: &[Group] = &[
    Group::CwdIf,
    Group::Cwd,
    Group::Account,
    Group::Tmux,
    Group::Agent,
    Group::Launcher,
    Group::Size,
    Group::Detach,
    Group::Bus,
    Group::BusNote,
    Group::Other,
];

fn group_of(word: &str) -> Group {
    match word {
        flag::NEW => Group::New,
        flag::CWD_IF => Group::CwdIf,
        flag::CWD => Group::Cwd,
        flag::ACCOUNT | flag::BASE => Group::Account,
        flag::TMUX | flag::TMUX_BASE => Group::Tmux,
        flag::AGENT => Group::Agent,
        flag::LAUNCHER => Group::Launcher,
        flag::TMUX_SIZE => Group::Size,
        flag::DETACH => Group::Detach,
        flag::BUS_REGISTER => Group::Bus,
        flag::BUS_NOTE => Group::BusNote,
        _ => Group::Other,
    }
}

/// 一条别名的参数按组拆开：每组是原样的那几个词，按原来的顺序。
struct Layout {
    left: Vec<(Group, Vec<String>)>,
    right: Vec<(Group, Vec<String>)>,
    /// 有 `--`。
    end: bool,
    /// 右边落成的意图（ccm 解析器自己的那一份）。
    opts: argv::Opts,
    /// 左边第一对 `--model <名>` 的名。
    model: String,
}

fn layout(args: &[String]) -> Layout {
    let (left, right, end) = match argv::last_end(args) {
        Some(k) => (&args[..k], &args[k + 1..], true),
        None => (args, &args[..0], false),
    };
    // 交给 agent 那一侧：第一对 `--model <名>`（agent 自己的 `--` 之后不看）归「模型」，其余每个词归「其余参数」。
    let mut l = Vec::new();
    let mut model = None;
    let mut past_end = false;
    let mut i = 0;
    while i < left.len() {
        let w = &left[i];
        let pair = !past_end
            && model.is_none()
            && w == MODEL_FLAG
            && left.get(i + 1).is_some_and(|v| v != flag::END);
        if pair {
            model = Some(left[i + 1].clone());
            l.push((Group::Model, left[i..i + 2].to_vec()));
            i += 2;
            continue;
        }
        past_end |= w == flag::END;
        l.push((Group::Pass, vec![w.clone()]));
        i += 1;
    }
    // ccm 那一侧：一组几个词、什么意思都问 ccm 的解析器；认不得 / 缺值 / 立即结束的那几个归「其它」，原样一个词一个词留着。
    let mut o = argv::blank_opts(&[]);
    o.agent = String::new();
    let mut r = Vec::new();
    let mut i = 0;
    while i < right.len() {
        let (g, n) = match argv::word_at(right, i) {
            Ok(w) => match argv::apply_word(&mut o, &w, i) {
                Ok(None) => (group_of(w.flag), w.len),
                _ => (Group::Other, w.len),
            },
            Err(_) => (Group::Other, 1),
        };
        r.push((g, right[i..i + n].to_vec()));
        i += n;
    }
    Layout {
        left: l,
        right: r,
        end,
        opts: o,
        model: model.unwrap_or_default(),
    }
}

/// 「接回一个 tmux 会话」只有一形：`-- --attach`，调用时的词交给 ccm。
fn is_attach(a: &Alias) -> bool {
    a.rest_to == RestTo::Ccm
        && a.args.len() == 2
        && a.args[0] == flag::END
        && a.args[1] == flag::ATTACH
}

fn blank(name: &str) -> AliasForm {
    AliasForm {
        name: name.to_string(),
        cwd_if: Vec::new(),
        cwd: String::new(),
        account: String::new(),
        base: false,
        tmux: TmuxMode::None,
        tmux_name: String::new(),
        agent: String::new(),
        model: String::new(),
        launcher: String::new(),
        tmux_size: String::new(),
        detach: false,
        bus_register: false,
        bus_note: String::new(),
        passthru: String::new(),
        ccm_other: String::new(),
    }
}

fn words_of(occs: &[(Group, Vec<String>)], g: Group) -> Vec<String> {
    occs.iter()
        .filter(|(x, _)| *x == g)
        .flat_map(|(_, w)| w.iter().cloned())
        .collect()
}

/// 一条别名 → 表单（「改」那一下）。不会失败：认不得的词原样进「其余参数」/「其它 ccm 参数」，**不挪边、不丢**。
pub(crate) fn to_form(a: &Alias) -> AliasForm {
    let mut f = blank(&a.name);
    if is_attach(a) {
        f.tmux = TmuxMode::Attach;
        return f;
    }
    let lay = layout(&a.args);
    let o = &lay.opts;
    f.cwd_if = o
        .cwd_if
        .iter()
        .map(|(at, to)| CwdCase {
            at: at.clone(),
            to: to.clone(),
        })
        .collect();
    f.cwd = match &o.cwd_spec {
        CwdSpec::Explicit(v) => v.clone(),
        CwdSpec::Auto => String::new(),
    };
    f.account = o.account.clone();
    f.base = o.use_base;
    (f.tmux, f.tmux_name) = if !o.tmux_base.is_empty() {
        (TmuxMode::Base, o.tmux_base.clone())
    } else if !o.tmux_name.is_empty() {
        (TmuxMode::Named, o.tmux_name.clone())
    } else if o.use_tmux {
        (TmuxMode::Auto, String::new())
    } else {
        (TmuxMode::None, String::new())
    };
    f.agent = o.agent.clone();
    f.model = lay.model.clone();
    f.launcher = o.launcher.clone();
    f.tmux_size = o.tmux_size.clone();
    f.detach = o.detach;
    f.bus_register = o.bus_register;
    f.bus_note = o.bus_note.clone();
    f.passthru = join_box(&words_of(&lay.left, Group::Pass));
    f.ccm_other = join_box(&words_of(&lay.right, Group::Other));
    f
}

/// 一条别名用人话怎么说（清单那一行的第二格，`aliases-read` 每条带一句）。和「改」那一下同一个解析器（[`to_form`]），
/// 界面不拼句。`shell` 只决定「当前终端 / 当前窗口」那一个词。
pub(crate) fn said(a: &Alias, shell: crate::platform::shell::dialect::Shell) -> String {
    use crate::platform::shell::dialect::Shell;
    let f = to_form(a);
    let mut p: Vec<String> = Vec::new();
    p.push(if f.base {
        copy_text("rsAccountAliases.said.base", &[])
    } else if f.account.is_empty() {
        copy_text("rsAccountAliases.said.defaultAccount", &[])
    } else {
        copy_text("rsAccountAliases.said.account", &[("account", &f.account)])
    });
    p.push(match f.tmux {
        TmuxMode::None => match shell {
            Shell::PowerShell => copy_text("rsAccountAliases.said.hereWindow", &[]),
            Shell::Posix => copy_text("rsAccountAliases.said.hereTerminal", &[]),
        },
        TmuxMode::Auto => copy_text("rsAccountAliases.said.tmuxAuto", &[]),
        TmuxMode::Named => copy_text("rsAccountAliases.said.tmuxNamed", &[("name", &f.tmux_name)]),
        TmuxMode::Base => copy_text("rsAccountAliases.said.tmuxBase", &[("name", &f.tmux_name)]),
        TmuxMode::Attach => copy_text("rsAccountAliases.said.attach", &[]),
    });
    if f.tmux == TmuxMode::Attach {
        p.push(copy_text("rsAccountAliases.said.attachHow", &[("alias", &f.name)]));
    }
    for c in &f.cwd_if {
        p.push(copy_text("rsAccountAliases.said.cwdIf", &[("at", &c.at), ("to", &c.to)]));
    }
    if !f.cwd.is_empty() {
        p.push(if f.cwd_if.is_empty() {
            copy_text("rsAccountAliases.said.cwd", &[("dir", &f.cwd)])
        } else {
            copy_text("rsAccountAliases.said.cwdElse", &[("dir", &f.cwd)])
        });
    }
    if !f.agent.is_empty() {
        p.push(copy_text("rsAccountAliases.said.agent", &[("agent", &f.agent)]));
    }
    if !f.model.is_empty() {
        p.push(copy_text("rsAccountAliases.said.model", &[("model", &f.model)]));
    }
    if !f.launcher.is_empty() {
        p.push(copy_text("rsAccountAliases.said.launcher", &[("launcher", &f.launcher)]));
    }
    if !f.tmux_size.is_empty() {
        p.push(copy_text("rsAccountAliases.said.size", &[("size", &f.tmux_size)]));
    }
    if f.detach {
        p.push(copy_text("rsAccountAliases.said.detach", &[]));
    }
    if f.bus_register {
        p.push(copy_text("rsAccountAliases.said.bus", &[]));
    }
    if !f.passthru.is_empty() {
        p.push(copy_text("rsAccountAliases.said.passthru", &[("args", &f.passthru)]));
    }
    if !f.ccm_other.is_empty() {
        p.push(copy_text("rsAccountAliases.said.ccmOther", &[("args", &f.ccm_other)]));
    }
    p.join(&copy_text("rsAccountAliases.said.sep", &[]))
}

/// 一组在表单这个样子下的规范写法（空 = 这一组不出现）。
/// 控件上关掉的组合照样不拼：不进 tmux ⇒ 容器那几格不出现；不 `--detach` ⇒ 不登记 cc-bus；只有半边的情况不拼。
fn canon(f: &AliasForm, g: Group) -> Result<Vec<String>, String> {
    let t = |s: &str| s.trim().to_string();
    let pair = |k: &str, v: &str| -> Vec<String> {
        if v.trim().is_empty() {
            Vec::new()
        } else {
            vec![k.to_string(), t(v)]
        }
    };
    let in_tmux = f.tmux != TmuxMode::None;
    let bus = in_tmux && f.detach && f.bus_register;
    Ok(match g {
        Group::Model => pair(MODEL_FLAG, &f.model),
        Group::Pass => {
            split_box(&f.passthru).map_err(|_| copy_text("beAliasForm.passthru.unbalanced", &[]))?
        }
        Group::Other => split_box(&f.ccm_other)
            .map_err(|_| copy_text("beAliasForm.ccmOther.unbalanced", &[]))?,
        Group::CwdIf => f
            .cwd_if
            .iter()
            .filter(|c| !c.at.trim().is_empty() && !c.to.trim().is_empty())
            .flat_map(|c| [flag::CWD_IF.to_string(), t(&c.at), t(&c.to)])
            .collect(),
        Group::Cwd => pair(flag::CWD, &f.cwd),
        Group::Account if !f.account.trim().is_empty() => pair(flag::ACCOUNT, &f.account),
        Group::Account if f.base => vec![flag::BASE.to_string()],
        Group::Account => Vec::new(),
        Group::Tmux => {
            let n = t(&f.tmux_name);
            match f.tmux {
                TmuxMode::Auto => vec![flag::TMUX.to_string()],
                TmuxMode::Named if !n.is_empty() => vec![format!("{}={n}", flag::TMUX)],
                TmuxMode::Base if !n.is_empty() => vec![flag::TMUX_BASE.to_string(), n],
                _ => Vec::new(),
            }
        }
        Group::Agent if f.agent.is_empty() => Vec::new(),
        Group::Agent => vec![flag::AGENT.to_string(), f.agent.clone()],
        Group::Launcher => pair(flag::LAUNCHER, &f.launcher),
        Group::Size if in_tmux => pair(flag::TMUX_SIZE, &f.tmux_size),
        Group::Detach if in_tmux && f.detach => vec![flag::DETACH.to_string()],
        Group::Bus if bus => vec![flag::BUS_REGISTER.to_string()],
        Group::BusNote if bus => pair(flag::BUS_NOTE, &f.bus_note),
        Group::Size | Group::Detach | Group::Bus | Group::BusNote | Group::New => Vec::new(),
    })
}

/// 一侧按原来那几组走：没动的原样留下；动了的在它第一次出现的地方换成规范写法、其余几处删。回带着出现过的组。
fn rebuild(
    f: &AliasForm,
    was: Option<&AliasForm>,
    occs: &[(Group, Vec<String>)],
) -> Result<(Vec<String>, Vec<Group>), String> {
    let mut out = Vec::new();
    let mut seen: Vec<Group> = Vec::new();
    for (g, toks) in occs {
        let same = match was {
            Some(w) => canon(f, *g)? == canon(w, *g)?,
            None => false,
        };
        if same {
            out.extend(toks.iter().cloned());
        } else if !seen.contains(g) {
            out.extend(canon(f, *g)?);
        }
        if !seen.contains(g) {
            seen.push(*g);
        }
    }
    Ok((out, seen))
}

/// 表单 → 一条别名。`orig` = 正在改的那一条（新增 ⇒ `None`）：没动过的组照它原样留。
/// 一格文本的引号没配对 ⇒ `Err(那句话)`。
pub(crate) fn from_form(f: &AliasForm, orig: Option<&Alias>) -> Result<Alias, String> {
    let name = match orig {
        Some(o) if o.name.trim() == f.name.trim() => o.name.clone(),
        _ => f.name.trim().to_string(),
    };
    if f.tmux == TmuxMode::Attach {
        return Ok(Alias {
            name,
            args: vec![flag::END.to_string(), flag::ATTACH.to_string()],
            rest_to: RestTo::Ccm,
        });
    }
    let orig = orig.filter(|o| !is_attach(o));
    let lay = orig.map(|o| layout(&o.args));
    let was = orig.map(to_form);
    let none: &[(Group, Vec<String>)] = &[];
    let (mut left, seen) = rebuild(f, was.as_ref(), lay.as_ref().map_or(none, |l| &l.left))?;
    if !seen.contains(&Group::Model) {
        let mut m = canon(f, Group::Model)?;
        m.append(&mut left);
        left = m;
    }
    if !seen.contains(&Group::Pass) {
        left.extend(canon(f, Group::Pass)?);
    }
    let (mut right, seen) = rebuild(f, was.as_ref(), lay.as_ref().map_or(none, |l| &l.right))?;
    for g in RIGHT_ORDER {
        if !seen.contains(g) {
            right.extend(canon(f, *g)?);
        }
    }
    // `--`：右边有东西 · 左边自己带着 agent 的 `--`（不补一个就会被当成分界）· 原来就是空着右边写了一个 —— 这三种才写。
    let bare_end = lay.as_ref().is_some_and(|l| l.end && l.right.is_empty());
    let mut args = left;
    if !right.is_empty() || args.iter().any(|w| w == flag::END) || bare_end {
        args.push(flag::END.to_string());
        args.extend(right);
    }
    Ok(Alias {
        name,
        args,
        rest_to: orig.map_or(RestTo::Agent, |o| o.rest_to),
    })
}

/// 一格文本 → 词（写法见头注）。引号没配对 ⇒ `Err`。
pub(crate) fn split_box(s: &str) -> Result<Vec<String>, ()> {
    let cs: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut started = false;
    let mut i = 0;
    // 下一个字符是不是 `want` 里的一个（不越界）。
    let next_in = |i: usize, want: &dyn Fn(char) -> bool| cs.get(i + 1).is_some_and(|&n| want(n));
    while i < cs.len() {
        let c = cs[i];
        match c {
            c if c.is_whitespace() => {
                if started {
                    out.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            '\'' => {
                started = true;
                let close = cs[i + 1..].iter().position(|&x| x == '\'').ok_or(())?;
                cur.extend(&cs[i + 1..i + 1 + close]);
                i += close + 1;
            }
            '"' => {
                started = true;
                loop {
                    i += 1;
                    match cs.get(i) {
                        Some('"') => break,
                        Some('\\') if next_in(i, &|n| matches!(n, '"' | '\\')) => {
                            i += 1;
                            cur.push(cs[i]);
                        }
                        Some(&x) => cur.push(x),
                        None => return Err(()),
                    }
                }
            }
            '\\' => {
                started = true;
                if next_in(i, &|n| n.is_whitespace() || matches!(n, '\'' | '"' | '\\')) {
                    i += 1;
                    cur.push(cs[i]);
                } else {
                    cur.push('\\');
                }
            }
            c => {
                started = true;
                cur.push(c);
            }
        }
        i += 1;
    }
    if started {
        out.push(cur);
    }
    Ok(out)
}

/// 一个词写进一格：切回来恰是它自己、又不以 `\` 结尾（后面那个分隔空格会被它转义掉）⇒ 原样；
/// 否则没有 `'` 就用 `'…'`，有就用 `"…"`（`"` 与 `\` 前加 `\`）。
fn join_word(w: &str) -> String {
    if !w.ends_with('\\') && split_box(w).is_ok_and(|v| v.len() == 1 && v[0] == w) {
        return w.to_string();
    }
    if !w.contains('\'') {
        return format!("'{w}'");
    }
    let mut s = String::from('"');
    for c in w.chars() {
        if c == '"' || c == '\\' {
            s.push('\\');
        }
        s.push(c);
    }
    s.push('"');
    s
}

/// 词 → 一格文本（[`split_box`] 的逆）。
pub(crate) fn join_box(words: &[String]) -> String {
    words
        .iter()
        .map(|w| join_word(w))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
#[path = "../../../../tests/backend/assets/aliases/form_tests.rs"]
mod tests;
