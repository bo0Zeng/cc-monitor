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

/// 短写的计数（`2.1M` · `25K` · `999`）：不满一千写整数；往上按千进位到 K · M · B，不满 10 带一位小数、否则取整（四舍五入，
/// 先化成整数再算，不靠浮点格式化）；舍到 1000 的进一档。单位格住文案表 `countFormat.unit.*`。
pub fn count_short(n: u64) -> String {
    const UNITS: u32 = 3;
    if n < 1_000 {
        return n.to_string();
    }
    let mut u = 1u32;
    while u < UNITS && u128::from(n) >= 1_000u128.pow(u + 1) {
        u += 1;
    }
    let round = |u: u32, scale: u128| {
        let div = 1_000u128.pow(u);
        (u128::from(n) * scale + div / 2) / div
    };
    let (u, num) = loop {
        let tenths = round(u, 10);
        if tenths < 100 {
            break (u, format!("{}.{}", tenths / 10, tenths % 10));
        }
        let whole = round(u, 1);
        if whole >= 1_000 && u < UNITS {
            u += 1;
            continue;
        }
        break (u, whole.to_string());
    };
    match u {
        1 => crate::copy_text("countFormat.unit.k", &[("n", &num)]),
        2 => crate::copy_text("countFormat.unit.m", &[("n", &num)]),
        _ => crate::copy_text("countFormat.unit.b", &[("n", &num)]),
    }
}
