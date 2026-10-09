//! 后端自有状态文件（`readonly_guard` 第四层）的读三态与原子写。第四层各份都经这里读写，不各写一套。
//!
//! - 读：[`read_bytes`] / [`read_json`] 交三态 —— 不在 / 读得到 / 读不出来（读不出来的那份调用方不覆盖）。只读常规文件、限量读。
//! - 写：[`write`] / [`write_json`]：旁名出生即只给本人（`O_EXCL` ＋ 0600）→ 写满 → `sync_all` → 原子挪过去；
//!   挪不过去就删掉**自己建的**那个旁名。旁名带 pid ＋ 线程 ＋ 进程内序号，同一进程几个线程同时写同一份也不撞。
//!   做读—改—写的调用方在 `platform::lock::hold` 里先读后调它（`readonly_guard` 第四层 ⑥ 钉「每份都拿锁」）。

use crate::common::said::Said;
use copy_core::copy_text;
use std::io::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// 读一次的结果。三态，不许合并：「不在」与「读不出来」对调用方是两件事（后者不覆盖）。
/// 读不出来那一形带那一句（路径 ＋ 原因词）与下层原话（[`Said`]）：读答里放 `reason` ＋ `detail` 两格（`stream::detail::unreadable`）。
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
    match std::fs::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Read::Absent,
        _ => {}
    }
    match crate::common::fs::read_regular_capped(path, cap) {
        Ok(b) => Read::Present(b),
        Err(e) => Read::Unreadable(Said {
            said: copy_text(
                "beOwnState.read.failed",
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

/// 整份写成紧凑 JSON、收尾一个换行。
pub(crate) fn write_json<T: serde::Serialize + ?Sized>(path: &Path, v: &T) -> Result<(), Said> {
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
    write(path, &body)
}

/// 进程内序号：同一线程连写两次也换一个旁名。
static SEQ: AtomicU64 = AtomicU64::new(0);

/// 整份原子写 `body`。那一句只有路径与原因词，系统原话进 `raw`；都不带内容。
pub(crate) fn write(path: &Path, body: &[u8]) -> Result<(), Said> {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(copy_text(
            "beOwnState.write.noParent",
            &[("path", &path.display().to_string())],
        )
        .into());
    };
    let tmp = dir.join(format!(
        "{}.{}.{:?}.{}.tmp",
        name.to_string_lossy(),
        std::process::id(),
        std::thread::current().id(),
        SEQ.fetch_add(1, Ordering::Relaxed),
    ));
    // 建不出来 ⇒ 那个名字上的文件不是我们的，什么都不删。
    let mut f = creds_core::perm::create_private(&tmp).map_err(|e| {
        Said::with_raw(
            copy_text(
                "beOwnState.write.tmpCreateFailed",
                &[
                    ("tmp", &tmp.display().to_string()),
                    ("why", &copy_core::io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    let result = f
        .write_all(body)
        .and_then(|()| f.sync_all())
        .map_err(|e| {
            Said::with_raw(
                copy_text(
                    "beOwnState.write.tmpWriteFailed",
                    &[
                        ("tmp", &tmp.display().to_string()),
                        ("why", &copy_core::io_reason(e.kind())),
                    ],
                ),
                &e,
            )
        })
        .and_then(|()| {
            drop(f);
            std::fs::rename(&tmp, path).map_err(|e| {
                Said::with_raw(
                    copy_text(
                        "beOwnState.write.renameFailed",
                        &[
                            ("tmp", &tmp.display().to_string()),
                            ("path", &path.display().to_string()),
                            ("why", &copy_core::io_reason(e.kind())),
                        ],
                    ),
                    &e,
                )
            })
        });
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
#[path = "../../../tests/backend/common/own_state_tests.rs"]
mod tests;
