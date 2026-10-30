//! 助手正文的 Markdown 解析：[`parse`] 把一段原文变成 [`Doc`]（块树 ＋ 代码高亮的类 ＋ 长正文的段界）。
//!
//! 只有这一处解析；两端（桌面 · 手机）只照树排版，不再各自解析、各自定规矩。这里一处定的规矩：
//! - 删除线只认 `~~`：单 `~` 拆回字（按源码位置看定界符长度）。
//! - 原样 HTML 不进树：行内 · 块级都当字面字交，只有 `<br>` 认成硬换行。
//! - 链接只放行 `http` · `https` · `mailto`，图只放行 `http` · `https`；别的交成它的字。
//! - 数学交 TeX 原文；段落里独占的 `$$…$$` · `\[…\]` 提成块级公式。
//! - 软换行：两边都是汉字（东亚宽字，不含谚文）⇒ 交空串，否则交一个空格。
//! - 代码块语言：信息串第一个词，小写，常见别名归一；认不出的不猜、不高亮。
//!
//! 分段见 [`segment`]：按顶层块累计字数，到 [`SEG_BYTES`] 就在下一个顶层块前记一个段起点。

#[path = "../../cmd-wire/src/md.rs"]
pub mod md;

pub use md::*;

use comrak::nodes::{AlertType, AstNode, ListType, NodeValue, TableAlignment};
use comrak::{parse_document, Arena, Options};
use std::collections::HashMap;
use std::sync::OnceLock;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxSet};

/// 一段的字数上限（UTF-8 字节），同界面「长回复先画头一截」那条线。
pub const SEG_BYTES: usize = 20_000;

/// 一段 Markdown 原文 ⇒ 块树。纯函数：不碰盘、不失败（认不出的写法照字交）。
pub fn parse(src: &str) -> Doc {
    let arena = Arena::new();
    let root = parse_document(&arena, src, &options());
    let mut conv = Conv::new(src);
    let mut blocks = Vec::new();
    conv.blocks_of(root, &mut blocks);
    if let Some(notes) = conv.notes() {
        blocks.push(notes);
    }
    segment(blocks)
}

fn options() -> Options<'static> {
    let mut o = Options::default();
    let e = &mut o.extension;
    e.strikethrough = true;
    e.table = true;
    e.autolink = true;
    e.tasklist = true;
    e.footnotes = true;
    e.alerts = true;
    e.math_dollars = true;
    e.math_latex = true;
    e.cjk_friendly_emphasis = true;
    o
}

// ───────────────────────────── 语法树 ⇒ Doc ─────────────────────────────

struct Conv<'s> {
    src: &'s str,
    /// 每行行首的字节偏移（源码位置的行列 ⇒ 字节）。
    lines: Vec<usize>,
    /// 脚注名 ⇒ 号（正文里第一次引用的先后）。
    fn_ix: HashMap<String, u32>,
    defs: Vec<(String, Vec<MdBlock>)>,
}

impl<'s> Conv<'s> {
    fn new(src: &'s str) -> Self {
        let mut lines = vec![0];
        lines.extend(src.match_indices('\n').map(|(i, _)| i + 1));
        Conv { src, lines, fn_ix: HashMap::new(), defs: Vec::new() }
    }

