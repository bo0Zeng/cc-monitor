//! **Everything 式搜索词** —— `files-find` 的 `query` 只在这里被读懂（窗口发原样的字，不解析）。
//!
//! 认得的写法：空格 ＝ 且 · `|` ／ `OR` ＝ 或（比空格结合得紧）· `!` ／ `NOT` ＝ 非 · `< >` ／ `( )` 分组 ·
//! `"…"` 里的空格与符号照原样 · `*` `?` 通配（带通配的词要对上**整个**名字）· `ext:a;b` · `path:` · `file:` ／ `folder:`。
//! 默认不分大小写（ASCII 逐字节折；词里有带大小写的非 ASCII 字母时整段按 Unicode 小写比）。
//! 词里带路径分隔符 ⇒ 对全路径，否则只对名字（最后一段）；`path:` 强制对全路径。
//! 别的 Everything 过滤器（`size:` `dm:` `regex:` …）⇒ 拒，说出是哪一个；认不出的 `xx:`（如 `12:30`）当普通字。
//! 打到一半的引号 ／ 括号在末尾自动收口（一敲就出：不因为还没打完就报错）。
//!
//! 解析器（`Parser` 那一段与它的几个小工具）改自 Cardinal 的 `cardinal-syntax`
//! （<https://github.com/cardisoft/cardinal>），许可原文照录：
//!
//! ```text
//! MIT License
//!
//! Copyright (c) 2025 Cardinal Contributors
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.
//! ```
//!
//! 改动：过滤器只留上面那几种（其余认得名字的拒）· 正则、范围、比较那几形删了 · 末尾没收口的引号与分组不报错 ·
//! 匹配那一半是本仓自己写的（索引是一摞全路径 ＋ 每条一个类型字节，逐条判）。

use std::cell::OnceCell;

/// 索引里每条的类型字节（`files-ls` 那四个词的同一个闭集）。
pub const KIND_FILE: u8 = 0;
pub const KIND_DIR: u8 = 1;
pub const KIND_SYMLINK: u8 = 2;
pub const KIND_OTHER: u8 = 3;

/// 目录项类型 → 类型字节。不跟链接（喂 `DirEntry::file_type` 那一份的三个判断）。
pub fn kind_of(is_dir: bool, is_symlink: bool, is_file: bool) -> u8 {
    if is_dir {
        KIND_DIR
    } else if is_symlink {
        KIND_SYMLINK
    } else if is_file {
        KIND_FILE
    } else {
        KIND_OTHER
    }
}

/// 解析失败的那几种。界面上说的话由 [`QueryError::said`] 出。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryError {
    /// 一个没有配对的右括号（`)` 或 `>`），`at` 是第几个字符（从 1 数）。
    StrayCloser { ch: char, at: usize },
    /// 认得名字、但这里不支持的过滤器（`size:` 之类）。
    Unsupported { name: String },
}

impl QueryError {
    pub fn said(&self) -> String {
        match self {
            Self::StrayCloser { ch, at } => copy_core::copy_text(
                "beFilesRead.find.strayCloser",
                &[("ch", &ch.to_string()), ("at", &at.to_string())],
            ),
            Self::Unsupported { name } => {
                copy_core::copy_text("beFilesRead.find.unsupported", &[("name", name)])
            }
        }
    }
}

/// 解析好、编译好的一条搜索词。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Matcher {
    root: Node,
    /// 排相关度用的那几个词：不在「非」底下、对名字比、不带通配的。
    terms: Vec<Text>,
}

/// 解析一条搜索词。空白 ⇒ 匹配一切。
pub fn parse(input: &str) -> Result<Matcher, QueryError> {
    let expr = Parser::new(input).parse()?;
    let root = compile(&expr)?;
    let mut terms = Vec::new();
    name_terms(&root, &mut terms);
    Ok(Matcher { root, terms })
}

