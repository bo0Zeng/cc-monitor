//! 一段话截成给人看的一截：截断 · 首个非空行 · 压空白，全仓 Rust 只此一份。
//!
//! 口径一种：按字符（Unicode 标量）数，留前 N 个；截了接一个「…」，省略号不占那 N 个。不按字节截，截不出半个字。

/// 截了接在后面的那一个字。
const CUT: char = '…';

/// 留前 `max` 个字；截了接一个「…」。没超 ⇒ 原样。
pub fn clip(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => {
            let mut out = String::with_capacity(i + CUT.len_utf8());
            out.push_str(&s[..i]);
            out.push(CUT);
            out
        }
        None => s.to_string(),
    }
}

/// 头一个非空行，去两头空白；没有 ⇒ `None`。
pub fn first_line(s: &str) -> Option<&str> {
    s.lines().map(str::trim).find(|l| !l.is_empty())
}

/// 连串空白（含换行）压成一个空格，去两头。
pub fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// ＝ `clip(&collapse_ws(s), max)`，只走到够数为止（一条几十万字的正文只为取几十个字，不整条压）。
pub fn clip_flat(s: &str, max: usize) -> String {
    let mut out = String::new();
    let mut n = 0;
    for word in s.split_whitespace() {
        let gap = (!out.is_empty()).then_some(' ');
        for c in gap.into_iter().chain(word.chars()) {
            if n == max {
                out.push(CUT);
                return out;
            }
            out.push(c);
            n += 1;
        }
    }
    out
}

#[cfg(test)]
#[path = "../../../../tests/common/copy-core/text_tests.rs"]
mod tests;
