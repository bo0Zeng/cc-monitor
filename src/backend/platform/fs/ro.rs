//! 读盘的唯一住址：[`Ro`]（只读的命名空间）· [`RoFile`]（只能读、只能挪读写位置的文件）。
//!
//! 后端碰盘的标准库与系统调用只在 `platform/fs/` 放行（`src/backend/clippy.toml` 碰盘那一组）；
//! 别处读盘一律经这里。`Ro` 没有写方法，[`Ro::open`] 交回的 [`RoFile`] 不实现 `Write` ⇒ 拿着它写不出来。
//! 缺一个读动词就加在这里（不跟链接地看一项 · inode 号这几样等写盘那几层搬过来时随用随加），名单不用动。
//! 本文件每一处放行都写在那一个函数上，理由一律是「只读」：放行处多一个写动词，读这一份的人看得见。
//! 平台无关的部分不认域知识（不认识账号、会话、帧）。

use copy_core::copy_text;
use copy_core::said::Said;
use relay_route_core::{Cap, Over, Regular};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

// 读出来的那两种值（元数据 · 目录遍历）只装读到的东西，没有一样能写回盘；调用方不用点名它们的类型。
/// 文件的元数据（读者按这个名字收，不点名 `std::fs`）。
pub type Metadata = std::fs::Metadata;
type ReadDir = std::fs::ReadDir;

/// 权限位那一格的值（从元数据读出来，或由 [`Ro::perms_from_mode`] 照一个 unix 模式造）；只是值，交给写面的改权限才落盘。
pub type Perms = std::fs::Permissions;

/// 盘上一份文件的戳：改动时刻 · 长度 · inode（[`Ro::stamp`]）。
pub type Stamp = (std::time::SystemTime, u64, u64);

/// 读盘的命名空间（零大小，不用递）。元数据 · 目录 · 打开与标准库同名同形，换法就是把 `std::fs::` 换成 `Ro::`；
/// 整份读只有带上限的那几样（[`Ro::read_capped`] 起，必带 [`Cap`]），没有不带上限的。
pub struct Ro;

impl Ro {
    #[expect(clippy::disallowed_methods, reason = "读盘的唯一住址：列目录，只读")]
    pub fn read_dir(p: impl AsRef<Path>) -> io::Result<ReadDir> {
        std::fs::read_dir(p)
    }

    #[expect(clippy::disallowed_methods, reason = "读盘的唯一住址：问元数据，只读")]
    pub fn metadata(p: impl AsRef<Path>) -> io::Result<Metadata> {
        std::fs::metadata(p)
    }

    /// 问元数据、不跟链接（最后一段是链接 ⇒ 答的是链接本身）。
    #[expect(clippy::disallowed_methods, reason = "读盘的唯一住址：问元数据，只读")]
    pub fn symlink_metadata(p: impl AsRef<Path>) -> io::Result<Metadata> {
        std::fs::symlink_metadata(p)
    }