/// 收那几个排相关度的词（「非」底下的不算：名字里没有它才中）。
fn name_terms(n: &Node, out: &mut Vec<Text>) {
    match n {
        Node::And(parts) | Node::Or(parts) => parts.iter().for_each(|p| name_terms(p, out)),
        Node::Text(t) if !t.on_path && !t.glob => out.push(t.clone()),
        _ => {}
    }
}

impl Matcher {
    /// 匹配一切（空白的搜索词）。
    pub fn is_all(&self) -> bool {
        matches!(self.root, Node::All)
    }

    /// 这一条（全路径原始字节 ＋ 类型字节）中不中。
    pub fn matches(&self, path: &[u8], kind: u8) -> bool {
        let c = Cand {
            path,
            name: name_of(path),
            kind,
            low_name: OnceCell::new(),
            low_path: OnceCell::new(),
        };
        c.eval(&self.root)
    }

    /// 相关度档（小的在前）：名字与某个词一样 0 ＞ 名字以它开头 1 ＞ 名字里有它 2 ＞ 别的 3。
    pub fn rank(&self, path: &[u8]) -> u8 {
        let name = name_of(path);
        let mut low: Option<Vec<u8>> = None;
        let mut best = 3u8;
        for t in &self.terms {
            let hay: &[u8] = if t.unicode {
                low.get_or_insert_with(|| lower(name))
            } else {
                name
            };
            let pat = t.pat.as_slice();
            let r = if hay.eq_ignore_ascii_case(pat) {
                0
            } else if hay.len() >= pat.len() && hay[..pat.len()].eq_ignore_ascii_case(pat) {
                1
            } else if contains_ci(hay, pat) {
                2
            } else {
                3
            };
            best = best.min(r);
        }
        best
    }

    /// 名字里被那几个排相关度的词对上的字节区间（起 · 止，不重叠、升序）—— 界面给这几段加底色。
    /// 按 Unicode 小写比的词只在小写后长度不变时给（区间才对得回原名字）。
    pub fn marks(&self, path: &[u8]) -> Vec<(usize, usize)> {
        let name = name_of(path);
        let low = lower(name);
        let mut out: Vec<(usize, usize)> = Vec::new();
        for t in &self.terms {
            let pat = t.pat.as_slice();
            if pat.is_empty() {
                continue;
            }
            let hay: &[u8] = if t.unicode {
                if low.len() != name.len() {
                    continue;
                }
                &low
            } else {
                name
            };
            let mut i = 0;
            while i + pat.len() <= hay.len() {
                if hay[i..i + pat.len()].eq_ignore_ascii_case(pat) {
                    out.push((i, i + pat.len()));
                    i += pat.len();
                } else {
                    i += 1;
                }
            }
        }
        out.sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for (a, b) in out {
            match merged.last_mut() {
                Some(last) if a <= last.1 => last.1 = last.1.max(b),
                _ => merged.push((a, b)),
            }
        }
        merged
    }
}

// ══════════════════════ 解析（改自 cardinal-syntax） ══════════════════════

