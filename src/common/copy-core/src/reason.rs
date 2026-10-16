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

/// 起一个子进程起不来的原因词：程序不在 ⇒ 「未装」（起进程那一口的 `NotFound` 说的是程序，不是某条路径）；其余同 [`io_reason`]。
pub fn spawn_reason(kind: std::io::ErrorKind) -> String {
    match kind {
        std::io::ErrorKind::NotFound => copy_text("reason.spawn.notInstalled", &[]),
        k => io_reason(k),
    }
}

/// 一个 SFTP 状态码（v3 标准码的数）→ 原因词：无此文件 · 无权限同 IO 那两个词，坏报文是两端协议对不上（程序出错），
/// 无连接 · 连接断是这一趟走的那条连接断了（「连接断开」，不是机器状态的「离线」），不支持就是不支持；
/// 通用失败（服务端把盘满等都归这一码）与没认出的码一律「原因不明」。
pub fn sftp_status_reason(code: u32) -> String {
    match code {
        2 => io_reason(std::io::ErrorKind::NotFound),
        3 => io_reason(std::io::ErrorKind::PermissionDenied),
        5 => copy_text("reason.code.bug", &[]),
        6 | 7 => copy_text("reason.link.lost", &[]),
        8 => copy_text("reason.sftp.unsupported", &[]),
        _ => io_reason(std::io::ErrorKind::Other),
    }
}

/// 一条写命令的码（文件管理那一族的闭集）→ 原因词；`io_failed` 照下层原话里系统那个错号说（`(os error N)`，没有 ⇒「原因不明」）。
/// 一组里没做成的那一项只带码与原话，原因词在这里定（全仓这一张表只住这里）。
pub fn code_reason(code: &str, raw: Option<&str>) -> String {
    match code {
        "exists" => io_reason(std::io::ErrorKind::AlreadyExists),
        "refused" | "bad_path" => copy_text("reason.code.refused", &[]),
        "stale" => copy_text("reason.code.stale", &[]),
        "no_unix_mode" => copy_text("reason.code.unsupported", &[]),
        "bad_name" => copy_text("reason.code.badName", &[]),
        "busy" => io_reason(std::io::ErrorKind::ResourceBusy),
        "too_many_transfers" => copy_text("reason.code.overLimit", &[]),
        _ => io_reason(os_kind(raw.unwrap_or_default())),
    }
}

/// 系统原话里那个错号（`... (os error 13)`）→ 那一种 IO 错；没有错号 ⇒ `Other`。
fn os_kind(raw: &str) -> std::io::ErrorKind {
    raw.rsplit_once("(os error ")
        .and_then(|(_, tail)| tail.split_once(')'))
        .and_then(|(n, _)| n.trim().parse::<i32>().ok())
        .map(|n| std::io::Error::from_raw_os_error(n).kind())
        .unwrap_or(std::io::ErrorKind::Other)
}

#[cfg(test)]
#[path = "../../../../tests/common/copy-core/reason_tests.rs"]
mod tests;