    #[expect(clippy::disallowed_methods, reason = "读盘的唯一住址：解路径，只读")]
    pub fn canonicalize(p: impl AsRef<Path>) -> io::Result<PathBuf> {
        std::fs::canonicalize(p)
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "读盘的唯一住址：读链接内容，只读"
    )]
    pub fn read_link(p: impl AsRef<Path>) -> io::Result<PathBuf> {
        std::fs::read_link(p)
    }

    /// 只读打开（跟链接，同 `File::open`）。交回的只能读、挪读写位置、问元数据。
    #[expect(
        clippy::disallowed_methods,
        reason = "读盘的唯一住址：只读打开，交回的 `RoFile` 不实现 `Write`"
    )]
    pub fn open(p: impl AsRef<Path>) -> io::Result<RoFile> {
        std::fs::File::open(p).map(RoFile)
    }

    /// 只读打开、最后一段是链接就失败（`O_NOFOLLOW`；别的平台同 [`Ro::open`]）。
    pub fn open_nofollow(p: impl AsRef<Path>) -> io::Result<RoFile> {
        nofollow_read().open(p).map(RoFile)
    }

    /// 异步问元数据（跟链接）。
    #[expect(clippy::disallowed_methods, reason = "读盘的唯一住址：问元数据，只读")]
    pub async fn metadata_async(p: impl AsRef<Path>) -> io::Result<Metadata> {
        tokio::fs::metadata(p).await
    }

    /// 异步只读打开（跟链接）：交回的只能读、挪读写位置。
    #[expect(
        clippy::disallowed_methods,
        reason = "读盘的唯一住址：只读打开，交回的 `RoAsyncFile` 只实现读与挪位置"
    )]
    pub async fn open_async(p: impl AsRef<Path>) -> io::Result<RoAsyncFile> {
        tokio::fs::File::open(p).await.map(RoAsyncFile)
    }

    /// 异步只读打开、不跟链接。
    pub async fn open_nofollow_async(p: impl AsRef<Path>) -> io::Result<RoAsyncFile> {
        tokio::fs::OpenOptions::from(nofollow_read())
            .open(p)
            .await
            .map(RoAsyncFile)
    }

    /// 照一个 unix 模式（低 12 位）造权限位那一格的值。别的平台没有这一维 ⇒ `None`。
    #[cfg_attr(
        unix,
        expect(
            clippy::disallowed_methods,
            reason = "读盘的唯一住址：造一个权限位的值，不落盘"
        )
    )]
    pub fn perms_from_mode(mode: u32) -> Option<Perms> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            Some(Perms::from_mode(mode))
        }
        #[cfg(not(unix))]
        {
            let _ = mode;
            None
        }
    }

    /// 带上限地整份读（跟链接）：只认普通文件（挡掉 FIFO / 字符设备 / socket —— 它们的 `metadata().len()` 报 0 会骗过大小检查，
    /// symlink→/dev/zero 几秒涨十几 GB）；比 `cap.bytes` 多一个字节 ⇒ 回超出上限那一句，一个字节不交出去。
    /// 链接照 `metadata()` 解析到目标：目标是普通文件才读。
    ///
    /// 整份读越界时交得出的只有「不给」：截一段交回去没有一格能带「截了」，就成了静默截断。`cap.over` 那一档在越界之后由调用方照办
    /// （硬报错 · 拒收 · 降级 · 带身份报告 …）；[`Over::SkipAndSay`] 那一档在这里 `warn!` 带上是哪一份，调用方当没有即可。
    /// 要截断 · 分块 · 只截可搜性的读不走整份读。
    pub fn read_capped(p: impl AsRef<Path>, cap: Cap) -> Result<Vec<u8>, Said> {
        read_whole(p.as_ref(), cap, Link::Follow)
    }

    /// 带上限、不跟链接地整份读：最后一段是链接 ⇒ 不读（回「不是普通文件」）；别的同 [`Ro::read_capped`]。
    pub fn read_capped_nofollow(p: impl AsRef<Path>, cap: Cap) -> Result<Vec<u8>, Said> {
        read_whole(p.as_ref(), cap, Link::Refuse)
    }

    /// 带上限地读成字符串（同 [`Ro::read_capped`]）：不是 UTF-8 ⇒ 读不成，解码原话进 `raw`。
    pub fn read_string_capped(p: impl AsRef<Path>, cap: Cap) -> Result<String, Said> {
        String::from_utf8(Self::read_capped(p, cap)?)
            .map_err(|e| Said::with_raw(copy_core::io_reason(io::ErrorKind::InvalidData), e))
    }

    /// 带上限地读一份 JSON（同 [`Ro::read_capped`]，剥掉 UTF-8 BOM 再解）：读不了 ⇒ 那一句是原因；解不开 ⇒「内容无法解析」，解析器原话进 `raw`。
    pub fn read_json(p: impl AsRef<Path>, cap: Cap) -> Result<serde_json::Value, Said> {
        parse_json(&Self::read_capped(p, cap)?)
    }

    /// 文件 mtime 的毫秒时间戳；任何读取失败都退化成 `0`（排序时沉到最旧：读失败与「1970 年的文件」混成同一个值，要改得连同调用点的排序语义一起想）。
    #[expect(clippy::disallowed_methods, reason = "读盘的唯一住址：问元数据，只读")]
    pub fn mtime_ms(p: &Path) -> i64 {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    /// unix 权限位（`st_mode` 全字，含文件类型位；调用方按需 `& 0o777`）。别的平台没有这一维 ⇒ `None`。
    #[cfg_attr(
        unix,
        expect(
            clippy::disallowed_methods,
            reason = "读盘的唯一住址：从已读的元数据取权限位"
        )
    )]
    pub fn mode(m: &Metadata) -> Option<u32> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            Some(m.permissions().mode())
        }
        #[cfg(not(unix))]
        {
            let _ = m;
            None
        }
    }

    /// unix 的属主 uid。别的平台 ⇒ `None`。
    #[cfg_attr(
        unix,
        expect(
            clippy::disallowed_methods,
            reason = "读盘的唯一住址：从已读的元数据取属主"
        )
    )]
    pub fn uid(m: &Metadata) -> Option<u32> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            Some(m.uid())
        }
        #[cfg(not(unix))]
        {
            let _ = m;
            None
        }
    }

    /// 盘上那一份的戳（[`Stamp`]）：改动时刻 · 长度 · inode（别的平台记 0）。不在 / 读不出元数据 ⇒ `None`。
    /// 原子替换每次换一个新节点，同一纳秒里写两次也分得出 —— 缓存判「别人写过没有」用它。
    #[expect(clippy::disallowed_methods, reason = "读盘的唯一住址：问元数据，只读")]
    pub fn stamp(p: &Path) -> Option<Stamp> {
        let m = std::fs::metadata(p).ok()?;
        #[cfg(unix)]
        let node = std::os::unix::fs::MetadataExt::ino(&m);
        #[cfg(not(unix))]
        let node = 0;
        Some((m.modified().ok()?, m.len(), node))
    }

    /// unix 的硬链接数。别的平台 ⇒ `None`。
    #[cfg_attr(
        unix,
        expect(
            clippy::disallowed_methods,
            reason = "读盘的唯一住址：从已读的元数据取硬链接数"
        )
    )]
    pub fn nlink(m: &Metadata) -> Option<u64> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            Some(m.nlink())
        }
        #[cfg(not(unix))]
        {
            let _ = m;
            None
        }
    }

    /// unix 的设备号（同一个文件系统内才相等）。别的平台 ⇒ `None`。
    #[cfg_attr(
        unix,
        expect(
            clippy::disallowed_methods,
            reason = "读盘的唯一住址：从已读的元数据取设备号"
        )
    )]
    pub fn dev(m: &Metadata) -> Option<u64> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            Some(m.dev())
        }
        #[cfg(not(unix))]
        {
            let _ = m;
            None
        }
    }
}

