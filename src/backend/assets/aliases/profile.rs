//! **配置文件**（`~/.cc-monitor/profiles.toml`）：一段 ＝ 一份具名配置 ＝ 一组 ccm 选项，可写「基于」另一段。
//!
//! ```toml
//! [cc]
//! cwd-if = [["~", "~/projects/notes"]]
//!
//! [cct]
//! from = "cc"        # 基于 cc
//! ccm-tmux = true
//! ```
//!
//! - **同一套词**：键就是 ccm 选项 `--` 后面那个词（[`key_of`]），值按「开关写 `true` · 一个值写字符串 ·
//!   几个值写字符串数组（`--cwd-if` 可写成数组的数组，一条一组）」变回命令行那几个词，交 ccm 自己的解析器（`argv::word_at`）读。
//!   只在这里有、命令行上没有的只有两项：`from`（＝ 命令行上的 `@名`）与 `args`（＝ 命令行上 `--` 左边交给 agent 的那串）。
//! - **先后**：继承链父 → 子，命令行当场给的最后盖；每一格怎么盖只住 `argv::parse_layered`。
//! - **可以手改**：每次用都读、校验；写错报到哪一行，写错的那一段（与基于它的）此刻不能用，别的照用。
//! - **按条目改**：设置窗改一段只动那一段（[`set_profile`] / [`remove_profile`]），手写的注释与排版留着。
//! - **一组会话**住同一份文件的 `[set."组名"]`：`row = [{ cwd, alias, n, tab-group? }]`，一行 ＝ 目录 ＋ 别名 ＋ 补足到几个；
//!   `set` 因此是保留名、不能当别名。读出 [`Book::sets`]，写经 [`Change::SetSet`] / [`Change::RemoveSet`]；一组写错只坏那一组。

use crate::common::said::IntoNote as _;
use crate::platform::fs::Ro;
use copy_core::copy_text;
use toml_edit::{Array, DocumentMut, Item, Table, Value};

use crate::ccm_line::argv::{self, flag, Die, Layer, Origin, Parsed};

/// 「基于」那一项的键（命令行上是 `@名`）。
pub(crate) const FROM_KEY: &str = "from";
/// 交给 agent 的那几个词（命令行上 `--` 左边那串）。
pub const ARGS_KEY: &str = "args";
/// 一组会话那一节（`[set."组名"]`）：保留名，不能当别名（它也是 shell 内建命令，做成链接或函数都会坏事）。
pub(crate) const SET_KEY: &str = "set";
/// 一组里那几行的键。
const ROW_KEY: &str = "row";
/// 一行的四个键：目录 · 别名 · 个数 · 标签页分组名（只在改过组名时写）。
const ROW_CWD: &str = "cwd";
const ROW_ALIAS: &str = "alias";
const ROW_N: &str = "n";
const ROW_TAB_GROUP: &str = "tab-group";
/// 一行最多几个、一组合计最多几个。
pub const ROW_MAX: u8 = 9;
pub const SET_MAX: u32 = 12;

/// 能写进配置的 ccm 词。不收的（`new` · `--attach` · `--hosts` · `--ccm-*` 诊断口 · `--ccm-sid` · `--account-dir` · `--no-rotation`）每次取值都不同或是一次性的动作
/// （`--no-rotation` 是给程序起的那一发用的；常用的会话不轮换在轮换设置里定），写成固定配置没意义；它们照样能在命令行 `--` 右边当场给。
pub const PROFILE_FLAGS: &[&str] = &[
    flag::CWD,
    flag::CWD_IF,
    flag::ACCOUNT,
    flag::BASE,
    flag::TMUX,
    flag::TMUX_BASE,
    flag::TMUX_SIZE,
    flag::DETACH,
    flag::HOSTED,
    flag::AGENT,
    flag::LAUNCHER,
    flag::TAB,
    flag::TAB_GROUP,
    flag::SAY,
];

/// [`PROFILE_FLAGS`] 写进文件的那个键（去掉打头的 `--`）。
pub fn key_of(f: &'static str) -> &'static str {
    f.trim_start_matches('-')
}

pub(crate) fn flag_of(key: &str) -> Option<&'static str> {
    PROFILE_FLAGS.iter().copied().find(|f| key_of(f) == key)
}

/// 配置文件里写的一项：键 ＋ 变回的命令行词（一组一个 `Vec`；`--cwd-if` 可以几组）＋ 行号。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setting {
    pub key: String,
    pub groups: Vec<Vec<String>>,
    pub line: usize,
}

/// 一段（一份具名配置）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    /// 段头那一行。
    pub line: usize,
    pub from: Option<String>,
    /// `from` 那一行（报「基于的不存在」用）。
    pub from_line: usize,
    pub agent: Vec<String>,
    pub items: Vec<Setting>,
    /// 内置的那一段（用户文件里没有、由后端照账号表现算，`accounts::with_builtins`）；用户文件里的 ⇒ `false`。
    pub builtin: bool,
}

