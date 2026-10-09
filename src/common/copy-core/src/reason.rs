//! **下层 IO 错 → 原因词**（句子里「失败 · X」那一格）：常见几种说人话，其余「原因不明」。
//! 只给词，不带原话 —— 原话交给出错那一端写进复制详情（[`crate::detail`]）。全仓这一张表只住这里。

use crate::copy_text;

/// 一种 IO 错的原因词（都在文案规范的原因词闭集里，判据住 `tests/common/copy-core/reason_tests.rs`）。
pub fn io_reason(kind: std::io::ErrorKind) -> String {
    use std::io::ErrorKind as K;
    match kind {
        K::PermissionDenied => copy_text("reason.io.denied", &[]),
        K::NotFound => copy_text("reason.io.notFound", &[]),
        K::StorageFull | K::QuotaExceeded => copy_text("reason.io.full", &[]),
        K::ResourceBusy => copy_text("reason.io.busy", &[]),
        K::AlreadyExists => copy_text("reason.io.exists", &[]),
        _ => copy_text("reason.io.unknown", &[]),
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/copy-core/reason_tests.rs"]
mod tests;
