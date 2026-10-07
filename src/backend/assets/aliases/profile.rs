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
//! - **同一套词**：键就是 ccm 选项 `--` 后面那个词（[`PROFILE_KEYS`]），值按「开关写 `true` · 一个值写字符串 ·
//!   几个值写字符串数组（`--cwd-if` 可写成数组的数组，一条一组）」变回命令行那几个词，交 ccm 自己的解析器（`argv::word_at`）读。
//!   只在这里有、命令行上没有的只有两项：`from`（＝ 命令行上的 `@名`）与 `args`（＝ 命令行上 `--` 左边交给 agent 的那串）。
//! - **先后**：继承链父 → 子，命令行当场给的最后盖；每一格怎么盖只住 `argv::parse_layered`。
//! - **可以手改**：每次用都读、校验；写错报到哪一行，写错的那一段（与基于它的）此刻不能用，别的照用。
//! - **按条目改**：设置窗改一段只动那一段（[`set_profile`] / [`remove_profile`]），手写的注释与排版留着。

use copy_core::copy_text;
use toml_edit::{Array, DocumentMut, Item, Table, Value};

use crate::control::ccm::argv::{self, flag, Die, Layer, Origin, Parsed};

/// 「基于」那一项的键（命令行上是 `@名`）。
pub(crate) const FROM_KEY: &str = "from";
/// 交给 agent 的那几个词（命令行上 `--` 左边那串）。
pub(crate) const ARGS_KEY: &str = "args";

/// 能写进配置的 ccm 词。不收的（`new` · `--attach` · `--ccm-*` 诊断口 · `--ccm-sid` · `--account-dir`）每次取值都不同或是一次性的动作，
/// 写成固定配置没意义；它们照样能在命令行 `--` 右边当场给。
pub(crate) const PROFILE_FLAGS: &[&str] = &[
    flag::CWD,
    flag::CWD_IF,
    flag::ACCOUNT,
    flag::BASE,
    flag::TMUX,
    flag::TMUX_BASE,
    flag::TMUX_SIZE,
    flag::DETACH,
    flag::AGENT,
    flag::LAUNCHER,
    flag::BUS_REGISTER,
    flag::BUS_NOTE,
];

/// [`PROFILE_FLAGS`] 写进文件的那个键（去掉打头的 `--`）。
pub(crate) static PROFILE_KEYS: std::sync::LazyLock<Vec<&'static str>> =
    std::sync::LazyLock::new(|| PROFILE_FLAGS.iter().map(|f| key_of(f)).collect());

fn key_of(f: &'static str) -> &'static str {
    f.trim_start_matches('-')
}

pub(crate) fn flag_of(key: &str) -> Option<&'static str> {
    PROFILE_FLAGS.iter().copied().find(|f| key_of(f) == key)
}

/// 配置文件里写的一项：键 ＋ 变回的命令行词（一组一个 `Vec`；`--cwd-if` 可以几组）＋ 行号。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Setting {
    pub key: String,
    pub groups: Vec<Vec<String>>,
    pub line: usize,
}

/// 一段（一份具名配置）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Profile {
    pub name: String,
    /// 段头那一行。
    pub line: usize,
    pub from: Option<String>,
    /// `from` 那一行（报「基于的不存在」用）。
    pub from_line: usize,
    pub agent: Vec<String>,
    pub items: Vec<Setting>,
}

impl Profile {
    /// 这一段自己那几项，写成命令行 `--` 右边的词。
    pub(crate) fn ccm_words(&self) -> Vec<String> {
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

/// 一处写错：哪一段（`None` ＝ 整份文件）· 第几行 · 什么错。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Problem {
    pub profile: Option<String>,
    pub line: Option<usize>,
    pub message: String,
}

/// 读出来的整份：认得的几段 ＋ 写错的几处。TOML 本身写错 ⇒ 一段都不认（整份不能用）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Book {
    pub profiles: Vec<Profile>,
    pub problems: Vec<Problem>,
}

