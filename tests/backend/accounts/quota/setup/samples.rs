//! 判据拿来当策略跑的三份样例：照号列表 · 离 5 小时重置近的权重高 · 余量多的权重高。每份 ＝ 头 ＋ 它自己的权重 `weight`
//! ＋ 三份共用的那一段换号的写法（`samples/turn.rhai`），拼出来是一份自足的策略文件。只在测试里：不进策略目录、不进任何默认。

const SAMPLES: [&str; 3] = [
    concat!(include_str!("samples/by-order.rhai"), include_str!("samples/turn.rhai")),
    concat!(include_str!("samples/by-reset.rhai"), include_str!("samples/turn.rhai")),
    concat!(include_str!("samples/by-headroom.rhai"), include_str!("samples/turn.rhai")),
];

/// 第 `n` 份样例的原文（`n` ＝ 0 照号列表 · 1 离重置近 · 2 余量多；大于 2 ⇒ 最后一份）。
pub(crate) fn of(n: usize) -> &'static str {
    SAMPLES[n.min(SAMPLES.len() - 1)]
}