    /// 源码位置（行 · 列，都从 1 起，列按字节）⇒ 从那里起的原文。
    fn at(&self, line: usize, col: usize) -> &'s str {
        let Some(&start) = self.lines.get(line.wrapping_sub(1)) else { return "" };
        self.src.get(start + col.saturating_sub(1)..).unwrap_or("")
    }

    fn notes(&mut self) -> Option<MdBlock> {
        let mut items: Vec<MdNote> = std::mem::take(&mut self.defs)
            .into_iter()
            .filter_map(|(name, blocks)| self.fn_ix.get(&name).map(|&n| MdNote { n, blocks }))
            .collect();
        if items.is_empty() {
            return None;
        }
        items.sort_by_key(|i| i.n);
        Some(MdBlock::Notes { items })
    }

    fn blocks_of<'a>(&mut self, node: &'a AstNode<'a>, out: &mut Vec<MdBlock>) {
        for child in node.children() {
            self.block(child, out);
        }
    }

    fn block<'a>(&mut self, node: &'a AstNode<'a>, out: &mut Vec<MdBlock>) {
        let value = node.data().value.clone();
        match value {
            NodeValue::Paragraph => self.paragraph(node, out),
            NodeValue::Heading(h) => out.push(MdBlock::H { level: h.level, k: self.inlines(node) }),
            NodeValue::CodeBlock(cb) => {
                let lang = lang_of(&cb.info);
                let x = cb.literal.strip_suffix('\n').unwrap_or(&cb.literal).to_string();
                let hl = lang.as_deref().map(|l| highlight(l, &x)).unwrap_or_default();
                out.push(MdBlock::Code { lang, x, hl, cont: false });
            }
            NodeValue::HtmlBlock(h) => {
                let mut k = Inls::default();
                for (i, line) in h.literal.trim_end_matches('\n').split('\n').enumerate() {
                    if i > 0 {
                        k.node(InlNode::Br);
                    }
                    if is_br(line.trim()) {
                        k.node(InlNode::Br);
                    } else {
                        k.text(line);
                    }
                }
                out.push(MdBlock::P { k: k.0 });
            }
            NodeValue::List(l) => {
                let mut items = Vec::new();
                for item in node.children() {
                    let task = match &item.data().value {
                        NodeValue::TaskItem(t) => Some(t.symbol.is_some()),
                        _ => None,
                    };
                    let mut blocks = Vec::new();
                    self.blocks_of(item, &mut blocks);
                    items.push(MdItem { task, blocks });
                }
                let start = (l.list_type == ListType::Ordered).then_some(l.start as u32);
                out.push(MdBlock::List { start, tight: l.tight, items });
            }
            NodeValue::BlockQuote | NodeValue::MultilineBlockQuote(_) => {
                let mut blocks = Vec::new();
                self.blocks_of(node, &mut blocks);
                out.push(MdBlock::Quote { blocks });
            }
            NodeValue::Alert(a) => {
                let kind = match a.alert_type {
                    AlertType::Note => AlertKind::Note,
                    AlertType::Tip => AlertKind::Tip,
                    AlertType::Important => AlertKind::Important,
                    AlertType::Warning => AlertKind::Warning,
                    AlertType::Caution => AlertKind::Caution,
                };
                let mut blocks = Vec::new();
                self.blocks_of(node, &mut blocks);
                out.push(MdBlock::Alert { kind, blocks });
            }
            NodeValue::Table(t) => {
                let align = t
                    .alignments
                    .iter()
                    .map(|a| match a {
                        TableAlignment::None => MdAlign::None,
                        TableAlignment::Left => MdAlign::Left,
                        TableAlignment::Center => MdAlign::Center,
                        TableAlignment::Right => MdAlign::Right,
                    })
                    .collect();
                let mut head = Vec::new();
                let mut rows = Vec::new();
                for row in node.children() {
                    let cells: Vec<Vec<Inl>> = row.children().map(|c| self.inlines(c)).collect();
                    match row.data().value {
                        NodeValue::TableRow(true) => head = cells,
                        _ => rows.push(cells),
                    }
                }
                out.push(MdBlock::Table { align, head, rows });
            }
            NodeValue::ThematicBreak => out.push(MdBlock::Rule),
            NodeValue::FootnoteDefinition(fd) => {
                let mut blocks = Vec::new();
                self.blocks_of(node, &mut blocks);
                self.defs.push((fd.name, blocks));
            }
            _ => self.blocks_of(node, out),
        }
    }

    /// 段落：独占的块级公式（`$$…$$` · `\[…\]`）提成 [`MdBlock::Math`]，把段落在那里断开。
    fn paragraph<'a>(&mut self, node: &'a AstNode<'a>, out: &mut Vec<MdBlock>) {
        let mut cur = Inls::default();
        let flush = |cur: &mut Inls, out: &mut Vec<MdBlock>| {
            let k = std::mem::take(cur).trimmed();
            if !k.is_empty() {
                out.push(MdBlock::P { k });
            }
        };
        for child in node.children() {
            if let NodeValue::Math(m) = &child.data().value {
                if m.display_math {
                    flush(&mut cur, out);
                    out.push(MdBlock::Math { tex: m.literal.trim().to_string() });
                    continue;
                }
            }
            self.inline(child, &mut cur);
        }
        flush(&mut cur, out);
    }

    fn inlines<'a>(&mut self, node: &'a AstNode<'a>) -> Vec<Inl> {
        let mut k = Inls::default();
        for child in node.children() {
            self.inline(child, &mut k);
        }
        k.0
    }

    fn inline<'a>(&mut self, node: &'a AstNode<'a>, out: &mut Inls) {
        let data = node.data();
        let sp = data.sourcepos;
        let value = data.value.clone();
        drop(data);
        match value {
            NodeValue::Text(t) => out.text(&t),
            NodeValue::SoftBreak => {
                let before = out.last_char();
                let after = node.next_sibling().and_then(first_char);
                let joined = matches!((before, after), (Some(a), Some(b)) if east_asian_wide(a) && east_asian_wide(b));
                out.text(if joined { "" } else { " " });
            }
            NodeValue::LineBreak => out.node(InlNode::Br),
            NodeValue::Code(c) => out.node(InlNode::C { x: c.literal }),
            NodeValue::HtmlInline(h) => {
                if is_br(&h) {
                    out.node(InlNode::Br)
                } else {
                    out.text(&h)
                }
            }
            NodeValue::Emph => out.node(InlNode::I { k: self.inlines(node) }),
            NodeValue::Strong => out.node(InlNode::B { k: self.inlines(node) }),
            NodeValue::Strikethrough => {
                if self.at(sp.start.line, sp.start.column).starts_with("~~") {
                    out.node(InlNode::Del { k: self.inlines(node) });
                } else {
                    out.text("~");
                    for child in node.children() {
                        self.inline(child, out);
                    }
                    out.text("~");
                }
            }
            NodeValue::Link(l) => {
                if link_ok(&l.url) {
                    out.node(InlNode::A { href: l.url, k: self.inlines(node) });
                } else {
                    for child in node.children() {
                        self.inline(child, out);
                    }
                }
            }
            NodeValue::Image(l) => {
                let alt = plain(&self.inlines(node));
                if image_ok(&l.url) {
                    out.node(InlNode::Img { src: l.url, alt });
                } else {
                    out.text(&alt);
                }
            }
            NodeValue::FootnoteReference(f) => {
                let next = self.fn_ix.len() as u32 + 1;
                let n = *self.fn_ix.entry(f.name).or_insert(next);
                out.node(InlNode::Fn { n });
            }
            NodeValue::Math(m) => out.node(InlNode::M { tex: m.literal }),
            NodeValue::Raw(r) => out.text(&r),
            NodeValue::EscapedTag(t) => out.text(t),
            _ => {
                for child in node.children() {
                    self.inline(child, out);
                }
            }
        }
    }
}

