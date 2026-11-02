//! 计划格引用标在树上（[`InlNode::Ref`]）。认编号是 pb 的事（`pb dump --refs`），这里只做换方向的纯函数：
//! - [`ref_text`]：一棵树投成交 pb 认的一段字 —— 只取字（`Inl::S`），代码 · 公式 · 图 · 换行 · 脚注号各换成一个 `U+FFFC`
//!   （编号不跨过它们，代码里的不认），块与块之间空一行；
//! - [`with_refs`]：照 pb 回的起止（换成 UTF-16、在投出来的那段字里）把那串字换成引用节点；
//! - [`parse_marked`]：原文在手上、pb 给的是原文里的 UTF-16 起止（计划格的正文）⇒ 解析时就把那几段认成引用节点。
//!
//! 一处引用只在落在同一串字里时才成（跨了加粗 · 链接的边界 ⇒ 不插，照原文）。

use crate::md::*;
use crate::parse::parse;

/// 投出来的字里，块与块之间的接缝（空一行：pb 的认法里编号不跨行）。
const SEAM: &str = "\n\n";
/// 不是字的那几种行内 · 块换成的占位字。
const GAP: char = '\u{FFFC}';
/// [`parse_marked`] 在原文里给一处引用做的记号：`OPEN` 序号 `CLOSE`（私用区，正文里不会有）。
const OPEN: char = '\u{E000}';
const CLOSE: char = '\u{E001}';

/// 一处引用：起止（UTF-16 下标：[`with_refs`] 里在投出来的那段字里，[`parse_marked`] 里在原文里）· 那几格 · 标题。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefAt {
    pub start: usize,
    pub end: usize,
    pub ids: Vec<String>,
    pub label: String,
}

/// 一棵树投成交给 pb 认的那段字。
pub fn ref_text(doc: &Doc) -> String {
    let mut w = Walk::new(&[]);
    w.blocks(&mut doc.blocks.clone());
    w.text
}

/// 照 `refs`（[`ref_text`] 那段字里的 UTF-16 起止）把那几串字换成引用节点；越界 · 叠着 · 跨了串的不插。
pub fn with_refs(doc: &Doc, refs: &[RefAt]) -> Doc {
    let mut sorted: Vec<&RefAt> = refs.iter().filter(|r| r.start < r.end).collect();
    sorted.sort_by_key(|r| r.start);
    let mut w = Walk::new(&sorted);
    let mut out = doc.clone();
    w.blocks(&mut out.blocks);
    out
}

/// 解析一段原文，`refs`（原文里的 UTF-16 起止）那几段解成引用节点（`label` 照给的）。越界 · 叠着 · 不落在字边界上的不算。
/// 落进代码里的那一处照原文（pb 本就不认代码里的编号）。
pub fn parse_marked(src: &str, refs: &[RefAt]) -> Doc {
    let mut sorted: Vec<&RefAt> = refs.iter().filter(|r| r.start < r.end).collect();
    sorted.sort_by_key(|r| r.start);
    let mut marked = String::with_capacity(src.len() + refs.len() * 4);
    let mut kept: Vec<(String, &RefAt)> = Vec::new();
    let mut u16_at = 0usize;
    let mut chars = src.char_indices().peekable();
    let mut next = sorted.into_iter().peekable();
    while let Some((i, c)) = chars.next() {
        // 跳过起点已经过去的（叠着的）。
        while next.peek().is_some_and(|r| r.start < u16_at) {
            next.next();
        }
        if let Some(r) = next.peek().copied().filter(|r| r.start == u16_at) {
            // 找终点那个字边界。
            let mut at = u16_at;
            let mut j = i;
            let mut ok = false;
            for (k, ch) in src[i..].char_indices() {
                if at == r.end {
                    j = i + k;
                    ok = true;
                    break;
                }
                at += ch.len_utf16();
            }
            if !ok && at == r.end {
                j = src.len();
                ok = true;
            }
            next.next();
            if ok {
                let x = &src[i..j];
                marked.push(OPEN);
                marked.push_str(&kept.len().to_string());
                marked.push(CLOSE);
                kept.push((x.to_string(), r));
                u16_at = r.end;
                while chars.peek().is_some_and(|(k, _)| *k < j) {
                    chars.next();
                }
                continue;
            }
        }
        marked.push(c);
        u16_at += c.len_utf16();
    }
    let mut doc = parse(&marked);
    let mut back = Unmark { kept: &kept };
    back.blocks(&mut doc.blocks);
    doc
}