impl Profile {
    /// 这一段自己那几项，写成命令行 `--` 右边的词。
    pub fn ccm_words(&self) -> Vec<String> {
        self.items
            .iter()
            .flat_map(|i| i.groups.iter().flatten().cloned())
            .collect()
    }

    fn layer(&self) -> Layer {
        Layer {
            name: self.name.clone(),
            agent: self.agent.clone(),
            ccm: self.ccm_words(),
        }
    }
}

/// 一组里的一行：在哪个目录 · 用哪条别名 · 补足到几个 · 进哪个标签页组（`None` ＝ 缺省，由补足那一侧算）· 行号。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetRow {
    pub cwd: String,
    pub alias: String,
    pub n: u8,
    pub tab_group: Option<String>,
    pub line: usize,
}

/// 一组会话（`[set."名"]`）：名字 · 段头那一行 · 几行 · 写错的几处（有 ⇒ 这一组此刻不能用，别的组与别名照用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Set {
    pub name: String,
    pub line: usize,
    pub rows: Vec<SetRow>,
    pub problems: Vec<Problem>,
}

/// 一处写错：哪一段（`None` ＝ 整份文件；组那一侧的记在组身上）· 第几行 · 什么错。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Problem {
    pub profile: Option<String>,
    pub line: Option<usize>,
    pub message: String,
    /// 下层原话（TOML 解析器那一句）：不上句子，进页面应答那一格的复制详情。
    #[serde(skip)]
    pub raw: Option<String>,
}

/// 读出来的整份：认得的几段 ＋ 几组 ＋ 写错的几处。TOML 本身写错 ⇒ 一段一组都不认（整份不能用）。
/// 组写错的那几处记在那一组自己身上（[`Set::problems`]），不进 `problems`：一组坏了不挡别名。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Book {
    pub profiles: Vec<Profile>,
    pub sets: Vec<Set>,
    pub problems: Vec<Problem>,
}

