//! 正在长的一段正文（活卡里的一个文字块）：每接上一截字，只重解析最后一个顶层块往后的那一截。
//!
//! 前面的顶层块已经定了：后面再来的字只会接在最后一块上，或者在它后面另起几块。所以手上留着「最后一个顶层块起在原文哪一行」，
//! 接上一截就从那一行起交给解析器，回 `(keep, tail)`：前 `keep` 块不变，从第 `keep` 块起换成 `tail`。
//! 例外：链接引用定义 · 脚注定义写在前面的块里、后面才用到的，这里认不到（尾巴解析时看不见它们）；记录落地那一份是整段解析的、以它为准。
//! 不分段（活卡不按段建）、不收脚注块。

use crate::md::MdBlock;
use crate::parse::parse_top;

/// 一段正在长的正文（见模块头注）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Growing {
    src: String,
    blocks: Vec<MdBlock>,
    /// 最后一个顶层节点起在 `src` 的哪个字节（那一行行首）· 它的块从 `blocks` 的第几块起。
    last: (usize, usize),
}

impl Growing {
    /// 接上一截字。回：前几块不变 · 从那里起换成的那几块。
    pub fn push(&mut self, s: &str) -> (usize, &[MdBlock]) {
        self.src.push_str(s);
        let (from, keep) = self.last;
        let top = parse_top(&self.src[from..]);
        self.blocks.truncate(keep);
        let mut last = self.last;
        for (at, bs) in top {
            last = (from + at, self.blocks.len());
            self.blocks.extend(bs);
        }
        self.last = last;
        (keep, &self.blocks[keep..])
    }

    /// 整段换成 `src`、从头解析（截过头之后）。
    pub fn reset(&mut self, src: String) {
        *self = Self::default();
        self.push(&src);
    }

    /// 此刻的原文。
    pub fn src(&self) -> &str {
        &self.src
    }

    /// 此刻的顶层块。
    pub fn blocks(&self) -> &[MdBlock] {
        &self.blocks
    }
}