#[derive(Debug, Clone, PartialEq, Eq)]
enum Expr {
    Empty,
    Term(Term),
    Not(Box<Expr>),
    And(Vec<Expr>),
    Or(Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Term {
    Word(String),
    Filter(FilterKind, Option<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FilterKind {
    File,
    Folder,
    Ext,
    Path,
    /// 认得名字、不支持。
    Unsupported(String),
    /// 认不出的名字 ⇒ 整段当普通字。
    Custom(String),
}

/// Everything 认得、这里不支持的过滤器名（拒的时候说得出是哪一个；其余 `xx:` 当普通字）。
const UNSUPPORTED_FILTERS: &[&str] = &[
    "album",
    "artist",
    "attrib",
    "attribdupe",
    "audio",
    "bitdepth",
    "case",
    "child",
    "comment",
    "content",
    "da",
    "dateaccessed",
    "datecreated",
    "datemodified",
    "daterun",
    "dc",
    "dimensions",
    "dm",
    "dmdupe",
    "doc",
    "dr",
    "dupe",
    "exe",
    "genre",
    "height",
    "in",
    "infolder",
    "namepartdupe",
    "nosubfolders",
    "nowholefilename",
    "orientation",
    "parent",
    "regex",
    "size",
    "sizedupe",
    "t",
    "tag",
    "title",
    "track",
    "type",
    "video",
    "width",
    "year",
];

impl FilterKind {
    fn from_name(name: &str) -> Self {
        let lower = name.to_ascii_lowercase();
        match lower.as_str() {
            "file" => FilterKind::File,
            "folder" => FilterKind::Folder,
            "ext" => FilterKind::Ext,
            "path" => FilterKind::Path,
            l if UNSUPPORTED_FILTERS.contains(&l) => FilterKind::Unsupported(lower),
            _ => FilterKind::Custom(name.to_string()),
        }
    }
}

/// 手写的递归下降（同原作者：语言很小，空白、布尔、过滤器要逐字照 Everything 的规矩）。
struct Parser<'a> {
    input: &'a str,
    pos: usize,
    group_stack: Vec<char>,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            pos: 0,
            group_stack: Vec::new(),
        }
    }

    fn parse(mut self) -> Result<Expr, QueryError> {
        let expr = self.parse_and()?;
        self.skip_ws();
        if let Some(ch) = self.peek_char() {
            return Err(self.stray(ch));
        }
        Ok(expr)
    }

    // 且：优先级最低，空白隐式连接；收成一个 Vec，不嵌二叉。
    fn parse_and(&mut self) -> Result<Expr, QueryError> {
        let mut parts = Vec::new();
        loop {
            self.skip_ws();
            if self.consume_keyword("AND") {
                continue;
            }
            if self.eof() || self.is_at_group_close() {
                break;
            }
            let before = self.pos;
            let expr = self.parse_or()?;
            if matches!(expr, Expr::Empty) {
                // 空的一项（如 `""`）跳过；一个字没吃 ⇒ 收（防空转）。
                if self.pos == before {
                    break;
                }
                continue;
            }
            parts.push(expr);
        }
        match parts.len() {
            0 => Ok(Expr::Empty),
            1 => Ok(parts.remove(0)),
            _ => Ok(Expr::And(parts)),
        }
    }

    // 或：比且结合得紧，自左向右收。空的一臂 ＝ 匹配一切（同原作者：空即全集）。
    fn parse_or(&mut self) -> Result<Expr, QueryError> {
        let mut parts = Vec::new();
        loop {
            self.skip_ws();
            let operand_is_empty =
                self.peek_char() == Some('|') || self.eof() || self.is_at_group_close();
            if operand_is_empty {
                parts.push(Expr::Empty);
            } else {
                parts.push(self.parse_not()?);
            }
            self.skip_ws();
            let matched = if self.peek_char() == Some('|') {
                self.advance_char();
                true
            } else {
                self.consume_keyword("OR")
            };
            if !matched {
                break;
            }
        }
        if parts.len() == 1 {
            Ok(parts.remove(0))
        } else {
            Ok(Expr::Or(parts))
        }
    }

    // 非：比且 ／ 或都紧；`!!!foo` 按奇偶收成一层。
    fn parse_not(&mut self) -> Result<Expr, QueryError> {
        let mut negations = 0;
        loop {
            self.skip_ws();
            if self.peek_char() == Some('!') {
                self.advance_char();
                negations += 1;
                continue;
            }
            if self.consume_keyword("NOT") {
                negations += 1;
                continue;
            }
            break;
        }
        let mut expr = self.parse_primary()?;
        if negations % 2 == 1 {
            expr = Expr::Not(Box::new(expr));
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, QueryError> {
        self.skip_ws();
        let Some(ch) = self.peek_char() else {
            return Ok(Expr::Empty);
        };
        match ch {
            '<' => self.parse_group('>'),
            '(' => self.parse_group(')'),
            '>' | ')' => Err(self.stray(ch)),
            _ => {
                let term = self.parse_word_like()?;
                match &term {
                    Term::Word(text) if text == "\"\"" => Ok(Expr::Empty),
                    _ => Ok(Expr::Term(term)),
                }
            }
        }
    }

    fn parse_group(&mut self, closing: char) -> Result<Expr, QueryError> {
        self.advance_char();
        self.group_stack.push(closing);
        let expr = self.parse_and()?;
        self.group_stack.pop();
        self.skip_ws();
        if self.peek_char() == Some(closing) {
            self.advance_char();
        }
        // 走到末尾还没收口 ⇒ 当它收了（打到一半）。
        Ok(expr)
    }

    // 一个词，或 `name:` 打头的过滤器；见到 `:` 就停，别把参数吃进来。
    fn parse_word_like(&mut self) -> Result<Term, QueryError> {
        let start = self.pos;
        let mut seen = false;
        while let Some(ch) = self.peek_char() {
            if ch == '\\' && self.peek_next_char() == Some('"') {
                seen = true;
                self.advance_char();
                self.advance_char();
                continue;
            }
            if ch == '"' {
                self.advance_char();
                let mut escaped = false;
                while let Some(next) = self.peek_char() {
                    self.advance_char();
                    if escaped {
                        escaped = false;
                        continue;
                    }
                    if next == '\\' {
                        escaped = true;
                        continue;
                    }
                    if next == '"' {
                        break;
                    }
                }
                seen = true;
                continue;
            }
            if ch == ':' && seen {
                let name = &self.input[start..self.pos];
                if is_valid_filter_name(name) {
                    self.advance_char();
                    return self.parse_filter_term(name.to_string());
                }
            }
            if is_term_breaker(ch) {
                break;
            }
            seen = true;
            self.advance_char();
        }
        if self.pos == start {
            // 落不进任何一档的单个字符：当普通字吃掉，保证往前走。
            self.advance_char();
        }
        Ok(Term::Word(self.input[start..self.pos].to_string()))
    }

    fn parse_filter_term(&mut self, name: String) -> Result<Term, QueryError> {
        let kind = FilterKind::from_name(&name);
        let argument = self.parse_filter_argument();
        Ok(Term::Filter(kind, argument))
    }

    // `name:` 后面紧跟的参数：到空白 ／ `|` ／ 本组的右括号为止；`name: x`（中间有空白）⇒ 没有参数。
    fn parse_filter_argument(&mut self) -> Option<String> {
        if self.eof()
            || self.peek_char().is_some_and(char::is_whitespace)
            || self.starts_with_filter_token()
            || self.is_at_group_close()
            || self.peek_char() == Some('|')
        {
            return None;
        }
        let mut buffer = String::new();
        while let Some(ch) = self.peek_char() {
            if ch == '"' {
                let quoted = self.parse_phrase_string();
                buffer.push('"');
                buffer.push_str(&quoted);
                buffer.push('"');
                continue;
            }
            if ch.is_whitespace() || ch == '|' || self.current_closer_is(ch) {
                break;
            }
            buffer.push(ch);
            self.advance_char();
        }
        (!buffer.is_empty() && buffer != "\"\"").then_some(buffer)
    }

    // 引号里原样（`\"` 不收口）；走到末尾没收口 ⇒ 当它收了。
    fn parse_phrase_string(&mut self) -> String {
        self.advance_char();
        let mut result = String::new();
        let mut escaped = false;
        while let Some(ch) = self.peek_char() {
            self.advance_char();
            if escaped {
                escaped = false;
                result.push(ch);
                continue;
            }
            if ch == '\\' {
                escaped = true;
                result.push(ch);
                continue;
            }
            if ch == '"' {
                break;
            }
            result.push(ch);
        }
        result
    }

    fn skip_ws(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace() {
                self.advance_char();
            } else {
                break;
            }
        }
    }

    fn consume_keyword(&mut self, keyword: &str) -> bool {
        let rest = self.remaining();
        if rest.len() < keyword.len() || !rest.is_char_boundary(keyword.len()) {
            return false;
        }
        if !rest[..keyword.len()].eq_ignore_ascii_case(keyword) {
            return false;
        }
        if let Some(next) = rest[keyword.len()..].chars().next() {
            if !is_keyword_boundary_char(next) {
                return false;
            }
        }
        self.pos += keyword.len();
        true
    }

    fn current_closer_is(&self, ch: char) -> bool {
        matches!(self.group_stack.last(), Some(&closer) if closer == ch)
    }

    fn is_at_group_close(&self) -> bool {
        matches!((self.group_stack.last(), self.peek_char()), (Some(&closer), Some(ch)) if closer == ch)
    }

    fn remaining(&self) -> &'a str {
        &self.input[self.pos..]
    }

    fn peek_char(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    fn peek_next_char(&self) -> Option<char> {
        let mut chars = self.remaining().chars();
        chars.next();
        chars.next()
    }

    fn advance_char(&mut self) {
        if let Some(ch) = self.peek_char() {
            self.pos += ch.len_utf8();
        }
    }

    fn eof(&self) -> bool {
        self.pos >= self.input.len()
    }

    fn stray(&self, ch: char) -> QueryError {
        QueryError::StrayCloser {
            ch,
            at: self.input[..self.pos].chars().count() + 1,
        }
    }

    // 光标是不是正停在下一个 `name:` 上（`video: size:>1gb` 那一形：第二个不是第一个的参数）。
    fn starts_with_filter_token(&self) -> bool {
        let rest = self.remaining();
        for (idx, ch) in rest.char_indices() {
            if ch.is_whitespace() || is_term_breaker(ch) {
                break;
            }
            if ch == ':' {
                return idx != 0 && is_valid_filter_name(&rest[..idx]);
            }
        }
        false
    }
}

fn is_term_breaker(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '|' | '<' | '>' | '(' | ')' | '!')
}

fn is_keyword_boundary_char(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '|' | '<' | '>' | '(' | ')' | '!' | ':' | '"' | '\\' | '/' | ';' | ',' | '.'
        )
        || matches!(ch, '[' | ']' | '{' | '}' | '#')
        || ch == '\0'
}