/// 行内一串，相邻的字并成一段。
#[derive(Default)]
struct Inls(Vec<Inl>);

impl Inls {
    fn text(&mut self, t: &str) {
        if t.is_empty() {
            return;
        }
        match self.0.last_mut() {
            Some(Inl::S(s)) => s.push_str(t),
            _ => self.0.push(Inl::S(t.to_string())),
        }
    }
    fn node(&mut self, n: InlNode) {
        self.0.push(Inl::N(n));
    }
    fn push(&mut self, i: Inl) {
        match i {
            Inl::S(s) => self.text(&s),
            n => self.0.push(n),
        }
    }
    fn last_char(&self) -> Option<char> {
        match self.0.last()? {
            Inl::S(s) => s.chars().last(),
            Inl::N(n) => last_char_of(n),
        }
    }
    /// 去掉头尾的空白字（段落被块级公式断开时，断口两边软换行留下的空格）。
    fn trimmed(mut self) -> Vec<Inl> {
        if let Some(Inl::S(s)) = self.0.first_mut() {
            *s = s.trim_start().to_string();
        }
        if let Some(Inl::S(s)) = self.0.last_mut() {
            *s = s.trim_end().to_string();
        }
        self.0.retain(|i| !matches!(i, Inl::S(s) if s.is_empty()));
        self.0
    }
}

