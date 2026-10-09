//! Claude 各型号的定价（美元 / 每百万 token；第一方 API 标准价）—— 会话花费成品按它算（注册表 `RecordFace.price`）。
//!
//! 写缓存：5 分钟档 1.25×、1 小时档 2× 输入价；读缓存一般 0.1× 输入价，个别型号另定（表里直接写读价）。
//! 不算的：批量折扣 · 快速模式加价 · 长上下文加价 · 第三方云的价。认不出的型号 ⇒ `None`（花费不算它，成品里单列）。

use crate::agents::Rates;

/// 每百万 token 的价（美元）：输入 · 输出 · 读缓存。按型号名前缀认，长的写在前面。
const TABLE: &[(&str, f64, f64, f64)] = &[
    ("claude-fable-5-1", 10.0, 50.0, 0.25),
    ("claude-mythos-5-1", 10.0, 50.0, 0.25),
    ("claude-fable-5", 10.0, 50.0, 1.0),
    ("claude-mythos-5", 10.0, 50.0, 1.0),
    ("claude-opus-5-5", 4.0, 20.0, 0.20),
    ("claude-opus-5", 5.0, 25.0, 0.5),
    ("claude-opus-4-8", 5.0, 25.0, 0.5),
    ("claude-opus-4-7", 5.0, 25.0, 0.5),
    ("claude-opus-4-6", 5.0, 25.0, 0.5),
    ("claude-opus-4-5", 5.0, 25.0, 0.5),
    ("claude-opus-4-1", 15.0, 75.0, 1.5),
    ("claude-opus-4", 15.0, 75.0, 1.5),
    ("claude-sonnet-5-5", 2.0, 10.0, 0.20),
    ("claude-sonnet-5", 2.0, 10.0, 0.20),
    ("claude-sonnet-4", 3.0, 15.0, 0.3),
    ("claude-haiku-4-5", 1.0, 5.0, 0.1),
    ("claude-3-7-sonnet", 3.0, 15.0, 0.3),
    ("claude-3-5-sonnet", 3.0, 15.0, 0.3),
    ("claude-3-5-haiku", 0.8, 4.0, 0.08),
    ("claude-3-haiku", 0.25, 1.25, 0.03),
    ("claude-3-opus", 15.0, 75.0, 1.5),
];

/// 型号名 ⇒ 它的价（微美元 / 每百万 token）；认不出 ⇒ `None`。
pub(crate) fn rates(model: &str) -> Option<Rates> {
    let m = model.trim().to_ascii_lowercase();
    let &(_, input, output, read) = TABLE.iter().find(|(p, ..)| m.starts_with(p))?;
    let micro = |usd: f64| (usd * 1_000_000.0).round() as u64;
    Some(Rates {
        input: micro(input),
        output: micro(output),
        cache_read: micro(read),
        cache_write5m: micro(input * 1.25),
        cache_write1h: micro(input * 2.0),
    })
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/price_tests.rs"]
mod tests;