/// 把 [`parse_marked`] 的记号换回引用节点（没落进字里的 —— 代码 · 公式 —— 换回原文）。
struct Unmark<'a> {
    kept: &'a [(String, &'a RefAt)],
}

impl Unmark<'_> {
    fn blocks(&mut self, blocks: &mut [MdBlock]) {
        for b in blocks {
            match b {
                MdBlock::P { k } | MdBlock::H { k, .. } => self.inls(k),
                MdBlock::Code { x, .. } => *x = self.plain(x),
                MdBlock::Math { tex } => *tex = self.plain(tex),
                MdBlock::List { items, .. } => {
                    items.iter_mut().for_each(|i| self.blocks(&mut i.blocks))
                }
                MdBlock::Quote { blocks } | MdBlock::Alert { blocks, .. } => self.blocks(blocks),
                MdBlock::Table { head, rows, .. } => {
                    for cell in head.iter_mut().chain(rows.iter_mut().flatten()) {
                        self.inls(cell);
                    }
                }
                MdBlock::Notes { items } => {
                    items.iter_mut().for_each(|i| self.blocks(&mut i.blocks))
                }
                MdBlock::Rule => {}
            }
        }
    }

    fn inls(&mut self, k: &mut Vec<Inl>) {
        let old = std::mem::take(k);
        for i in old {
            match i {
                Inl::S(s) => self.split(&s, k),
                Inl::N(mut n) => {
                    match &mut n {
                        InlNode::B { k: inner }
                        | InlNode::I { k: inner }
                        | InlNode::Del { k: inner }
                        | InlNode::A { k: inner, .. } => self.inls(inner),
                        InlNode::C { x } => *x = self.plain(x),
                        InlNode::M { tex } => *tex = self.plain(tex),
                        InlNode::Img { alt, .. } => *alt = self.plain(alt),
                        _ => {}
                    }
                    k.push(Inl::N(n));
                }
            }
        }
    }

    /// 一串字里的记号 ⇒ 引用节点，前后的字照旧。
    fn split(&self, s: &str, out: &mut Vec<Inl>) {
        let mut rest = s;
        let mut text = String::new();
        while let Some(o) = rest.find(OPEN) {
            let after = &rest[o + OPEN.len_utf8()..];
            let Some((n, tail)) = after
                .split_once(CLOSE)
                .and_then(|(n, t)| Some((n.parse::<usize>().ok()?, t)))
            else {
                break;
            };
            let Some((x, r)) = self.kept.get(n) else {
                break;
            };
            text.push_str(&rest[..o]);
            if !text.is_empty() {
                out.push(Inl::S(std::mem::take(&mut text)));
            }
            out.push(Inl::N(InlNode::Ref {
                x: x.clone(),
                ids: r.ids.clone(),
                label: r.label.clone(),
            }));
            rest = tail;
        }
        text.push_str(rest);
        if !text.is_empty() {
            match out.last_mut() {
                Some(Inl::S(prev)) => prev.push_str(&text),
                _ => out.push(Inl::S(text)),
            }
        }
    }

    /// 没落进字里的记号换回原文。
    fn plain(&self, s: &str) -> String {
        let mut out = String::new();
        let mut parts = Vec::new();
        self.split(s, &mut parts);
        for p in parts {
            match p {
                Inl::S(t) => out.push_str(&t),
                Inl::N(InlNode::Ref { x, .. }) => out.push_str(&x),
                _ => {}
            }
        }
        out
    }
}