fn last_char_of(n: &InlNode) -> Option<char> {
    match n {
        InlNode::B { k } | InlNode::I { k } | InlNode::Del { k } | InlNode::A { k, .. } => match k.last()? {
            Inl::S(s) => s.chars().last(),
            Inl::N(n) => last_char_of(n),
        },
        InlNode::C { x } => x.chars().last(),
        _ => None,
    }
}

/// 一个行内节点的头一个字（软换行看后面那一边）。
fn first_char<'a>(node: &'a AstNode<'a>) -> Option<char> {
    match &node.data().value {
        NodeValue::Text(t) => t.chars().next(),
        NodeValue::Code(c) => c.literal.chars().next(),
        NodeValue::Emph | NodeValue::Strong | NodeValue::Strikethrough | NodeValue::Link(_) => {
            node.first_child().and_then(first_char)
        }
        _ => None,
    }
}

/// 东亚宽字（汉字 · 假名 · 全角标点）：两边都是它的软换行不出空格。谚文照西文（词间本来有空格）。
fn east_asian_wide(c: char) -> bool {
    matches!(c as u32,
        0x2E80..=0x303F       // 部首 · 康熙部首 · CJK 符号与标点
        | 0x3040..=0x33FF     // 假名 · 注音 · 兼容字
        | 0x3400..=0x4DBF     // 扩展 A
        | 0x4E00..=0x9FFF     // 基本区
        | 0xF900..=0xFAFF     // 兼容表意字
        | 0xFE30..=0xFE4F     // 兼容形式
        | 0xFF00..=0xFF60     // 全角 ASCII 与标点
        | 0xFFE0..=0xFFE6
        | 0x20000..=0x3FFFD)  // 扩展 B 起
}

fn is_br(s: &str) -> bool {
    let t: String = s.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_lowercase();
    matches!(t.as_str(), "<br>" | "<br/>")
}

fn scheme_in(url: &str, allowed: &[&str]) -> bool {
    let lower = url.trim_start().to_ascii_lowercase();
    allowed.iter().any(|s| lower.starts_with(s))
}

fn link_ok(url: &str) -> bool {
    scheme_in(url, &["http://", "https://", "mailto:"])
}

fn image_ok(url: &str) -> bool {
    scheme_in(url, &["http://", "https://"])
}

/// 行内一串里的字（图的替代字用）。
fn plain(k: &[Inl]) -> String {
    let mut out = String::new();
    for i in k {
        match i {
            Inl::S(s) => out.push_str(s),
            Inl::N(InlNode::B { k } | InlNode::I { k } | InlNode::Del { k } | InlNode::A { k, .. }) => out.push_str(&plain(k)),
            Inl::N(InlNode::C { x }) => out.push_str(x),
            Inl::N(InlNode::M { tex }) => out.push_str(tex),
            Inl::N(InlNode::Img { alt, .. }) => out.push_str(alt),
            Inl::N(InlNode::Br) => out.push(' '),
            Inl::N(InlNode::Fn { .. }) => {}
        }
    }
    out
}

// ───────────────────────────── 代码高亮 ─────────────────────────────

/// 信息串 ⇒ 归一过的语言名：第一个词（逗号也断开）、小写、常见别名换成正名。
fn lang_of(info: &str) -> Option<String> {
    let word = info.split(|c: char| c.is_whitespace() || c == ',' || c == '{').next()?.trim();
    if word.is_empty() {
        return None;
    }
    let w = word.to_ascii_lowercase();
    let canon = match w.as_str() {
        "sh" | "shell" | "zsh" | "bash" | "console" | "shellsession" => "bash",
        "ts" | "typescript" | "mts" | "cts" => "typescript",
        "js" | "javascript" | "mjs" | "cjs" | "node" => "javascript",
        "yml" | "yaml" => "yaml",
        "py" | "python" | "python3" | "py3" => "python",
        "rs" | "rust" => "rust",
        "kt" | "kts" | "kotlin" => "kotlin",
        "md" | "markdown" => "markdown",
        "golang" | "go" => "go",
        "c++" | "cpp" | "cc" | "cxx" | "hpp" => "cpp",
        "cs" | "csharp" | "c#" => "cs",
        "rb" | "ruby" => "ruby",
        "ps" | "ps1" | "pwsh" | "powershell" => "powershell",
        "jsonc" | "json5" | "json" => "json",
        "htm" | "html" | "xhtml" => "html",
        "dockerfile" | "docker" => "dockerfile",
        "patch" | "diff" => "diff",
        _ => return Some(w),
    };
    Some(canon.to_string())
}

