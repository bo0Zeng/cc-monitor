//! 计数（几个字 · 几条 ⇒ 给人看的一格）：核心写进成品的计数都经 [`count_text`]；界面那一个读口 `format.ts::countText`
//! 与它对同一份金样 `tests/__fixtures__/count-text.golden.json`。不带单位（单位在各自那句文案里）。

/// 整数，从个位起每三位一个英文逗号（`1234567` ⇒ `1,234,567`）。
pub fn count_text(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}