impl Book {
    pub(crate) fn find(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    fn problem_of(&self, name: &str) -> Option<&Problem> {
        self.problems
            .iter()
            .find(|p| p.profile.as_deref() == Some(name))
    }

    /// 整份文件级的错（TOML 写错）。
    fn file_problem(&self) -> Option<&Problem> {
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

/// 这个名字能不能当一段配置的名字：两种 shell 都认得的命令名，而且不是 ccm 自己（[`crate::control::ccm::is_own_name`]）。
pub(crate) fn name_ok(name: &str) -> Result<(), String> {
    use crate::platform::shell::dialect::Shell;
    if crate::control::ccm::is_own_name(name) {
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
    copy_text("beProfile.value.parser", &[("key", key), ("e", m)])
}

/// **读口**：整份文本 ⇒ [`Book`]。一个字节的盘都不碰。
pub(crate) fn parse_book(text: &str) -> Book {
    let doc = match toml_edit::Document::parse(text) {
        Ok(d) => d,
        Err(e) => {
            return Book {
                profiles: Vec::new(),
                problems: vec![Problem {
                    profile: None,
                    line: e.span().map(|s| line_at(text, s.start)),
                    message: copy_text("beProfile.file.syntax", &[("e", e.message().trim())]),
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
        let mut problem = |profile: Option<&str>, line: usize, message: String| {
            book.problems.push(Problem {
                profile: profile.map(str::to_string),
                line: Some(line),
                message,
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

/// 配置文件在这台机器上的路径。
pub(crate) fn path_in(home: &str) -> String {
    crate::assets::door::join_under(home, relay_route_core::PROFILES_REL)
}

/// ccm 那一趟直接读（一次性模式不经文件管理面）：不在 ⇒ 一段都没有；读不了 ⇒ 那一句。
pub(crate) fn read_at(home: &str) -> Result<Book, String> {
    let path = path_in(home);
    match std::fs::read_to_string(&path) {
        Ok(t) => Ok(parse_book(&t)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Book::default()),
        Err(e) => Err(copy_text(
            "beProfile.file.unreadable",
            &[("path", &path), ("e", &e.to_string())],
        )),
    }
}

/// 合并的成品：落好的意图 ＋ 每一项来自哪一层 ＋ 继承链（父 → 子）。
#[derive(Debug)]
pub(crate) struct Resolved {
    pub parsed: Parsed,
    pub origins: Vec<Origin>,
    pub chain: Vec<String>,
}

fn at_line(line: usize, said: String) -> String {
    copy_text(
        "beProfile.at.line",
        &[("line", &line.to_string()), ("e", &said)],
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
                    ("e", &at_line(p.line.unwrap_or(cur.line), p.message.clone())),
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
pub(crate) fn resolve(book: &Book, name: &str, args: &[String]) -> Result<Resolved, String> {
    let chain = chain(book, name)?;
    let layers: Vec<Layer> = chain.iter().map(|p| p.layer()).collect();
    let (parsed, origins) = argv::parse_layered(&layers, args)
        .map_err(|Die(m)| copy_text("beProfile.resolve.refused", &[("name", name), ("e", &m)]))?;
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
pub(crate) struct ProfileEdit {
    pub name: String,
    pub from: Option<String>,
    pub agent: Vec<String>,
    pub ccm: Vec<String>,
}

/// 开始改：整份文本 ⇒ 可改的文档（TOML 写错 ⇒ 那一句，不改）。
pub(crate) fn edit_start(text: &str) -> Result<DocumentMut, String> {
    text.parse::<DocumentMut>()
        .map_err(|e| copy_text("beProfile.file.syntax", &[("e", e.message().trim())]))
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
        return Err(copy_text("beProfile.rename.taken", &[("name", to)]));
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

/// 对配置文件的一处改动（设置窗存一下、建号删号那一刻、迁移，都折成这几种）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Change {
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
    let mut doc = edit_start(text.map_or_else(|| header(), str::to_string).as_str())?;
    for c in changes {
        match c {
            Change::Set(e) => set_profile(&mut doc, e)?,
            Change::Remove(n) => remove_profile(&mut doc, n),
            Change::Rename { from, to } => rename_profile(&mut doc, from, to)?,
            Change::SetFrom { name, from } => set_from(&mut doc, name, from.as_deref())?,
        }
    }
    Ok(doc.to_string())
}

/// 改完之后整份还合格吗：每一处写错、每一段合不下来的，都是一句（带段名）。空 ＝ 合格。
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
    out
}

/// 一段单独拿出来逐项合不合格：回第一处写错的原话（不带段名与行号）。几项之间怎么组合要连「基于」那一层一起看，不在这里判
/// （整份判见 [`problems_after`]）。
pub(crate) fn check_alone(e: &ProfileEdit) -> Result<(), String> {
    let alone = ProfileEdit {
        from: None,
        ..e.clone()
    };
    let text = apply_changes(Some(""), &[Change::Set(alone)])?;
    match parse_book(&text).problems.first() {
        Some(p) => Err(p.message.clone()),
        None => Ok(()),
    }
}

/// 基于 `name` 的那几段（直接写 `from = name` 的）。
pub(crate) fn children_of<'a>(book: &'a Book, name: &str) -> Vec<&'a str> {
    book.profiles
        .iter()
        .filter(|p| p.from.as_deref() == Some(name))
        .map(|p| p.name.as_str())
        .collect()
}

/// 这一段自己写的样子（设置窗的表单回填、旧帧那条清单都从这里取）。
pub(crate) fn edit_of(p: &Profile) -> ProfileEdit {
    ProfileEdit {
        name: p.name.clone(),
        from: p.from.clone(),
        agent: p.agent.clone(),
        ccm: p.ccm_words(),
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/assets/aliases/profile_tests.rs"]
mod tests;
