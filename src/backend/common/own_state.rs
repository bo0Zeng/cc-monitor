//! 后端自有状态文件的读三态与编码。各份读自己那一份文件都经这里，不各写一套；写经 `platform::fs::Own`（原子写的本体只住那里）。
//!
//! - 读：[`read_bytes`] / [`read_json`] 交三态 —— 不在 / 读得到 / 读不出来（读不出来的那份调用方不覆盖）。只读常规文件、限量读。
//! - 编码：[`json_body`] 交要写的那一份字节（紧凑 JSON ＋ 收尾换行），由调用方在那个目录的锁里经 `Own::write` 落盘。

use crate::common::said::Said;
use copy_core::copy_text;
use std::path::Path;

/// 读一次的结果。三态，不许合并：「不在」与「读不出来」对调用方是两件事（后者不覆盖）。
/// 读不出来那一形带那一句（路径 ＋ 原因词）与下层原话（[`Said`]）：读答里放 `reason` ＋ `detail` 两格（`wire::detail::unreadable`）。
#[derive(Debug)]
pub enum Read<T> {
    Absent,
    Present(T),
    Unreadable(Said),
}

impl<T> Read<T> {
    /// 读得到时换个形状；另两态原样。
    pub(crate) fn and_then<U>(self, f: impl FnOnce(T) -> Read<U>) -> Read<U> {
        match self {
            Read::Absent => Read::Absent,
            Read::Present(t) => f(t),
            Read::Unreadable(e) => Read::Unreadable(e),
        }
    }
}

/// 读整份字节（上限 `cap`）。不在 ⇒ `Absent`；不是常规文件 / 超上限 / 读失败 ⇒ `Unreadable`（原因带路径）。
pub(crate) fn read_bytes(path: &Path, cap: u64) -> Read<Vec<u8>> {
    match crate::platform::fs::Ro::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Read::Absent,
        _ => {}
    }
    match crate::platform::fs::Ro::read_regular_capped(path, cap) {
        Ok(b) => Read::Present(b),
        Err(e) => Read::Unreadable(Said {
            said: copy_text(
                "reason.fs.readFailed",
                &[("path", &path.display().to_string()), ("why", &e.said)],
            ),
            raw: e.raw,
        }),
    }
}

/// 读整份并按 JSON 解成 `T`。解不开 ⇒ `Unreadable`。
pub(crate) fn read_json<T: serde::de::DeserializeOwned>(path: &Path, cap: u64) -> Read<T> {
    read_bytes(path, cap).and_then(|b| match serde_json::from_slice::<T>(&b) {
        Ok(t) => Read::Present(t),
        Err(e) => Read::Unreadable(Said::with_raw(
            copy_text(
                "beOwnState.read.notJson",
                &[("path", &path.display().to_string())],
            ),
            e,
        )),
    })
}

/// 整份写成紧凑 JSON、收尾一个换行：交回要写的那一份字节（经 `Own::write` 落盘）。
pub(crate) fn json_body<T: serde::Serialize + ?Sized>(path: &Path, v: &T) -> Result<Vec<u8>, Said> {
    let mut body = serde_json::to_vec(v).map_err(|e| {
        Said::with_raw(
            copy_text(
                "beOwnState.write.encodeFailed",
                &[("path", &path.display().to_string())],
            ),
            e,
        )
    })?;
    body.push(b'\n');
    Ok(body)
}

#[cfg(test)]
#[path = "../../../tests/backend/common/own_state_tests.rs"]
mod tests;