fn syntaxes() -> &'static SyntaxSet {
    static SS: OnceLock<SyntaxSet> = OnceLock::new();
    SS.get_or_init(two_face::syntax::extra_newlines)
}

/// 作用域名 ⇒ 类；`None` ⇒ 这一层不定，往外一层看。
fn class_of_scope(name: &str) -> Option<HlClass> {
    let is = |p: &str| name == p || (name.starts_with(p) && name.as_bytes().get(p.len()) == Some(&b'.'));
    let c = if is("comment") {
        HlClass::Comment
    } else if is("string") {
        HlClass::String
    } else if is("constant.numeric") {
        HlClass::Number
    } else if is("constant") || is("variable.other.constant") || is("support.constant") {
        HlClass::Constant
    } else if is("keyword.operator") || is("punctuation") {
        return None;
    } else if is("entity.name.function") || is("support.function") || is("variable.function") {
        HlClass::Function
    } else if is("entity.name.type")
        || is("entity.name.class")
        || is("entity.name.struct")
        || is("entity.name.enum")
        || is("entity.name.trait")
        || is("entity.name.interface")
        || is("entity.other.inherited-class")
        || is("support.type")
        || is("support.class")
        || is("storage.type.primitive")
        || is("storage.type.numeric")
    {
        HlClass::Type
    } else if is("keyword") || is("storage") {
        HlClass::Keyword
    } else if is("entity.name.tag") {
        HlClass::Tag
    } else if is("entity.other.attribute-name") {
        HlClass::Attribute
    } else if is("markup.inserted") {
        HlClass::Inserted
    } else if is("markup.deleted") {
        HlClass::Deleted
    } else if is("variable.parameter") || is("variable.language") {
        HlClass::Variable
    } else if is("markup.heading")
        || is("entity.name.section")
        || is("meta.preprocessor")
        || is("meta.annotation")
        || is("meta.attribute")
        || is("meta.decorator")
        || is("meta.diff.range")
        || is("meta.diff.header")
    {
        HlClass::Meta
    } else {
        return None;
    };
    Some(c)
}

/// 代码 ⇒ `hl` 三元组（UTF-16 码元：起点差 · 长 · 类）。语言认不出 ⇒ 空。
fn highlight(lang: &str, x: &str) -> Vec<u32> {
    let ss = syntaxes();
    let Some(syntax) = ss.find_syntax_by_token(lang) else { return Vec::new() };
    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut memo: HashMap<Vec<Scope>, Option<HlClass>> = HashMap::new();
    let mut spans: Vec<(u32, u32, u32)> = Vec::new();
    let mut base16: u32 = 0;
    let emit = |spans: &mut Vec<(u32, u32, u32)>, start16: u32, text: &str, class: Option<HlClass>| {
        let text = text.trim_end_matches('\n');
        let len = text.encode_utf16().count() as u32;
        let Some(class) = class else { return };
        if len == 0 {
            return;
        }
        let c = class as u32;
        match spans.last_mut() {
            Some((s, l, lc)) if *lc == c && *s + *l == start16 => *l += len,
            _ => spans.push((start16, len, c)),
        }
    };
    for line in x.split_inclusive('\n') {
        let owned;
        let line = if line.ends_with('\n') {
            line
        } else {
            owned = format!("{line}\n");
            owned.as_str()
        };
        let Ok(ops) = state.parse_line(line, ss) else { return Vec::new() };
        let mut prev = 0usize;
        let mut at16 = base16;
        for (pos, op) in ops {
            if pos > prev {
                let class = classify(&stack, &mut memo);
                let seg = &line[prev..pos];
                emit(&mut spans, at16, seg, class);
                at16 += seg.encode_utf16().count() as u32;
                prev = pos;
            }
            if stack.apply(&op).is_err() {
                return Vec::new();
            }
        }
        if prev < line.len() {
            let class = classify(&stack, &mut memo);
            emit(&mut spans, at16, &line[prev..], class);
        }
        base16 += line.encode_utf16().count() as u32;
    }
    encode(&spans)
}

