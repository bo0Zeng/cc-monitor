//! 代码高亮：看见才算。树里的代码块不带高亮；一段被拉的那一下，[`Highlighter::seg`] 才算那一段里的代码块，
//! 按内容（语言 ＋ 原文）缓存，同样的内容不算第二遍。
//!
//! [`Highlighter`] 是一个值，由调用方持有（语法表与缓存都在里面）；这里没有全局状态。
//! 出的 `hl` 是平铺的三元组 `[起点差, 长, 类]…`：偏移按 UTF-16 码元，起点差相对上一段的末尾，类是 [`HlClass`] 的序号。

use crate::md::*;
use crate::segment::seg_blocks;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};

/// 语法表 ＋ 按内容键的高亮缓存（字节上限内最近最少用的先淘汰）。
pub struct Highlighter {
    ss: SyntaxSet,
    cap: usize,
    bytes: usize,
    tick: u64,
    map: HashMap<Arc<str>, Entry>,
    /// 最近一次用到的先后 ⇒ 键（最小的最久没用）。
    order: BTreeMap<u64, Arc<str>>,
    computed: u64,
}

struct Entry {
    hl: Vec<u32>,
    tick: u64,
}

impl Highlighter {
    /// `cap_bytes`：缓存的字节上限（每条按 键长 ＋ `hl` 长 × 4 计）。载语法表要几十毫秒，建一次、一直用。
    pub fn new(cap_bytes: usize) -> Self {
        Highlighter {
            ss: two_face::syntax::extra_newlines(),
            cap: cap_bytes,
            bytes: 0,
            tick: 0,
            map: HashMap::new(),
            order: BTreeMap::new(),
            computed: 0,
        }
    }

    /// 第 `i` 段里每个代码块（顶层与嵌在列表 · 引用 · 提示块 · 脚注里的，先序）各一份 `hl`。
    /// 没写语言、语言认不出 ⇒ 空。段号越界 ⇒ 空表。
    pub fn seg(&mut self, doc: &Doc, i: usize) -> Vec<Vec<u32>> {
        let mut codes = Vec::new();
        collect_codes(seg_blocks(doc, i), &mut codes);
        codes
            .into_iter()
            .map(|(lang, x)| self.code(lang, x))
            .collect()
    }

    /// 真算过的次数（命中缓存的不算）。
    pub fn computed(&self) -> u64 {
        self.computed
    }

    /// 缓存此刻占的字节。
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    fn code(&mut self, lang: Option<&str>, x: &str) -> Vec<u32> {
        let Some(lang) = lang else { return Vec::new() };
        let Some(syntax) = self.ss.find_syntax_by_token(lang) else {
            return Vec::new();
        };
        let key = format!("{lang}\n{x}");
        self.tick += 1;
        if let Some(e) = self.map.get_mut(key.as_str()) {
            let k = self
                .order
                .remove(&e.tick)
                .expect("缓存的先后表与键表对不上");
            e.tick = self.tick;
            self.order.insert(self.tick, k);
            return e.hl.clone();
        }
        self.computed += 1;
        let hl = highlight(&self.ss, syntax, x);
        let size = key.len() + hl.len() * 4;
        if size > self.cap {
            return hl;
        }
        while self.bytes + size > self.cap {
            let Some((_, old)) = self.order.pop_first() else {
                break;
            };
            if let Some(e) = self.map.remove(&old) {
                self.bytes -= old.len() + e.hl.len() * 4;
            }
        }
        let key: Arc<str> = key.into();
        self.order.insert(self.tick, key.clone());
        self.map.insert(
            key,
            Entry {
                hl: hl.clone(),
                tick: self.tick,
            },
        );
        self.bytes += size;
        hl
    }
}

fn collect_codes<'d>(blocks: &'d [MdBlock], out: &mut Vec<(Option<&'d str>, &'d str)>) {
    for b in blocks {
        match b {
            MdBlock::Code { lang, x, .. } => out.push((lang.as_deref(), x)),
            MdBlock::List { items, .. } => items.iter().for_each(|i| collect_codes(&i.blocks, out)),
            MdBlock::Quote { blocks } | MdBlock::Alert { blocks, .. } => collect_codes(blocks, out),
            MdBlock::Notes { items } => items.iter().for_each(|n| collect_codes(&n.blocks, out)),
            MdBlock::P { .. }
            | MdBlock::H { .. }
            | MdBlock::Math { .. }
            | MdBlock::Table { .. }
            | MdBlock::Rule => {}
        }
    }
}

/// 作用域名 ⇒ 类；`None` ⇒ 这一层不定，往外一层看。
fn class_of_scope(name: &str) -> Option<HlClass> {
    let is =
        |p: &str| name == p || (name.starts_with(p) && name.as_bytes().get(p.len()) == Some(&b'.'));
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

/// 代码 ⇒ `hl` 三元组。语法表出错 ⇒ 空。
fn highlight(ss: &SyntaxSet, syntax: &SyntaxReference, x: &str) -> Vec<u32> {
    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut memo: HashMap<Vec<Scope>, Option<HlClass>> = HashMap::new();
    let mut spans: Vec<(u32, u32, u32)> = Vec::new();
    let mut base16: u32 = 0;
    let emit =
        |spans: &mut Vec<(u32, u32, u32)>, start16: u32, text: &str, class: Option<HlClass>| {
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
        let Ok(ops) = state.parse_line(line, ss) else {
            return Vec::new();
        };
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

fn classify(
    stack: &ScopeStack,
    memo: &mut HashMap<Vec<Scope>, Option<HlClass>>,
) -> Option<HlClass> {
    let scopes = stack.as_slice();
    if let Some(c) = memo.get(scopes) {
        return *c;
    }
    let c = scopes
        .iter()
        .rev()
        .find_map(|s| class_of_scope(&s.build_string()));
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
