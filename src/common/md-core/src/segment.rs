//! 长正文分段：按顶层块累计字数，到 [`SEG_BYTES`] 就在下一个顶层块前记一个段起点；单个顶层块就超过一段的先切开。

use crate::md::*;
use crate::parse::Inls;

/// 一段的字数上限（UTF-8 字节），同界面「长回复先画头一截」那条线。
pub const SEG_BYTES: usize = 20_000;

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

/// 第 `i` 段的顶层块（段号从 0 起）；段号越界 ⇒ 空。
pub fn seg_blocks(doc: &Doc, i: usize) -> &[MdBlock] {
    if i > doc.segs.len() {
        return &[];
    }
    let start = if i == 0 { 0 } else { doc.segs[i - 1] as usize };
    let end = doc.segs.get(i).map_or(doc.blocks.len(), |&e| e as usize);
    doc.blocks.get(start..end).unwrap_or(&[])
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
        MdBlock::Code { lang, x, .. } => split_code(lang, &x),
        MdBlock::List {
            start,
            tight,
            items,
        } => {
            let mut at = start;
            pack(items, |i| weight_blocks(&i.blocks))
                .into_iter()
                .map(|items| {
                    let here = at;
                    at = at.map(|s| s + items.len() as u32);
                    MdBlock::List {
                        start: here,
                        tight,
                        items,
                    }
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
            let next_blank = it.peek().is_none_or(|(_, n)| n.is_whitespace());
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

/// 超长代码按行切成几块（不含行间的那个换行）。
fn split_code(lang: Option<String>, x: &str) -> Vec<MdBlock> {
    let lines: Vec<&str> = x.split('\n').collect();
    pack(lines, |l| l.len() + 1)
        .into_iter()
        .enumerate()
        .map(|(i, ls)| MdBlock::Code {
            lang: lang.clone(),
            x: ls.join("\n"),
            cont: i > 0,
        })
        .collect()
}