fn classify(stack: &ScopeStack, memo: &mut HashMap<Vec<Scope>, Option<HlClass>>) -> Option<HlClass> {
    let scopes = stack.as_slice();
    if let Some(c) = memo.get(scopes) {
        return *c;
    }
    let c = scopes.iter().rev().find_map(|s| class_of_scope(&s.build_string()));
    memo.insert(scopes.to_vec(), c);
    c
}

/// 绝对的 (起点, 长, 类) ⇒ 平铺三元组（起点差相对上一段的末尾）。
fn encode(spans: &[(u32, u32, u32)]) -> Vec<u32> {
    let mut out = Vec::with_capacity(spans.len() * 3);
    let mut end = 0;
    for &(s, l, c) in spans {
        out.extend([s - end, l, c]);
        end = s + l;
    }
    out
}

fn decode(hl: &[u32]) -> Vec<(u32, u32, u32)> {
    let mut at = 0;
    hl.chunks_exact(3)
        .map(|c| {
            let s = at + c[0];
            at = s + c[1];
            (s, c[1], c[2])
        })
        .collect()
}

// ───────────────────────────── 分段 ─────────────────────────────

/// 顶层块 ⇒ 带段界的 [`Doc`]：按块里的字（UTF-8 字节）累计，加上下一块就超过 [`SEG_BYTES`] ⇒ 在它前面记一个段起点。
/// 单个顶层块就超过一段的先切开：代码块按行（后几块 `cont`）· 列表按项（有序的后一截带上起号）· 段落按句末。
pub fn segment(blocks: Vec<MdBlock>) -> Doc {
    let mut out = Vec::with_capacity(blocks.len());
    let mut segs = Vec::new();
    let mut acc = 0usize;
    for b in blocks {
        for p in split(b) {
            let w = weight(&p);
            if acc > 0 && acc + w > SEG_BYTES {
                segs.push(out.len() as u32);
                acc = 0;
            }
            out.push(p);
            acc += w;
        }
    }
    Doc { blocks: out, segs }
}

fn weight_inl(k: &[Inl]) -> usize {
    k.iter()
        .map(|i| match i {
            Inl::S(s) => s.len(),
            Inl::N(InlNode::B { k } | InlNode::I { k } | InlNode::Del { k }) => weight_inl(k),
            Inl::N(InlNode::A { href, k }) => href.len() + weight_inl(k),
            Inl::N(InlNode::C { x }) => x.len(),
            Inl::N(InlNode::M { tex }) => tex.len(),
            Inl::N(InlNode::Img { src, alt }) => src.len() + alt.len(),
            Inl::N(InlNode::Br | InlNode::Fn { .. }) => 1,
        })
        .sum()
}

fn weight_blocks(bs: &[MdBlock]) -> usize {
    bs.iter().map(weight).sum()
}

/// 一块里的字（UTF-8 字节）。
fn weight(b: &MdBlock) -> usize {
    match b {
        MdBlock::P { k } | MdBlock::H { k, .. } => weight_inl(k),
        MdBlock::Code { x, .. } => x.len(),
        MdBlock::Math { tex } => tex.len(),
        MdBlock::List { items, .. } => items.iter().map(|i| weight_blocks(&i.blocks)).sum(),
        MdBlock::Quote { blocks } | MdBlock::Alert { blocks, .. } => weight_blocks(blocks),
        MdBlock::Table { head, rows, .. } => {
            head.iter().map(|c| weight_inl(c)).sum::<usize>()
                + rows.iter().flatten().map(|c| weight_inl(c)).sum::<usize>()
        }
        MdBlock::Rule => 1,
        MdBlock::Notes { items } => items.iter().map(|n| weight_blocks(&n.blocks)).sum(),
    }
}