fn is_valid_filter_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

/// `ext:` 的 `a;b;"c d"`：引号里的 `;` 与 `\;` 不切。
fn split_list(raw: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut in_quotes = false;
    let mut escaped = false;
    for (idx, ch) in raw.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            in_quotes = !in_quotes;
            continue;
        }
        if ch == ';' && !in_quotes {
            parts.push(raw[start..idx].to_string());
            start = idx + 1;
        }
    }
    parts.push(raw[start..].to_string());
    parts
}

// ══════════════════════ 编译 ＋ 匹配（本仓自写） ══════════════════════

#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    All,
    Not(Box<Node>),
    And(Vec<Node>),
    Or(Vec<Node>),
    Text(Text),
    /// `true` ＝ 只要目录（`folder:`），`false` ＝ 只要不是目录（`file:`）。
    Dir(bool),
    /// `ext:` —— 不是目录、扩展名（小写）在这张表里。
    Ext(Vec<Vec<u8>>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Text {
    /// 对全路径（否则只对名字）。
    on_path: bool,
    /// 带通配 ⇒ 整段对上；否则子串。
    glob: bool,
    /// 按 Unicode 小写比（否则 ASCII 逐字节折）。
    unicode: bool,
    /// 已经按上面那一档折好的模式。
    pat: Vec<u8>,
}

fn compile(e: &Expr) -> Result<Node, QueryError> {
    Ok(match e {
        Expr::Empty => Node::All,
        Expr::Not(inner) => Node::Not(Box::new(compile(inner)?)),
        Expr::And(parts) => Node::And(parts.iter().map(compile).collect::<Result<_, _>>()?),
        Expr::Or(parts) => {
            let nodes: Vec<Node> = parts.iter().map(compile).collect::<Result<_, _>>()?;
            // 空的一臂 ＝ 全集 ⇒ 整个或就是全集（打到 `foo|` 时先不缩）。
            if nodes.iter().any(|n| matches!(n, Node::All)) {
                Node::All
            } else {
                Node::Or(nodes)
            }
        }
        Expr::Term(Term::Word(w)) => text(&unquote(w), false),
        Expr::Term(Term::Filter(kind, arg)) => {
            let arg = arg.as_deref().map(unquote);
            match kind {
                FilterKind::Unsupported(name) => {
                    return Err(QueryError::Unsupported { name: name.clone() })
                }
                FilterKind::Custom(name) => {
                    text(&format!("{name}:{}", arg.unwrap_or_default()), false)
                }
                FilterKind::Path => match arg {
                    Some(a) if !a.is_empty() => text(&a, true),
                    _ => Node::All,
                },
                FilterKind::File | FilterKind::Folder => {
                    let want = Node::Dir(matches!(kind, FilterKind::Folder));
                    match arg {
                        Some(a) if !a.is_empty() => Node::And(vec![want, text(&a, false)]),
                        _ => want,
                    }
                }
                FilterKind::Ext => {
                    let exts: Vec<Vec<u8>> = arg
                        .as_deref()
                        .map(split_list)
                        .unwrap_or_default()
                        .iter()
                        .map(|s| unquote(s).trim().trim_start_matches('.').to_lowercase())
                        .filter(|s| !s.is_empty())
                        .map(String::into_bytes)
                        .collect();
                    // 打到 `ext:` 还没写扩展名 ⇒ 先不缩。
                    if exts.is_empty() {
                        Node::All
                    } else {
                        Node::Ext(exts)
                    }
                }
            }
        }
    })
}

/// 去掉成对的引号（`\"` 留成一个 `"`）。
fn unquote(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut escaped = false;
    for c in s.chars() {
        if escaped {
            escaped = false;
            if c == '"' {
                out.push('"');
                continue;
            }
            out.push('\\');
        }
        match c {
            '\\' => escaped = true,
            '"' => {}
            _ => out.push(c),
        }
    }
    if escaped {
        out.push('\\');
    }
    out
}

fn is_sep(b: u8) -> bool {
    b == b'/' || (cfg!(windows) && b == b'\\')
}

/// 一个词 → 一个文本判断。
fn text(word: &str, force_path: bool) -> Node {
    if word.is_empty() {
        return Node::All;
    }
    let unicode = word.chars().any(|c| {
        !c.is_ascii()
            && (c.to_lowercase().ne(std::iter::once(c)) || c.to_uppercase().ne(std::iter::once(c)))
    });
    let folded = if unicode {
        word.to_lowercase()
    } else {
        word.to_string()
    };
    let mut pat = folded.into_bytes();
    if cfg!(windows) {
        for b in pat.iter_mut() {
            if *b == b'\\' {
                *b = b'/';
            }
        }
    }
    Node::Text(Text {
        on_path: force_path || pat.iter().any(|&b| b == b'/'),
        glob: pat.iter().any(|&b| b == b'*' || b == b'?'),
        unicode,
        pat,
    })
}

/// 全路径的最后一段。
fn name_of(path: &[u8]) -> &[u8] {
    match path.iter().rposition(|&b| is_sep(b)) {
        Some(i) => &path[i + 1..],
        None => path,
    }
}

/// 一条候选：全路径、名字、类型；Unicode 小写那一份要用到才算。
struct Cand<'a> {
    path: &'a [u8],
    name: &'a [u8],
    kind: u8,
    low_name: OnceCell<Vec<u8>>,
    low_path: OnceCell<Vec<u8>>,
}

