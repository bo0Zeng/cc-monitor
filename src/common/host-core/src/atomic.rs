//! 前端自有状态文件（不是用户文件）的原子覆盖，与前端两个包整读一份文件的有上限的读。
//!
//! 原子覆盖本身只有一处（`platform_core::atomic::write_atomic`：旁名只给本人 → 写满 → 落盘 → 换上去，Windows 上保目标 ACL）；
//! 这里只留 JSON 那一层：序列化 · 建父目录 · 起旁名。
//! **作用范围限于** monitor 数据目录（`~/.cc-monitor/`）下前端自己的产物（详 IPC-PROTOCOL.md § 通用约束）；
//! 用户文件一律经后端文件管理面写（INVARIANTS § 4）。今天的调用方：文件窗口的书签。

use relay_route_core::{Cap, CappedAll};

/// 一个进程里第几次写（临时件名字里带它：同一毫秒两个线程各写各的）。
static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 原子 JSON 写入（经 `platform_core::atomic::write_atomic`）。失败时目标原样、不留旁名。
///
/// 写的都是 monitor 自己的东西 ⇒ 只给本人：缺的父目录建成 0700、临时件出生即 0600（unix；换名上位后就是这份的权限），
/// 已在的目录不动。别的平台照常建（那边按父目录继承 ACL）。
///
/// 设计权衡：本 helper 不做 backup / 写后校验；这里只解决「原子覆盖」，避免 `remove + rename` 中间 crash 丢文件。
pub fn atomic_write_json<T: serde::Serialize>(
    path: &std::path::Path,
    value: &T,
) -> std::io::Result<()> {
    use std::io::Write as _;
    if let Some(parent) = path.parent() {
        platform_core::dir::private_dir_all(parent)?;
    }
    let body = serde_json::to_string_pretty(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let fname = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "out.json".to_string());
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_file_name(format!("{fname}.ccm-tmp-{ms}-{}-{seq}", std::process::id()));
    platform_core::atomic::write_atomic(
        path,
        &tmp,
        platform_core::atomic::create_new_private,
        |f| f.write_all(body.as_bytes()),
    )
    .map_err(|fail| fail.err)
}

/// 有上限整读一份文件的结局（`T`：原文 [`String`] 或字节 `Vec<u8>`）。
#[derive(Debug, PartialEq, Eq)]
pub enum Capped<T> {
    /// 不在。
    Absent,
    /// 整份（不超过上限）。
    Whole(T),
    /// 超过上限：不交半份；超了之后怎么办照调用方那道 [`Cap`] 的 `over` 那一档。
    TooBig,
}

/// 整读一份文件，最多 `cap.bytes` 字节（上限是调用方声明的那道 [`Cap`]，如声明表 `relay_route_core::CONFIG_FILE` 那一行的）。
/// 读经 [`relay_route_core::read_capped_from`]：只读 `cap.bytes + 1` 个字节分辨「刚好读满」与「其实还有」，不按文件大小分配。
/// 前端两个包整读一份文件只经它与 [`read_capped`]。
pub fn read_capped_bytes(path: &std::path::Path, cap: Cap) -> std::io::Result<Capped<Vec<u8>>> {
    let f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Capped::Absent),
        Err(e) => return Err(e),
    };
    Ok(match relay_route_core::read_capped_from(f, cap)? {
        CappedAll::All(buf) => Capped::Whole(buf),
        CappedAll::OverCap => Capped::TooBig,
    })
}

/// 同 [`read_capped_bytes`]，交回原文：不是 UTF-8 ⇒ `InvalidData`。
pub fn read_capped(path: &std::path::Path, cap: Cap) -> std::io::Result<Capped<String>> {
    match read_capped_bytes(path, cap)? {
        Capped::Whole(buf) => String::from_utf8(buf)
            .map(Capped::Whole)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        Capped::Absent => Ok(Capped::Absent),
        Capped::TooBig => Ok(Capped::TooBig),
    }
}