/// 整份读遇到最后一段是链接时：跟到目标 · 不读。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Link {
    Follow,
    Refuse,
}

/// 带上限的整份读：先看类型 · 再看长度 · 再读，都在 `relay_route_core::read_capped_regular`（手机核心读本机文件同一份）；
/// 这里只定跟不跟链接（元数据怎么取 · 怎么打开）与那一句。读不了的那一句只是原因（原因词 · 不是普通文件 · 超出上限）；系统原话进 `raw`（复制详情）。
#[expect(
    clippy::disallowed_methods,
    reason = "读盘的唯一住址：问元数据 · 只读打开，读满上限即停"
)]
fn read_whole(path: &Path, limit: Cap, link: Link) -> Result<Vec<u8>, Said> {
    let io = |e: io::Error| Said::with_raw(copy_core::io_reason(e.kind()), e);
    let meta = match link {
        Link::Follow => std::fs::metadata(path),
        Link::Refuse => std::fs::symlink_metadata(path),
    }
    .map_err(io)?;
    let open = || match link {
        Link::Follow => std::fs::File::open(path),
        Link::Refuse => nofollow_read().open(path),
    };
    match relay_route_core::read_capped_regular(&meta, open, limit).map_err(io)? {
        Regular::Whole(buf) => Ok(buf),
        Regular::NotRegular => Err(copy_text("reason.fs.notRegular", &[]).into()),
        Regular::OverCap(size) => Err(over_cap(path, limit, size)),
    }
}

/// 超出上限那一句（量得出实际大小就带上）；[`Over::SkipAndSay`] 那一档在这里 `warn!` 是哪一份。
fn over_cap(path: &Path, cap: Cap, size: Option<u64>) -> Said {
    if cap.over == Over::SkipAndSay {
        tracing::warn!(path = %path.display(), cap = cap.bytes, ?size, "超出读上限，跳过");
    }
    let c = cap.bytes.to_string();
    match size {
        Some(n) => copy_text(
            "beFs.readRegularCapped.tooBigSized",
            &[("cap", &c), ("size", &n.to_string())],
        ),
        None => copy_text("beFs.readRegularCapped.tooBig", &[("cap", &c)]),
    }
    .into()
}

/// 剥掉 UTF-8 BOM 再解；解不开 ⇒「内容无法解析」，解析器原话进 `raw`。
fn parse_json(b: &[u8]) -> Result<serde_json::Value, Said> {
    serde_json::from_slice(b.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(b))
        .map_err(|e| Said::with_raw(copy_text("reason.content.unparsable", &[]), e))
}

/// 只读、不跟链接的开法（最后一段是链接 ⇒ 失败）。
#[expect(
    clippy::disallowed_methods,
    reason = "读盘的唯一住址：只读的开法（只 `read(true)` ＋ 不跟链接）"
)]
fn nofollow_read() -> std::fs::OpenOptions {
    let mut o = std::fs::OpenOptions::new();
    o.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        o.custom_flags(super::NO_FOLLOW);
    }
    o
}

/// 异步只读打开的文件：只实现 `AsyncRead` 与 `AsyncSeek`。
#[derive(Debug)]
pub struct RoAsyncFile(tokio::fs::File);

impl tokio::io::AsyncRead for RoAsyncFile {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().0).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncSeek for RoAsyncFile {
    fn start_seek(self: std::pin::Pin<&mut Self>, pos: SeekFrom) -> io::Result<()> {
        std::pin::Pin::new(&mut self.get_mut().0).start_seek(pos)
    }
    fn poll_complete(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<u64>> {
        std::pin::Pin::new(&mut self.get_mut().0).poll_complete(cx)
    }
}

/// 只读打开的文件：只实现 `Read` 与 `Seek`，不实现 `Write`（`readonly_guard` 有一条现打的自检）。
#[derive(Debug)]
pub struct RoFile(std::fs::File);

impl RoFile {
    pub fn metadata(&self) -> io::Result<Metadata> {
        self.0.metadata()
    }
}

impl Read for RoFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Seek for RoFile {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.0.seek(pos)
    }
}

/// 拿描述符的那一面只给 unix：目录锁要对它 `flock`（`platform::lock`）。描述符本身换不回一个能写的文件 ——
/// 它是只读打开的，包成别的文件照样写不进去。
#[cfg(unix)]
impl std::os::fd::AsFd for RoFile {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.0.as_fd()
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/platform/fs/ro_tests.rs"]
mod tests;
