//! 语法树 ⇒ [`Doc`]：comrak 解析一遍，照两端真画的那十几种块收成树；代码块只记语言与原文，高亮另算（[`crate::highlight`]）。

use crate::md::*;
use crate::segment::segment;
use comrak::nodes::{AlertType, AstNode, ListType, NodeValue, TableAlignment};
use comrak::{parse_document, Arena, Options};
use std::collections::HashMap;

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
        Conv {
            src,
            lines,
            fn_ix: HashMap::new(),
            defs: Vec::new(),
        }
    }

    /// 源码位置（行 · 列，都从 1 起，列按字节）⇒ 从那里起的原文。
    fn at(&self, line: usize, col: usize) -> &'s str {
        let Some(&start) = self.lines.get(line.wrapping_sub(1)) else {
            return "";
        };
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
            NodeValue::Heading(h) => out.push(MdBlock::H {
                level: h.level,
                k: self.inlines(node),
            }),
            NodeValue::CodeBlock(cb) => {
                let lang = lang_of(&cb.info);
                let x = cb
                    .literal
                    .strip_suffix('\n')
                    .unwrap_or(&cb.literal)
                    .to_string();
                out.push(MdBlock::Code {
                    lang,
                    x,
                    cont: false,
                });
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
                out.push(MdBlock::List {
                    start,
                    tight: l.tight,
                    items,
                });
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
                    out.push(MdBlock::Math {
                        tex: m.literal.trim().to_string(),
                    });
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
            NodeValue::Emph => out.node(InlNode::I {
                k: self.inlines(node),
            }),
            NodeValue::Strong => out.node(InlNode::B {
                k: self.inlines(node),
            }),
            NodeValue::Strikethrough => {
                if self.at(sp.start.line, sp.start.column).starts_with("~~") {
                    out.node(InlNode::Del {
                        k: self.inlines(node),
                    });
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
                    out.node(InlNode::A {
                        href: l.url,
                        k: self.inlines(node),
                    });
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
pub(crate) struct Inls(pub(crate) Vec<Inl>);

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
    pub(crate) fn push(&mut self, i: Inl) {
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
        InlNode::B { k } | InlNode::I { k } | InlNode::Del { k } | InlNode::A { k, .. } => {
            match k.last()? {
                Inl::S(s) => s.chars().last(),
                Inl::N(n) => last_char_of(n),
            }
        }
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
        | 0x20000..=0x3FFFD) // 扩展 B 起
}

fn is_br(s: &str) -> bool {
    let t: String = s
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
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
            Inl::N(
                InlNode::B { k } | InlNode::I { k } | InlNode::Del { k } | InlNode::A { k, .. },
            ) => out.push_str(&plain(k)),
            Inl::N(InlNode::C { x }) => out.push_str(x),
            Inl::N(InlNode::M { tex }) => out.push_str(tex),
            Inl::N(InlNode::Img { alt, .. }) => out.push_str(alt),
            Inl::N(InlNode::Br) => out.push(' '),
            Inl::N(InlNode::Fn { .. } | InlNode::Ref { .. }) => {}
        }
    }
    out
}

/// 信息串 ⇒ 归一过的语言名：第一个词（逗号也断开）、小写、常见别名换成正名。
fn lang_of(info: &str) -> Option<String> {
    let word = info
        .split(|c: char| c.is_whitespace() || c == ',' || c == '{')
        .next()?
        .trim();
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