/// 一个件一个件往里装，装下这一件就超过一段 ⇒ 另起一截。
fn pack<T>(units: Vec<T>, w: impl Fn(&T) -> usize) -> Vec<Vec<T>> {
    let mut out: Vec<Vec<T>> = Vec::new();
    let mut cur = Vec::new();
    let mut acc = 0;
    for u in units {
        let uw = w(&u);
        if !cur.is_empty() && acc + uw > SEG_BYTES {
            out.push(std::mem::take(&mut cur));
            acc = 0;
        }
        acc += uw;
        cur.push(u);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn split(b: MdBlock) -> Vec<MdBlock> {
    if weight(&b) <= SEG_BYTES {
        return vec![b];
    }
    match b {
        MdBlock::Code { lang, x, hl, .. } => split_code(lang, &x, &hl),
        MdBlock::List { start, tight, items } => {
            let mut at = start;
            pack(items, |i| weight_blocks(&i.blocks))
                .into_iter()
                .map(|items| {
                    let here = at;
                    at = at.map(|s| s + items.len() as u32);
                    MdBlock::List { start: here, tight, items }
                })
                .collect()
        }
        MdBlock::P { k } => pack(sentences(k), |i| weight_inl(std::slice::from_ref(i)))
            .into_iter()
            .map(|units| {
                let mut k = Inls::default();
                for u in units {
                    k.push(u);
                }
                MdBlock::P { k: k.0 }
            })
            .collect(),
        other => vec![other],
    }
}

/// 段落的行内一串按句末（。！？ 以及后面跟空白或到头的 . ! ?）拆成小件；别的行内节点整个一件。
fn sentences(k: Vec<Inl>) -> Vec<Inl> {
    let mut out = Vec::new();
    for i in k {
        let Inl::S(s) = i else {
            out.push(i);
            continue;
        };
        let mut from = 0;
        let mut it = s.char_indices().peekable();
        while let Some((at, c)) = it.next() {
            let end = at + c.len_utf8();
            let next_blank = it.peek().map_or(true, |(_, n)| n.is_whitespace());
            if matches!(c, '。' | '！' | '？') || (matches!(c, '.' | '!' | '?') && next_blank) {
                out.push(Inl::S(s[from..end].to_string()));
                from = end;
            }
        }
        if from < s.len() {
            out.push(Inl::S(s[from..].to_string()));
        }
    }
    out
}

/// 超长代码按行切成几块：每块的 `hl` 照它在原文里的 UTF-16 区间截出来。
fn split_code(lang: Option<String>, x: &str, hl: &[u32]) -> Vec<MdBlock> {
    let spans = decode(hl);
    // (字节起, 字节止, UTF-16 起) —— 不含行间的那个换行
    let mut lines = Vec::new();
    let mut at = 0usize;
    let mut at16 = 0u32;
    for line in x.split('\n') {
        lines.push((at, at + line.len(), at16));
        at += line.len() + 1;
        at16 += line.encode_utf16().count() as u32 + 1;
    }
    let pieces = pack(lines, |(a, b, _)| b - a + 1);
    pieces
        .into_iter()
        .enumerate()
        .map(|(i, ls)| {
            let (a, _, a16) = ls[0];
            let (_, b, _) = *ls.last().unwrap();
            let text = &x[a..b];
            let b16 = a16 + text.encode_utf16().count() as u32;
            let mine: Vec<(u32, u32, u32)> = spans
                .iter()
                .filter_map(|&(s, l, c)| {
                    let (s2, e2) = (s.max(a16), (s + l).min(b16));
                    (s2 < e2).then(|| (s2 - a16, e2 - s2, c))
                })
                .collect();
            MdBlock::Code { lang: lang.clone(), x: text.to_string(), hl: encode(&mine), cont: i > 0 }
        })
        .collect()
}

#[cfg(test)]
#[path = "../../../../tests/common/md-core/md_core_tests.rs"]
mod tests;
