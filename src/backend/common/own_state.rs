//! 后端自有落盘文件的声明、读三态与编码。写经 `platform::fs::Own`（原子写的本体只住那里）；读只经这里。
//!
//! - 声明：每份落盘文件一处 [`Doc`]（落点 `rel` · 读上限 `cap` · 它是哪一种 [`Kind`]），住写它的那个模块；
//!   经 `Own` 写的模块都得有（判据扫）。
//! - 读：[`Doc::read`] / [`Doc::read_with`] / [`Doc::read_stream`] 交三态 —— 不在 / 读得到 / 读不出来。只读常规文件、限量读。
//!   **状态**读不出来 ⇒ `Unreadable`（调用方不覆盖、照实说出来）；**缓存**读不出来 ⇒ 当没有（`Absent`，记一行日志）。
//! - 编码：[`json_body`] 交要写的那一份字节（紧凑 JSON ＋ 收尾换行），由调用方在那个目录的锁里经 `Own::write` 落盘。

use crate::common::said::Said;
use copy_core::copy_text;
use std::io::Read as _;
use std::marker::PhantomData;
use std::path::Path;

/// 一份落盘文件是哪一种：决定读不出来时怎么办。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// 状态（丢了就没了）：读不出来 ⇒ `Unreadable`，调用方不覆盖、照实说出来。
    State,
    /// 缓存（照别的东西能重建）：读不出来 ⇒ 当没有。
    Cache,
}

/// 一份落盘文件的声明。`T` 是读成的那一形。
pub(crate) struct Doc<T> {
    /// 落点（家目录底下 `.cc-monitor/` 那一层起；一类多份的写成那一类的样子，如 `…/<pid>.json`；不住那一层的写成它住哪）。
    pub(crate) rel: &'static str,
    /// 读上限（字节）：超了当读不出来。
    pub(crate) cap: u64,
    pub(crate) kind: Kind,
    of: PhantomData<fn() -> T>,
}

impl<T> Doc<T> {
    pub(crate) const fn state(rel: &'static str, cap: u64) -> Self {
        Doc {
            rel,
            cap,
            kind: Kind::State,
            of: PhantomData,
        }
    }

    pub(crate) const fn cache(rel: &'static str, cap: u64) -> Self {
        Doc {
            rel,
            cap,
            kind: Kind::Cache,
            of: PhantomData,
        }
    }

    /// 读整份字节、由 `decode` 解成 `T`（JSON 之外的格式 · 解之前要先动一下原文的）。
    pub(crate) fn read_with(
        &self,
        path: &Path,
        decode: impl FnOnce(Vec<u8>) -> Result<T, Said>,
    ) -> Read<T> {
        self.settle(
            path,
            read_bytes(path, self.cap).and_then(|b| decoded(decode(b))),
        )
    }

    /// 流式读（大的缓存不先攒成整份）：`decode` 从限量的读者里读。读成哪一形由 `decode` 定（一类多份、各份形状不同的那几处）。
    pub(crate) fn read_stream<U>(
        &self,
        path: &Path,
        decode: impl FnOnce(&mut dyn std::io::Read) -> Result<U, Said>,
    ) -> Read<U> {
        let got = match opened(path, self.cap) {
            Read::Absent => Read::Absent,
            Read::Unreadable(e) => Read::Unreadable(e),
            Read::Present(f) => {
                let mut r = std::io::BufReader::new(f.take(self.cap));
                decoded(decode(&mut r))
            }
        };
        self.kind_of::<U>().settle(path, got)
    }

    /// 按 JSON 读成另一形（升级链 · 换形那几处读原文 `Value`；读上限与哪一种照这份声明）。
    pub(crate) fn read_as<U: serde::de::DeserializeOwned>(&self, path: &Path) -> Read<U> {
        self.kind_of::<U>().read(path)
    }

    fn kind_of<U>(&self) -> Doc<U> {
        Doc {
            rel: self.rel,
            cap: self.cap,
            kind: self.kind,
            of: PhantomData,
        }
    }

    /// 照这份是哪一种落定：缓存读不出来 ⇒ 当没有（记一行日志）。
    fn settle(&self, path: &Path, got: Read<T>) -> Read<T> {
        match (self.kind, got) {
            (Kind::Cache, Read::Unreadable(e)) => {
                tracing::warn!(
                    "cache {} unreadable, treated as absent: {}",
                    path.display(),
                    e.logged()
                );
                Read::Absent
            }
            (_, got) => got,
        }
    }
}

impl<T: serde::de::DeserializeOwned> Doc<T> {
    /// 读整份并按 JSON 解成 `T`。
    pub(crate) fn read(&self, path: &Path) -> Read<T> {
        self.settle(path, read_json(path, self.cap))
    }
}

fn decoded<T>(r: Result<T, Said>) -> Read<T> {
    match r {
        Ok(t) => Read::Present(t),
        Err(e) => Read::Unreadable(e),
    }
}

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
fn read_bytes(path: &Path, cap: u64) -> Read<Vec<u8>> {
    match crate::platform::fs::Ro::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Read::Absent,
        _ => {}
    }
    match crate::platform::fs::Ro::read_regular_capped(path, cap) {
        Ok(b) => Read::Present(b),
        Err(e) => Read::Unreadable(failed(path, e)),
    }
}

/// 打开来流式读（同 [`read_bytes`] 的三态与上限：先认常规文件、先看长度；读的那一侧再 `take(cap)`）。
fn opened(path: &Path, cap: u64) -> Read<crate::platform::fs::RoFile> {
    let meta = match crate::platform::fs::Ro::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Read::Absent,
        Err(e) => {
            return Read::Unreadable(failed(
                path,
                Said::with_raw(copy_core::io_reason(e.kind()), e),
            ))
        }
        Ok(m) => m,
    };
    let refused = if !meta.is_file() {
        Some(copy_text("reason.fs.notRegular", &[]))
    } else if meta.len() > cap {
        Some(copy_text(
            "beFs.readRegularCapped.tooBigSized",
            &[("cap", &cap.to_string()), ("size", &meta.len().to_string())],
        ))
    } else {
        None
    };
    if let Some(why) = refused {
        return Read::Unreadable(failed(path, why.into()));
    }
    match crate::platform::fs::Ro::open(path) {
        Ok(f) => Read::Present(f),
        Err(e) => Read::Unreadable(failed(
            path,
            Said::with_raw(copy_core::io_reason(e.kind()), e),
        )),
    }
}

/// 读不出来的那一句：路径 ＋ 原因词；下层原话照留。
fn failed(path: &Path, e: Said) -> Said {
    Said {
        said: copy_text(
            "reason.fs.readFailed",
            &[("path", &path.display().to_string()), ("why", &e.said)],
        ),
        raw: e.raw,
    }
}

/// 读整份并按 JSON 解成 `T`。解不开 ⇒ `Unreadable`。
fn read_json<T: serde::de::DeserializeOwned>(path: &Path, cap: u64) -> Read<T> {
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

#[cfg(test)]
#[path = "../../../tests/backend/common/own_doc_guard.rs"]
mod doc_guard;
