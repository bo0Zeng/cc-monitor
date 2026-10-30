//! 后端自有落盘文件的声明、读三态与编码。写经 `platform::fs::Own`（原子写的本体只住那里）；读只经这里。
//!
//! - 声明：每份落盘文件一处 [`Doc`]（落点 `rel` · 读上限 `cap` · 它是哪一种 [`Kind`] · 格式与版本），只从声明表取
//!   （[`Doc::of`] · 挂升级步的 [`upgrade!`]，表住 `relay_route_core::OWN_FILES`），住写它的那个模块；经 `Own` 写的模块都得有（判据扫）。
//! - 读：[`Doc::read`] / [`Doc::read_with`] / [`Doc::read_stream`] 交三态 —— 不在 / 读得到 / 读不出来。只读常规文件、限量读。
//!   **状态**读不出来 ⇒ `Unreadable`（调用方不覆盖、照实说出来）；**缓存**读不出来 ⇒ 当没有（`Absent`，记一行日志）。
//! - 版本（从表取的、格式是 JSON 的那几份；[`Doc::read`] · [`Doc::read_as`]）：顶层整数 `version`，缺了当 1。
//!   比声明新 ⇒ 读不出来（状态不覆盖、缓存当没有）；比下限老 ⇒ 当没有；中间的依次过升级步；`version` 那一格摘掉再交给 `T`。
//!   流式读写的大缓存不经 `Value`：自己核顶层 `version`，值取 [`Doc::version`]。
//! - 写：[`Doc::write`] 整份写、顶层自动带 `version`；盖新版状态 ⇒ 拒；盖旧版 ⇒ 原文先照字节留成 `<名>.v<N>`。
//!   一类多份的那几样文件名另给：[`Doc::write_named`]。字节是 [`json_body`]（紧凑 JSON ＋ 收尾换行）。

use crate::common::said::Said;
use crate::platform::fs::{Own, OwnLock, OwnPath};
use copy_core::copy_text;
use relay_route_core::{Class, Decl, Format, Upgradable};
use std::io::Read as _;
use std::marker::PhantomData;
use std::path::Path;

/// 一份落盘文件是哪一种：决定读不出来时怎么办（同声明表那一格）。
pub(crate) use relay_route_core::Kind;

/// 一步升级：第 `N` 版的原文 ⇒ 第 `N + 1` 版（纯函数）。
pub(crate) type Step = fn(serde_json::Value) -> Result<serde_json::Value, Said>;

/// 一份落盘文件的声明。`T` 是读成的那一形。
pub(crate) struct Doc<T> {
    /// 落点（家目录底下 `.cc-monitor/` 那一层起；一类多份的写成那一类的样子，如 `…/<pid>.json`；不住那一层的写成它住哪）。
    pub(crate) rel: &'static str,
    /// 读上限（字节）：超了当读不出来。
    pub(crate) cap: u64,
    pub(crate) kind: Kind,
    /// 格式与版本（声明表那一格）。
    format: Format,
    /// 升级步：第 `floor + i` 版 ⇒ 下一版。
    steps: &'static [Step],
    of: PhantomData<fn() -> T>,
}

/// 挂升级步：`upgrade!(声明, [第 floor 版 ⇒ 下一版, …])`。只收状态（缓存挂不上，编不过）；步数要等于 `version - floor`。
macro_rules! upgrade {
    ($decl:expr, [$($step:expr),+ $(,)?]) => {
        $crate::common::own_state::Doc::upgraded(
            relay_route_core::upgradable(&$decl),
            &[$($step),+],
        )
    };
}
pub(crate) use upgrade;

impl<T> Doc<T> {
    /// 从声明表的一行取。读得懂的不止这一版（下限低于版本）⇒ 要挂升级步，用 [`upgrade!`]。
    pub(crate) const fn of<K: Class>(d: &Decl<K>) -> Self {
        Self::upgraded_any(d, &[])
    }

    /// [`upgrade!`] 展开成它（种类的闸在 `relay_route_core::upgradable`）。
    pub(crate) const fn upgraded<K: Upgradable>(d: &Decl<K>, steps: &'static [Step]) -> Self {
        Self::upgraded_any(d, steps)
    }