impl Book {
    pub fn find(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    pub fn find_set(&self, name: &str) -> Option<&Set> {
        self.sets.iter().find(|s| s.name == name)
    }

    fn problem_of(&self, name: &str) -> Option<&Problem> {
        self.problems
            .iter()
            .find(|p| p.profile.as_deref() == Some(name))
    }

    /// 整份文件级的错（TOML 写错）。
    pub(crate) fn file_problem(&self) -> Option<&Problem> {
        self.problems.iter().find(|p| p.profile.is_none())
    }
}

/// 字节偏移 ⇒ 行号（从 1 数）。
fn line_at(raw: &str, at: usize) -> usize {
    raw.as_bytes()[..at.min(raw.len())]
        .iter()
        .filter(|b| **b == b'\n')
        .count()
        + 1
}

/// 这个名字能不能当一段配置的名字：两种 shell 都认得的命令名，而且不是 ccm 自己（[`crate::ccm_line::is_own_name`]）。
pub fn name_ok(name: &str) -> Result<(), String> {
    use crate::platform::shell::dialect::Shell;
    if name == SET_KEY {
        return Err(copy_text("beProfile.name.reserved", &[("name", name)]));
    }
    if crate::ccm_line::is_own_name(name) {
        return Err(copy_text("beProfile.name.own", &[("name", name)]));
    }
    if !Shell::Posix.dialect().name_is_valid(name)
        || !Shell::PowerShell.dialect().name_is_valid(name)
    {
        return Err(copy_text("beProfile.name.bad", &[("name", name)]));
    }
    Ok(())
}

/// 一项的值 ⇒ 命令行那几组词（`f` 是这一项的旗标）。只认三种写法；认不得、或解析器拒 ⇒ 那句话。
fn groups_of(key: &str, f: &'static str, item: &Item) -> Result<Vec<Vec<String>>, String> {
    let bad = || copy_text("beProfile.value.shape", &[("key", key)]);
    let strs = |a: &Array| -> Option<Vec<String>> {
        a.iter().map(|v| v.as_str().map(str::to_string)).collect()
    };
    // `(那组词, 值有几个)`：值的个数与解析器读出来的对不上 ⇒ 写法不对（开关写了字符串 · 一个值的写成了数组）。
    let wanted: Vec<(Vec<String>, usize)> = match item.as_value() {
        Some(Value::Boolean(b)) if *b.value() => vec![(vec![f.to_string()], 0)],
        Some(Value::Boolean(_)) => {
            return Err(copy_text("beProfile.value.switchFalse", &[("key", key)]))
        }
        // `--ccm-tmux` 的名只认 `=` 那一形（它光秃秃是开关）；其余一个值的写成 `--x <值>`，与命令行同形。
        Some(Value::String(s)) if f == flag::TMUX => vec![(vec![format!("{f}={}", s.value())], 1)],
        Some(Value::String(s)) => vec![(vec![f.to_string(), s.value().to_string()], 1)],
        Some(Value::Array(a)) if a.iter().all(|v| v.is_str()) && !a.is_empty() => {
            let vals = strs(a).ok_or_else(bad)?;
            let n = vals.len();
            vec![(std::iter::once(f.to_string()).chain(vals).collect(), n)]
        }
        Some(Value::Array(a)) if !a.is_empty() && a.iter().all(|v| v.as_array().is_some()) => a
            .iter()
            .map(|v| {
                let vals = v.as_array().and_then(strs).ok_or_else(bad)?;
                let n = vals.len();
                Ok((std::iter::once(f.to_string()).chain(vals).collect(), n))
            })
            .collect::<Result<_, String>>()?,
        _ => return Err(bad()),
    };
    let mut out = Vec::new();
    for (g, n) in wanted {
        let w = argv::word_at(&g, 0).map_err(|Die(m)| parser_said(key, &m))?;
        if w.len != g.len() || w.vals.len() != n {
            return Err(bad());
        }
        let mut scratch = argv::blank_opts(&[]);
        argv::apply_word(&mut scratch, &w, 1).map_err(|Die(m)| parser_said(key, &m))?;
        argv::check_values(&scratch).map_err(|Die(m)| parser_said(key, &m))?;
        // 目录要钉住（`~` 打头或绝对）：配置在哪敲都是同一份，相对的说不清是相对谁。
        if f == flag::CWD || f == flag::CWD_IF {
            if let Some(v) = w.vals.iter().find(|v| !super::pinned_anywhere(v)) {
                return Err(copy_text(
                    "beProfile.value.notPinned",
                    &[("key", key), ("value", v)],
                ));
            }
        }
        out.push(g);
    }
    Ok(out)
}

fn parser_said(key: &str, m: &str) -> String {
    copy_text("beProfile.value.parser", &[("key", key), ("why", m)])
}

/// **读口**：整份文本 ⇒ [`Book`]。一个字节的盘都不碰。
pub fn parse_book(text: &str) -> Book {
    let doc = match toml_edit::Document::parse(text) {
        Ok(d) => d,
        Err(e) => {
            return Book {
                profiles: Vec::new(),
                sets: Vec::new(),
                problems: vec![Problem {
                    profile: None,
                    line: e.span().map(|s| line_at(text, s.start)),
                    message: copy_text("beProfile.file.syntax", &[]),
                    raw: Some(e.message().trim().to_string()),
                }],
            }
        }
    };
    let mut book = Book::default();
    let line_of = |k: &toml_edit::Key| k.span().map_or(0, |s| line_at(text, s.start));
    for (name, item) in doc.iter() {
        let (key, _) = doc
            .as_table()
            .get_key_value(name)
            .expect("key listed just above");
        let line = line_of(key);
        if name == SET_KEY {
            match item.as_table() {
                Some(t) => book.sets.extend(sets_of(text, t)),
                None => book.problems.push(Problem {
                    profile: None,
                    line: Some(line),
                    message: copy_text("beProfile.set.shape", &[]),
                    raw: None,
                }),
            }
            continue;
        }
        let mut problem = |profile: Option<&str>, line: usize, message: String| {
            book.problems.push(Problem {
                profile: profile.map(str::to_string),
                line: Some(line),
                message,
                raw: None,
            })
        };
        let Some(t) = item.as_table() else {
            problem(
                None,
                line,
                copy_text("beProfile.file.topLevel", &[("key", name)]),
            );
            continue;
        };
        if let Err(e) = name_ok(name) {
            problem(Some(name), line, e);
            continue;
        }
        let mut p = Profile {
            name: name.to_string(),
            line,
            from: None,
            from_line: line,
            agent: Vec::new(),
            items: Vec::new(),
            builtin: false,
        };
        for (k, v) in t.iter() {
            let (kk, _) = t.get_key_value(k).expect("key listed just above");
            let kl = line_of(kk);
            match k {
                FROM_KEY => match v.as_str() {
                    Some(s) => {
                        p.from = Some(s.to_string());
                        p.from_line = kl;
                    }
                    None => problem(Some(name), kl, copy_text("beProfile.value.from", &[])),
                },
                ARGS_KEY => match v.as_array().and_then(|a| {
                    a.iter()
                        .map(|x| x.as_str().map(str::to_string))
                        .collect::<Option<Vec<_>>>()
                }) {
                    Some(words) => p.agent = words,
                    None => problem(Some(name), kl, copy_text("beProfile.value.args", &[])),
                },
                _ => match flag_of(k) {
                    None => problem(
                        Some(name),
                        kl,
                        copy_text("beProfile.value.unknown", &[("key", k)]),
                    ),
                    Some(f) => match groups_of(k, f, v) {
                        Ok(groups) => p.items.push(Setting {
                            key: k.to_string(),
                            groups,
                            line: kl,
                        }),
                        Err(e) => problem(Some(name), kl, e),
                    },
                },
            }
        }
        book.profiles.push(p);
    }
    book
}

/// `set` 那一节 ⇒ 几组（照写下的先后）。一组写错只记在它自己身上。
fn sets_of(text: &str, t: &Table) -> Vec<Set> {
    let span_line = |s: Option<std::ops::Range<usize>>| s.map_or(0, |s| line_at(text, s.start));
    t.iter()
        .map(|(name, item)| {
            let (key, _) = t.get_key_value(name).expect("key listed just above");
            let line = span_line(key.span());
            let mut errs: Vec<(usize, String)> = Vec::new();
            let mut rows: Vec<SetRow> = Vec::new();
            if name.trim().is_empty() {
                errs.push((line, copy_text("beProfile.set.name", &[])));
            }
            match item.as_table() {
                None => errs.push((line, copy_text("beProfile.set.shape", &[]))),
                Some(body) => {
                    let mut found: Vec<(&dyn toml_edit::TableLike, usize)> = Vec::new();
                    for (k, v) in body.iter() {
                        let (kk, _) = body.get_key_value(k).expect("key listed just above");
                        let kl = span_line(kk.span());
                        if k != ROW_KEY {
                            errs.push((kl, copy_text("beProfile.set.shape", &[])));
                        } else if let Some(a) = v.as_array() {
                            for x in a.iter() {
                                match x.as_inline_table() {
                                    Some(r) => found.push((r, span_line(x.span()).max(kl))),
                                    None => {
                                        errs.push((kl, copy_text("beProfile.set.rowShape", &[])))
                                    }
                                }
                            }
                        } else if let Some(a) = v.as_array_of_tables() {
                            for r in a.iter() {
                                found.push((r, span_line(r.span()).max(kl)));
                            }
                        } else {
                            errs.push((kl, copy_text("beProfile.set.rowShape", &[])));
                        }
                    }
                    for (r, rl) in found {
                        match row_of(r, rl) {
                            Ok(row) => rows.push(row),
                            Err(m) => errs.push((rl, m)),
                        }
                    }
                    if rows.is_empty() && errs.is_empty() {
                        errs.push((line, copy_text("beProfile.set.empty", &[])));
                    }
                }
            }
            for (i, r) in rows.iter().enumerate() {
                if let Some(first) = rows[..i]
                    .iter()
                    .find(|o| o.cwd == r.cwd && o.alias == r.alias)
                {
                    errs.push((
                        r.line,
                        copy_text(
                            "beProfile.set.dup",
                            &[
                                ("cwd", &r.cwd),
                                ("alias", &r.alias),
                                ("line", &first.line.to_string()),
                            ],
                        ),
                    ));
                }
            }
            let total: u32 = rows.iter().map(|r| u32::from(r.n)).sum();
            if total > SET_MAX {
                errs.push((
                    line,
                    copy_text(
                        "beProfile.set.total",
                        &[("max", &SET_MAX.to_string()), ("n", &total.to_string())],
                    ),
                ));
            }
            Set {
                name: name.to_string(),
                line,
                rows,
                problems: errs
                    .into_iter()
                    .map(|(l, message)| Problem {
                        profile: None,
                        line: Some(l),
                        message,
                        raw: None,
                    })
                    .collect(),
            }
        })
        .collect()
}

/// 一行 ⇒ [`SetRow`]：四个键之外的不认；目录要钉住；别名要是能当别名的名字；个数 1–[`ROW_MAX`]；组名非空。
/// 别名在不在不在这里判（不在 ⇒ 那一行起不了、别的照起，判在补足那一侧）。
fn row_of(t: &dyn toml_edit::TableLike, line: usize) -> Result<SetRow, String> {
    let shape = || copy_text("beProfile.set.rowShape", &[]);
    let mut row = SetRow {
        cwd: String::new(),
        alias: String::new(),
        n: 0,
        tab_group: None,
        line,
    };
    let (mut cwd, mut alias, mut n) = (None, None, None);
    for (k, v) in t.iter() {
        match k {
            ROW_CWD => cwd = Some(v.as_str().ok_or_else(shape)?.to_string()),
            ROW_ALIAS => alias = Some(v.as_str().ok_or_else(shape)?.to_string()),
            ROW_N => n = Some(v.as_integer().ok_or_else(shape)?),
            ROW_TAB_GROUP => {
                let g = v.as_str().ok_or_else(shape)?;
                if g.trim().is_empty() {
                    return Err(copy_text("beProfile.set.tabGroup", &[]));
                }
                row.tab_group = Some(g.to_string());
            }
            other => return Err(copy_text("beProfile.set.rowKey", &[("key", other)])),
        }
    }
    let missing = |key: &str| copy_text("beProfile.set.rowMissing", &[("key", key)]);
    row.cwd = cwd.ok_or_else(|| missing(ROW_CWD))?;
    row.alias = alias.ok_or_else(|| missing(ROW_ALIAS))?;
    let n = n.ok_or_else(|| missing(ROW_N))?;
    if !super::pinned_anywhere(&row.cwd) {
        return Err(copy_text(
            "beProfile.value.notPinned",
            &[("key", ROW_CWD), ("value", &row.cwd)],
        ));
    }
    name_ok(&row.alias)?;
    row.n = u8::try_from(n)
        .ok()
        .filter(|n| (1..=ROW_MAX).contains(n))
        .ok_or_else(|| copy_text("beProfile.set.count", &[("max", &ROW_MAX.to_string())]))?;
    Ok(row)
}

/// 配置文件在这台机器上的路径。
pub fn path_in(home: &str) -> String {
    crate::common::door::join_under(home, relay_route_core::PROFILES_REL)
}

/// ccm 那一趟直接读（一次性模式不经文件管理面），合上内置那几段（`aliases::book_in`）：不在 ⇒ 只有内置的；读不了 ⇒ 那一句。
pub fn read_at(home: &str) -> Result<Book, String> {
    let path = path_in(home);
    if matches!(Ro::metadata(&path), Err(e) if e.kind() == std::io::ErrorKind::NotFound) {
        return Ok(super::book_in(home, None));
    }
    Ro::read_string_capped(&path, PROFILES_CAP)
        .map(|t| super::book_in(home, Some(&t)))
        .map_err(|e| {
            e.wrap(|why| copy_text("reason.fs.readFailed", &[("path", &path), ("why", why)]))
                .into_note()
        })
}

/// 读配置文件的上限（手写的几段 TOML，远不到这个量级）：超了这一趟整个失败、那一句照实交上去。
const PROFILES_CAP: cap_core::Cap = cap_core::Cap {
    bytes: 1024 * 1024,
    over: cap_core::Over::HardError,
};

/// 合并的成品：落好的意图 ＋ 每一项来自哪一层 ＋ 继承链（父 → 子）。
#[derive(Debug)]
pub struct Resolved {
    pub parsed: Parsed,
    pub origins: Vec<Origin>,
    pub chain: Vec<String>,
}

fn at_line(line: usize, said: String) -> String {
    copy_text(
        "beProfile.at.line",
        &[("line", &line.to_string()), ("why", &said)],
    )
}

/// `name` 的继承链（父 → 子）。查环、查「基于」的名字在不在、链上有没有写错的那一段。
pub(crate) fn chain<'a>(book: &'a Book, name: &str) -> Result<Vec<&'a Profile>, String> {
    if let Some(p) = book.file_problem() {
        return Err(at_line(p.line.unwrap_or(0), p.message.clone()));
    }
    let mut out: Vec<&Profile> = Vec::new();
    let mut cur = book
        .find(name)
        .ok_or_else(|| copy_text("beProfile.chain.unknown", &[("name", name)]))?;
    loop {
        if out.iter().any(|p| p.name == cur.name) {
            let mut names: Vec<&str> = out.iter().map(|p| p.name.as_str()).collect();
            names.push(&cur.name);
            return Err(copy_text(
                "beProfile.chain.cycle",
                &[(
                    "chain",
                    &names.join(&copy_text("beProfile.chain.arrow", &[])),
                )],
            ));
        }
        if let Some(p) = book.problem_of(&cur.name) {
            return Err(copy_text(
                "beProfile.chain.broken",
                &[
                    ("name", &cur.name),
                    (
                        "why",
                        &at_line(p.line.unwrap_or(cur.line), p.message.clone()),
                    ),
                ],
            ));
        }
        out.push(cur);
        let Some(f) = &cur.from else { break };
        cur = book.find(f).ok_or_else(|| {
            at_line(
                cur.from_line,
                copy_text(
                    "beProfile.chain.missingFrom",
                    &[("name", &cur.name), ("from", f)],
                ),
            )
        })?;
    }
    out.reverse();
    Ok(out)
}