/// 对全路径比的那一份：Windows 上分隔符统一成 `/`（模式那一侧同样换过）。
fn path_form(p: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    if cfg!(windows) && p.contains(&b'\\') {
        std::borrow::Cow::Owned(
            p.iter()
                .map(|&b| if b == b'\\' { b'/' } else { b })
                .collect(),
        )
    } else {
        std::borrow::Cow::Borrowed(p)
    }
}

/// 合法的 UTF-8 段按 Unicode 小写，不合法的字节原样留着（只用来比，不回送）。
fn lower(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len());
    for chunk in b.utf8_chunks() {
        out.extend_from_slice(chunk.valid().to_lowercase().as_bytes());
        out.extend_from_slice(chunk.invalid());
    }
    out
}

impl Cand<'_> {
    fn eval(&self, n: &Node) -> bool {
        match n {
            Node::All => true,
            Node::Not(inner) => !self.eval(inner),
            Node::And(parts) => parts.iter().all(|p| self.eval(p)),
            Node::Or(parts) => parts.iter().any(|p| self.eval(p)),
            Node::Dir(want) => (self.kind == KIND_DIR) == *want,
            Node::Ext(exts) => {
                self.kind != KIND_DIR
                    && extension_of(self.name)
                        .is_some_and(|e| exts.iter().any(|x| e.eq_ignore_ascii_case(x)))
            }
            Node::Text(t) => {
                let hay: std::borrow::Cow<'_, [u8]> = match (t.on_path, t.unicode) {
                    (false, false) => std::borrow::Cow::Borrowed(self.name),
                    (false, true) => {
                        std::borrow::Cow::Borrowed(self.low_name.get_or_init(|| lower(self.name)))
                    }
                    (true, false) => path_form(self.path),
                    (true, true) => std::borrow::Cow::Borrowed(
                        self.low_path.get_or_init(|| lower(&path_form(self.path))),
                    ),
                };
                // Unicode 那一档两侧都已小写过；ASCII 那一档逐字节折（非 ASCII 字节原样比）。
                if t.glob {
                    glob(&t.pat, &hay)
                } else {
                    contains_ci(&hay, &t.pat)
                }
            }
        }
    }
}