struct Walk<'a> {
    text: String,
    cp: usize,
    refs: &'a [&'a RefAt],
    next: usize,
    started: bool,
}

impl<'a> Walk<'a> {
    fn new(refs: &'a [&'a RefAt]) -> Self {
        Walk {
            text: String::new(),
            cp: 0,
            refs,
            next: 0,
            started: false,
        }
    }

    /// 起点已经过去了的（落在占位字 · 接缝上 · 跨了串的）不再等。
    fn skip_past(&mut self) {
        while self.refs.get(self.next).is_some_and(|r| r.start < self.cp) {
            self.next += 1;
        }
    }

    fn gap(&mut self) {
        self.text.push(GAP);
        self.cp += 1;
        self.skip_past();
    }

    fn seam(&mut self) {
        if self.started {
            self.text.push_str(SEAM);
            self.cp += SEAM.len();
            self.skip_past();
        }
        self.started = true;
    }

    fn blocks(&mut self, blocks: &mut [MdBlock]) {
        for b in blocks {
            match b {
                MdBlock::P { k } | MdBlock::H { k, .. } => {
                    self.seam();
                    self.inls(k);
                }
                MdBlock::Code { .. } | MdBlock::Math { .. } | MdBlock::Rule => {
                    self.seam();
                    self.gap();
                }
                MdBlock::List { items, .. } => {
                    items.iter_mut().for_each(|i| self.blocks(&mut i.blocks))
                }
                MdBlock::Quote { blocks } | MdBlock::Alert { blocks, .. } => self.blocks(blocks),
                MdBlock::Table { head, rows, .. } => {
                    for cell in head.iter_mut().chain(rows.iter_mut().flatten()) {
                        self.seam();
                        self.inls(cell);
                    }
                }
                MdBlock::Notes { items } => {
                    items.iter_mut().for_each(|i| self.blocks(&mut i.blocks))
                }
            }
        }
    }

    fn inls(&mut self, k: &mut Vec<Inl>) {
        let old = std::mem::take(k);
        for i in old {
            match i {
                Inl::S(s) => self.run(&s, k),
                Inl::N(mut n) => {
                    match &mut n {
                        InlNode::B { k: inner }
                        | InlNode::I { k: inner }
                        | InlNode::Del { k: inner }
                        | InlNode::A { k: inner, .. } => self.inls(inner),
                        _ => self.gap(),
                    }
                    k.push(Inl::N(n));
                }
            }
        }
    }

    /// 一串字：起止都落在这一串里（且在字边界上）的那几处换成引用节点。
    fn run(&mut self, s: &str, out: &mut Vec<Inl>) {
        self.text.push_str(s);
        let start = self.cp;
        // 每个字边界：UTF-16 下标（相对这一串）⇒ 字节。
        let mut bounds: Vec<(usize, usize)> = Vec::with_capacity(s.len() + 1);
        let mut u = 0;
        for (b, c) in s.char_indices() {
            bounds.push((u, b));
            u += c.len_utf16();
        }
        bounds.push((u, s.len()));
        let n = u;
        let byte = |at: usize| {
            bounds
                .binary_search_by_key(&at, |p| p.0)
                .ok()
                .map(|i| bounds[i].1)
        };
        let mut cut = 0usize;
        while let Some(r) = self.refs.get(self.next) {
            if r.start >= start + n {
                break;
            }
            self.next += 1;
            if r.start < start + cut || r.end > start + n {
                continue;
            }
            let (Some(c), Some(a), Some(b)) =
                (byte(cut), byte(r.start - start), byte(r.end - start))
            else {
                continue;
            };
            if a > c {
                out.push(Inl::S(s[c..a].to_string()));
            }
            out.push(Inl::N(InlNode::Ref {
                x: s[a..b].to_string(),
                ids: r.ids.clone(),
                label: r.label.clone(),
            }));
            cut = r.end - start;
        }
        let c = byte(cut).unwrap_or(s.len());
        if c < s.len() {
            out.push(Inl::S(s[c..].to_string()));
        }
        self.cp = start + n;
    }
}