    const fn upgraded_any<K: Class>(d: &Decl<K>, steps: &'static [Step]) -> Self {
        let span = match d.row.format {
            Format::Json { version, floor } => (version - floor) as usize,
            Format::Foreign => 0,
        };
        assert!(steps.len() == span, "升级步数要等于 version - floor");
        Doc {
            rel: d.row.rel,
            cap: d.row.cap,
            kind: d.row.kind,
            format: d.row.format,
            steps,
            of: PhantomData,
        }
    }

    /// 声明的版本（格式是 JSON 的那几份；别的在编译期就拒）。流式读写的那几份大缓存自己核这一格，值只从这里来。
    pub(crate) const fn version(&self) -> u32 {
        match self.format {
            Format::Json { version, .. } => version,
            Format::Foreign => panic!("只有 JSON 那几份有版本"),
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

    /// 按 JSON 读成另一形（换形那几处读原文 `Value`；读上限 · 哪一种 · 版本照这份声明）。
    pub(crate) fn read_as<U: serde::de::DeserializeOwned>(&self, path: &Path) -> Read<U> {
        self.kind_of::<U>().read(path)
    }

    fn kind_of<U>(&self) -> Doc<U> {
        Doc {
            rel: self.rel,
            cap: self.cap,
            kind: self.kind,
            format: self.format,
            steps: self.steps,
            of: PhantomData,
        }
    }

    /// 盘上那一份原文 ⇒ 这一版的原文（`version` 那一格已摘掉）：比声明新 ⇒ 读不出来；比下限老 ⇒ 当没有；中间的依次过升级步。
    fn current(
        &self,
        path: &Path,
        mut v: serde_json::Value,
        version: u32,
        floor: u32,
    ) -> Read<serde_json::Value> {
        let found = match on_disk_version(path, &v) {
            Ok(n) => n,
            Err(e) => return Read::Unreadable(e),
        };
        if found > u64::from(version) {
            return Read::Unreadable(newer(path, found, version));
        }
        if found < u64::from(floor) {
            tracing::warn!(
                "{} is format {found}, older than this backend reads ({floor}..={version}); treated as absent",
                path.display()
            );
            return Read::Absent;
        }
        let from = (found - u64::from(floor)) as usize;
        for step in &self.steps[from..] {
            v = match step(v) {
                Ok(v) => v,
                Err(e) => return Read::Unreadable(e),
            };
        }
        if let Some(o) = v.as_object_mut() {
            o.remove(VERSION_KEY);
        }
        Read::Present(v)
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
    /// 读整份并按 JSON 解成 `T`（JSON 那几份照版本，见模块头）。
    pub(crate) fn read(&self, path: &Path) -> Read<T> {
        let got = match self.format {
            Format::Json { version, floor } => read_json::<serde_json::Value>(path, self.cap)
                .and_then(|v| self.current(path, v, version, floor))
                .and_then(|v| match serde_json::from_value::<T>(v) {
                    Ok(t) => Read::Present(t),
                    Err(e) => Read::Unreadable(not_json(path, e)),
                }),
            Format::Foreign => read_json(path, self.cap),
        };
        self.settle(path, got)
    }
}

impl<T: serde::Serialize> Doc<T> {
    /// 整份写进 `dir` 底下这份（名字是落点的最后一截），`held` 是 `dir` 的锁。从表取的 JSON：顶层带 `version`；
    /// 盘上那一份是状态且比声明新 ⇒ 拒、一个字节不动；比声明老 ⇒ 原文先照字节留成 `<名>.v<N>`（已有不动）。
    pub(crate) fn write(
        &self,
        own: &Own,
        dir: &OwnPath,
        held: &OwnLock,
        v: &T,
    ) -> Result<(), Said> {
        self.write_named(own, dir, relay_route_core::file_name_of(self.rel), held, v)
    }

    /// 同 [`Doc::write`]，文件名另给：一类多份的那几样（落点只写得出那一类的样子，如 `…/<sid>.json`）。
    pub(crate) fn write_named(
        &self,
        own: &Own,
        dir: &OwnPath,
        name: &str,
        held: &OwnLock,
        v: &T,
    ) -> Result<(), Said> {
        let at = dir.join(name)?;
        let body = match self.format {
            Format::Json { version, .. } => {
                if self.kind == Kind::State {
                    self.keep_older(own, dir, name, &at, held, version)?;
                }
                versioned_body(at.as_path(), v, version)?
            }
            Format::Foreign => json_body(at.as_path(), v)?,
        };
        own.write(&at, held, &body)
    }

    /// 盖之前看一眼盘上那一份：新版 ⇒ 拒；旧版 ⇒ 留一份 `<名>.v<N>`。不在 · 读不懂 ⇒ 不管（读不懂的盖不盖由调用方定）。
    fn keep_older(
        &self,
        own: &Own,
        dir: &OwnPath,
        name: &str,
        at: &OwnPath,
        held: &OwnLock,
        version: u32,
    ) -> Result<(), Said> {
        let Read::Present(bytes) = read_bytes(at.as_path(), self.cap) else {
            return Ok(());
        };
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            return Ok(());
        };
        let Ok(found) = on_disk_version(at.as_path(), &v) else {
            return Ok(());
        };
        if found > u64::from(version) {
            return Err(newer(at.as_path(), found, version));
        }
        if found < u64::from(version) {
            let keep = dir.join(&format!("{name}.v{found}"))?;
            if matches!(read_bytes(keep.as_path(), self.cap), Read::Absent) {
                own.write(&keep, held, &bytes)?;
            }
        }
        Ok(())
    }
}

/// 版本那一格的名字（顶层）。
const VERSION_KEY: &str = "version";

/// 盘上那一份的版本：缺了当 1；不是非负整数 ⇒ 读不懂。
fn on_disk_version(path: &Path, v: &serde_json::Value) -> Result<u64, Said> {
    match v.get(VERSION_KEY) {
        None => Ok(1),
        Some(n) => n
            .as_u64()
            .ok_or_else(|| not_json(path, format!("`{VERSION_KEY}` is not an integer: {n}"))),
    }
}

/// 比这一版新的那一句（读 · 写同一句）。
fn newer(path: &Path, found: u64, version: u32) -> Said {
    copy_text(
        "beOwnState.read.newer",
        &[
            ("path", &path.display().to_string()),
            ("mine", &version.to_string()),
            ("theirs", &found.to_string()),
        ],
    )
    .into()
}

fn not_json(path: &Path, raw: impl std::fmt::Display) -> Said {
    Said::with_raw(
        copy_text(
            "beOwnState.read.notJson",
            &[("path", &path.display().to_string())],
        ),
        raw,
    )
}

/// 带版本的那一份字节：整份须是一个对象，顶层 `version` 换成这一版。
fn versioned_body<T: serde::Serialize + ?Sized>(
    path: &Path,
    v: &T,
    version: u32,
) -> Result<Vec<u8>, Said> {
    let encode_failed = |raw: String| {
        Said::with_raw(
            copy_text(
                "beOwnState.write.encodeFailed",
                &[("path", &path.display().to_string())],
            ),
            raw,
        )
    };
    let mut value = serde_json::to_value(v).map_err(|e| encode_failed(e.to_string()))?;
    let Some(o) = value.as_object_mut() else {
        return Err(encode_failed(
            "a versioned document must be a JSON object".into(),
        ));
    };
    o.insert(VERSION_KEY.into(), serde_json::json!(version));
    json_body(path, &value)
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
    e.wrap(|why| {
        copy_text(
            "reason.fs.readFailed",
            &[("path", &path.display().to_string()), ("why", why)],
        )
    })
}

/// 读整份并按 JSON 解成 `T`。解不开 ⇒ `Unreadable`。
fn read_json<T: serde::de::DeserializeOwned>(path: &Path, cap: u64) -> Read<T> {
    read_bytes(path, cap).and_then(|b| match serde_json::from_slice::<T>(&b) {
        Ok(t) => Read::Present(t),
        Err(e) => Read::Unreadable(not_json(path, e)),
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