/// 扩展名：最后一个 `.` 之后（`.bashrc` 的是 `bashrc`，同原作者）；没有或在末尾 ⇒ 没有。
fn extension_of(name: &[u8]) -> Option<&[u8]> {
    let pos = name.iter().rposition(|&b| b == b'.')?;
    (pos + 1 < name.len()).then(|| &name[pos + 1..])
}

/// 子串，ASCII 不分大小写。
fn contains_ci(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > hay.len() {
        return false;
    }
    let first = needle[0].to_ascii_lowercase();
    let rest = &needle[1..];
    let last = hay.len() - needle.len();
    (0..=last).any(|i| {
        hay[i].to_ascii_lowercase() == first
            && hay[i + 1..i + needle.len()].eq_ignore_ascii_case(rest)
    })
}

/// 从 `i` 往后跳过一个字符（UTF-8 的后续字节一并跳；不是 UTF-8 的就跳一个字节）。
fn next_char(s: &[u8], i: usize) -> usize {
    let mut j = i + 1;
    while j < s.len() && (s[j] & 0xC0) == 0x80 {
        j += 1;
    }
    j
}

/// 通配，整段对上：`*` 任意串（含分隔符）· `?` 一个字符；ASCII 不分大小写。
fn glob(p: &[u8], s: &[u8]) -> bool {
    let (mut pi, mut si) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;
    while si < s.len() {
        match p.get(pi) {
            Some(b'*') => {
                star = Some((pi, si));
                pi += 1;
            }
            Some(b'?') => {
                pi += 1;
                si = next_char(s, si);
            }
            Some(&c) if c.eq_ignore_ascii_case(&s[si]) => {
                pi += 1;
                si += 1;
            }
            _ => match star {
                Some((sp, ss)) => {
                    let ns = next_char(s, ss);
                    star = Some((sp, ns));
                    pi = sp + 1;
                    si = ns;
                }
                None => return false,
            },
        }
    }
    p[pi..].iter().all(|&b| b == b'*')
}

#[cfg(test)]
#[path = "../../../tests/backend/files/query_tests.rs"]
mod tests;
