//! 大小（字节 ⇒ 给人看的一格）：后端 · 壳 · 文件窗口写大小都经 [`size_text`]；界面那一个读口 `format.ts::sizeText`、
//! 手机端 `SizeFormat` 与它对同一份金样 `tests/__fixtures__/size-text.golden.json`。单位格住文案表 `sizeFormat.unit.*`。

use crate::copy_text;

/// 单位有几档（B · KB · MB · GB · TB）。
const UNITS: usize = 5;

/// 第 `u` 档单位写上数（文案键逐档写成字面量：表里每一格都有人引用）。
fn unit(u: usize, n: &str) -> String {
    match u {
        0 => copy_text("sizeFormat.unit.b", &[("n", n)]),
        1 => copy_text("sizeFormat.unit.kb", &[("n", n)]),
        2 => copy_text("sizeFormat.unit.mb", &[("n", n)]),
        3 => copy_text("sizeFormat.unit.gb", &[("n", n)]),
        _ => copy_text("sizeFormat.unit.tb", &[("n", n)]),
    }
}

/// 不满 1 KB 写整数字节；往上按 1024 进位、最多到 TB，一位小数（四舍五入：先化成十分位整数再算，两边不靠浮点格式化对齐）；
/// 舍入到 1024.0 的进一档（`1048575` ⇒ `1.0 MB`）。
pub fn size_text(n: u64) -> String {
    let mut u = 0usize;
    while u + 1 < UNITS && u128::from(n) >= 1024u128.pow(u as u32 + 1) {
        u += 1;
    }
    if u == 0 {
        return unit(0, &n.to_string());
    }
    let tenths = |u: usize| {
        let div = 1024u128.pow(u as u32);
        (u128::from(n) * 10 + div / 2) / div
    };
    let mut t = tenths(u);
    if t >= 10_240 && u + 1 < UNITS {
        u += 1;
        t = tenths(u);
    }
    unit(u, &format!("{}.{}", t / 10, t % 10))
}