/// **继承合并的入口**：ccm 起会话（`@名` / 被叫成那个名字）、`--ccm-print`、设置窗的合并预览都从这里拿。
/// `args` 是命令行当场给的（`[交给 agent 的…] -- [ccm 选项…]`），盖在合并结果最上面；合并规则只住 `argv::parse_layered`。
///
/// 〔继承表〕别名那一格：只认这一发自己的 `@名`，不随主会话。
pub fn resolve(book: &Book, name: &str, args: &[String]) -> Result<Resolved, String> {
    let chain = chain(book, name)?;
    let layers: Vec<Layer> = chain.iter().map(|p| p.layer()).collect();
    let (parsed, origins) = argv::parse_layered(&layers, args)
        .map_err(|Die(m)| copy_text("beProfile.resolve.refused", &[("name", name), ("why", &m)]))?;
    Ok(Resolved {
        parsed,
        origins,
        chain: chain.iter().map(|p| p.name.clone()).collect(),
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// 按条目改（保留手写的注释与排版）
// ═══════════════════════════════════════════════════════════════════════════

/// 改一段要写成的样子：名字 · 基于谁 · 交给 agent 的词 · ccm 选项（命令行 `--` 右边那种写法）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileEdit {
    pub name: String,
    pub from: Option<String>,
    pub agent: Vec<String>,
    pub ccm: Vec<String>,
}

/// 开始改：整份文本 ⇒ 可改的文档（TOML 写错 ⇒ 那一句，不改）。
pub(crate) fn edit_start(text: &str) -> Result<DocumentMut, String> {
    text.parse::<DocumentMut>().map_err(|e| {
        crate::common::said::Said::with_raw(
            copy_text("beProfile.file.syntax", &[]),
            e.message().trim(),
        )
        .into_note()
    })
}

/// ccm 选项 ⇒ 这一段要写的 `(键, 值)`，按第一次出现的顺序；同一个键写了几次：`--cwd-if` 并成数组的数组，其余后写的算。
fn items_of(ccm: &[String]) -> Result<Vec<(&'static str, Value)>, String> {
    let mut out: Vec<(&'static str, Value)> = Vec::new();
    let mut i = 0;
    while i < ccm.len() {
        let w = argv::word_at(ccm, i).map_err(|Die(m)| m)?;
        i += w.len;
        let f = PROFILE_FLAGS
            .iter()
            .copied()
            .find(|f| *f == w.flag)
            .ok_or_else(|| copy_text("beProfile.value.unknown", &[("key", key_of(w.flag))]))?;
        let k = key_of(f);
        let val: Value = match w.vals.as_slice() {
            [] => true.into(),
            [one] if f != flag::CWD_IF => one.as_str().into(),
            many => {
                let a: Array = many.iter().map(String::as_str).collect();
                if f == flag::CWD_IF {
                    let mut outer = Array::new();
                    outer.push(a);
                    outer.into()
                } else {
                    a.into()
                }
            }
        };
        match out.iter_mut().find(|(kk, _)| *kk == k) {
            Some((_, prev)) if f == flag::CWD_IF => {
                if let (Some(pa), Value::Array(na)) = (prev.as_array_mut(), val) {
                    for x in na.into_iter() {
                        pa.push_formatted(x);
                    }
                }
            }
            Some((_, prev)) => *prev = val,
            None => out.push((k, val)),
        }
    }
    Ok(out)
}

/// 新值与盘上那一项意思一样吗（意思一样就一个字节不动，连同它的行尾注释）。
fn same_meaning(key: &str, old: &Item, new: &Value) -> bool {
    let Some(f) = flag_of(key) else { return false };
    let new_item = Item::Value(new.clone());
    matches!((groups_of(key, f, old), groups_of(key, f, &new_item)), (Ok(a), Ok(b)) if a == b)
}

fn set_key(t: &mut Table, key: &str, v: Value) {
    match t.get_mut(key) {
        Some(slot) => {
            // 换值不换键：键的位置与前面的注释留着；值后面的行尾注释留着。
            let suffix = slot.as_value().and_then(|o| o.decor().suffix().cloned());
            let mut v = v.decorated(" ", "");
            if let Some(s) = suffix {
                v.decor_mut().set_suffix(s);
            }
            *slot = Item::Value(v);
        }
        None => {
            t.insert(key, Item::Value(v));
        }
    }
}

/// 把一段改成 `e` 那样（没有就加在末尾）。这一段里意思没变的项一个字节不动；别的段一个字节不动。
pub(crate) fn set_profile(doc: &mut DocumentMut, e: &ProfileEdit) -> Result<(), String> {
    name_ok(&e.name)?;
    let items = items_of(&e.ccm)?;
    let fresh = !doc.contains_key(&e.name);
    if fresh {
        let mut t = Table::new();
        if !doc.as_table().is_empty() {
            t.decor_mut().set_prefix("\n");
        }
        doc.insert(&e.name, Item::Table(t));
    }
    let t = doc
        .get_mut(&e.name)
        .and_then(Item::as_table_mut)
        .ok_or_else(|| copy_text("beProfile.file.topLevel", &[("key", &e.name)]))?;
    // 不再有的项删掉。
    let keep: Vec<String> = t
        .iter()
        .map(|(k, _)| k.to_string())
        .filter(|k| match k.as_str() {
            FROM_KEY => e.from.is_some(),
            ARGS_KEY => !e.agent.is_empty(),
            k => items.iter().any(|(kk, _)| *kk == k),
        })
        .collect();
    let gone: Vec<String> = t
        .iter()
        .map(|(k, _)| k.to_string())
        .filter(|k| !keep.contains(k))
        .collect();
    for k in gone {
        t.remove(&k);
    }
    if let Some(f) = &e.from {
        if t.get(FROM_KEY).and_then(Item::as_str) != Some(f.as_str()) {
            set_key(t, FROM_KEY, f.as_str().into());
        }
    }
    if !e.agent.is_empty() {
        let same = t.get(ARGS_KEY).and_then(Item::as_array).is_some_and(|a| {
            a.iter().map(|v| v.as_str()).collect::<Vec<_>>()
                == e.agent.iter().map(|s| Some(s.as_str())).collect::<Vec<_>>()
        });
        if !same {
            let a: Array = e.agent.iter().map(String::as_str).collect();
            set_key(t, ARGS_KEY, a.into());
        }
    }
    for (k, v) in items {
        if t.get(k).is_some_and(|old| same_meaning(k, old, &v)) {
            continue;
        }
        set_key(t, k, v);
    }
    Ok(())
}

/// 删一段（连同它前面那几行注释）。别的段一个字节不动。
pub(crate) fn remove_profile(doc: &mut DocumentMut, name: &str) {
    doc.remove(name);
}

/// 一段原地改名：键换掉，位置 · 段头前的注释 · 各项一个字节不动。新名字已有 ⇒ 那一句。
pub(crate) fn rename_profile(doc: &mut DocumentMut, from: &str, to: &str) -> Result<(), String> {
    name_ok(to)?;
    if doc.contains_key(to) {
        return Err(copy_text("addMachine.err.taken", &[("name", to)]));
    }
    let (key, item) = doc
        .as_table_mut()
        .remove_entry(from)
        .ok_or_else(|| copy_text("beProfile.chain.unknown", &[("name", from)]))?;
    let mut new_key = toml_edit::Key::new(to);
    *new_key.leaf_decor_mut() = key.leaf_decor().clone();
    doc.as_table_mut().insert_formatted(&new_key, item);
    Ok(())
}

/// 只改一段的「基于」。
pub(crate) fn set_from(
    doc: &mut DocumentMut,
    name: &str,
    from: Option<&str>,
) -> Result<(), String> {
    let t = doc
        .get_mut(name)
        .and_then(Item::as_table_mut)
        .ok_or_else(|| copy_text("beProfile.chain.unknown", &[("name", name)]))?;
    match from {
        Some(f) => set_key(t, FROM_KEY, f.into()),
        None => {
            t.remove(FROM_KEY);
        }
    }
    Ok(())
}

/// 一组要写成的样子：名字 ＋ 几行。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetEdit {
    pub name: String,
    pub rows: Vec<RowEdit>,
}

/// 一行要写成的样子（`tab_group` 只在改过组名时给）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RowEdit {
    pub cwd: String,
    pub alias: String,
    pub n: u8,
    #[serde(default)]
    pub tab_group: Option<String>,
}

/// 这一组自己写的样子（框的回填从这里取）。
pub fn edit_of_set(s: &Set) -> SetEdit {
    SetEdit {
        name: s.name.clone(),
        rows: s
            .rows
            .iter()
            .map(|r| RowEdit {
                cwd: r.cwd.clone(),
                alias: r.alias.clone(),
                n: r.n,
                tab_group: r.tab_group.clone(),
            })
            .collect(),
    }
}

/// 几行 ⇒ `row` 那个值：一行一张行内表，一行一个、末尾带逗号（手改好读）。
fn rows_value(rows: &[RowEdit]) -> Value {
    let mut a = Array::new();
    for r in rows {
        let mut t = toml_edit::InlineTable::new();
        t.insert(ROW_CWD, r.cwd.as_str().into());
        t.insert(ROW_ALIAS, r.alias.as_str().into());
        t.insert(ROW_N, i64::from(r.n).into());
        if let Some(g) = &r.tab_group {
            t.insert(ROW_TAB_GROUP, g.as_str().into());
        }
        t.fmt();
        a.push_formatted(Value::InlineTable(t).decorated("\n  ", ""));
    }
    a.set_trailing_comma(true);
    a.set_trailing("\n");
    a.into()
}

/// 把一组改成 `e` 那样（没有就加在末尾）。几行意思没变 ⇒ 一个字节不动；别的段与组一个字节不动。
pub(crate) fn set_set(doc: &mut DocumentMut, e: &SetEdit) -> Result<(), String> {
    if e.name.trim().is_empty() {
        return Err(copy_text("beProfile.set.name", &[]));
    }
    let fresh_root = !doc.contains_key(SET_KEY);
    if fresh_root {
        let mut t = Table::new();
        t.set_implicit(true);
        doc.insert(SET_KEY, Item::Table(t));
    }
    let nonempty = doc.as_table().iter().any(|(k, _)| k != SET_KEY);
    let root = doc
        .get_mut(SET_KEY)
        .and_then(Item::as_table_mut)
        .ok_or_else(|| copy_text("beProfile.set.shape", &[]))?;
    if !root.contains_key(&e.name) {
        let mut t = Table::new();
        if nonempty || !root.is_empty() {
            t.decor_mut().set_prefix("\n");
        }
        root.insert(&e.name, Item::Table(t));
    }
    let t = root
        .get_mut(&e.name)
        .and_then(Item::as_table_mut)
        .ok_or_else(|| copy_text("beProfile.set.shape", &[]))?;
    // 盘上那几行意思与要写的一样 ⇒ 一个字节不动（连同手写的排版）。
    let same = t.get(ROW_KEY).and_then(Item::as_array).is_some_and(|a| {
        let rows: Option<Vec<SetRow>> = a
            .iter()
            .map(|x| x.as_inline_table().and_then(|r| row_of(r, 0).ok()))
            .collect();
        rows.is_some_and(|rows| {
            let set = Set {
                name: e.name.clone(),
                line: 0,
                rows,
                problems: Vec::new(),
            };
            edit_of_set(&set).rows == e.rows
        })
    });
    if !same {
        let keep_suffix = t
            .get(ROW_KEY)
            .and_then(Item::as_value)
            .and_then(|o| o.decor().suffix().cloned());
        let mut v = rows_value(&e.rows).decorated(" ", "");
        if let Some(s) = keep_suffix {
            v.decor_mut().set_suffix(s);
        }
        t.insert(ROW_KEY, Item::Value(v));
    }
    Ok(())
}

/// 删一组；`set` 那一节空了连它一起删。别的段与组一个字节不动。
pub(crate) fn remove_set(doc: &mut DocumentMut, name: &str) {
    let empty = match doc.get_mut(SET_KEY).and_then(Item::as_table_mut) {
        Some(root) => {
            root.remove(name);
            root.is_empty()
        }
        None => return,
    };
    if empty {
        doc.remove(SET_KEY);
    }
}

/// 对配置文件的一处改动（设置窗存一下、建号删号那一刻、迁移、存一组，都折成这几种）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// 存一组（没有就加在末尾）。
    SetSet(SetEdit),
    /// 删一组。
    RemoveSet(String),
    Set(ProfileEdit),
    Remove(String),
    /// 改名：那一段原地换个名字（位置、注释、各项一个字节不动）。
    Rename {
        from: String,
        to: String,
    },
    /// 只改一段的「基于」（`None` ＝ 不基于谁）；别的项一个字节不动。
    SetFrom {
        name: String,
        from: Option<String>,
    },
}

/// 在 `text`（不在 ⇒ 新建，开头带一段说明注释）上依次做 `changes`，回改完的全文。没动的段一个字节不动。
pub(crate) fn apply_changes(text: Option<&str>, changes: &[Change]) -> Result<String, String> {
    let header = || {
        copy_text("beProfile.file.header", &[])
            .lines()
            .map(|l| format!("# {l}\n"))
            .collect::<String>()
    };
    let mut doc = edit_start(text.map_or_else(header, str::to_string).as_str())?;
    for c in changes {
        match c {
            Change::Set(e) => set_profile(&mut doc, e)?,
            Change::Remove(n) => remove_profile(&mut doc, n),
            Change::Rename { from, to } => rename_profile(&mut doc, from, to)?,
            Change::SetFrom { name, from } => set_from(&mut doc, name, from.as_deref())?,
            Change::SetSet(e) => set_set(&mut doc, e)?,
            Change::RemoveSet(n) => remove_set(&mut doc, n),
        }
    }
    Ok(doc.to_string())
}

/// 改完之后整份还合格吗：每一处写错、每一段合不下来的，都是一句（带段名）。空 ＝ 合格（判据的对照）。
#[cfg(test)]
pub(crate) fn problems_after(text: &str) -> Vec<String> {
    let b = parse_book(text);
    let mut out: Vec<String> = b
        .problems
        .iter()
        .map(|p| {
            let said = p
                .line
                .map_or(p.message.clone(), |l| at_line(l, p.message.clone()));
            match &p.profile {
                Some(n) => copy_text("beProfile.named.say", &[("name", n), ("said", &said)]),
                None => said,
            }
        })
        .collect();
    if out.is_empty() {
        for p in &b.profiles {
            if let Err(e) = resolve(&b, &p.name, &[]) {
                out.push(e);
            }
        }
    }
    for s in &b.sets {
        for p in &s.problems {
            let said = p
                .line
                .map_or(p.message.clone(), |l| at_line(l, p.message.clone()));
            out.push(copy_text(
                "beProfile.named.say",
                &[("name", &s.name), ("said", &said)],
            ));
        }
    }
    out
}

/// 基于 `name` 的那几段（直接写 `from = name` 的）。
pub(crate) fn children_of<'a>(book: &'a Book, name: &str) -> Vec<&'a str> {
    book.profiles
        .iter()
        .filter(|p| p.from.as_deref() == Some(name))
        .map(|p| p.name.as_str())
        .collect()
}

#[cfg(test)]
#[path = "../../../../tests/backend/assets/aliases/profile_tests.rs"]
mod tests;
