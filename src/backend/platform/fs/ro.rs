//! 读盘的唯一住址：[`Ro`]（只读的命名空间）· [`RoFile`]（只能读、只能挪读写位置的文件）。
//!
//! 后端生产段只有 `platform/fs/` 许点名文件系统命名空间（`readonly_guard::only_the_fs_platform_module_names_the_filesystem`）；
//! 别处读盘一律经这里。`Ro` 没有写方法，[`Ro::open`] 交回的 [`RoFile`] 不实现 `Write` ⇒ 拿着它写不出来。
//! 缺一个读动词就加在这里（不跟链接地看一项 · inode 号这几样等写盘那几层搬过来时随用随加），护栏不用动。本文件点名的标准库条目由同一份护栏逐个核是读（`the_fs_home_names_only_its_registered_entries`）。
//! 平台无关的部分不认域知识（不认识账号、会话、帧）。

use copy_core::copy_text;
use copy_core::said::Said;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

// 读出来的那两种值（元数据 · 目录遍历）只装读到的东西，没有一样能写回盘；调用方不用点名它们的类型。
type Metadata = std::fs::Metadata;
type ReadDir = std::fs::ReadDir;

/// 读盘的命名空间（零大小，不用递）。方法与标准库同名同形，换法就是把 `std::fs::` 换成 `Ro::`。
pub(crate) struct Ro;

impl Ro {
    pub(crate) fn read(p: impl AsRef<Path>) -> io::Result<Vec<u8>> {
        std::fs::read(p)
    }

    pub(crate) fn read_to_string(p: impl AsRef<Path>) -> io::Result<String> {
        std::fs::read_to_string(p)
    }

    pub(crate) fn read_dir(p: impl AsRef<Path>) -> io::Result<ReadDir> {
        std::fs::read_dir(p)
    }

    pub(crate) fn metadata(p: impl AsRef<Path>) -> io::Result<Metadata> {
        std::fs::metadata(p)
    }

    pub(crate) fn canonicalize(p: impl AsRef<Path>) -> io::Result<PathBuf> {
        std::fs::canonicalize(p)
    }

    pub(crate) fn read_link(p: impl AsRef<Path>) -> io::Result<PathBuf> {
        std::fs::read_link(p)
    }

    /// 只读打开（跟链接，同 `File::open`）。交回的只能读、挪读写位置、问元数据。
    pub(crate) fn open(p: impl AsRef<Path>) -> io::Result<RoFile> {
        std::fs::File::open(p).map(RoFile)
    }

    /// 安全读取：先确认是常规文件（挡掉 FIFO / 字符设备 / socket —— 它们的 `metadata().len()` 报 0 会骗过大小检查，而无上限的读能把内存吃爆，
    /// symlink→/dev/zero 几秒涨十几 GB），再 `take(cap)` 限量读。symlink 会被 `metadata()`（跟随）解析到目标类型：目标是常规文件才放行、是设备就拒。
    pub(crate) fn read_regular_capped(path: &Path, cap: u64) -> Result<Vec<u8>, Said> {
        // 读不了的那一句只是原因（原因词 · 不是普通文件 · 超出上限）；系统原话进 `raw`（复制详情）。
        let io = |e: std::io::Error| Said::with_raw(copy_core::io_reason(e.kind()), e);
        let meta = std::fs::metadata(path).map_err(io)?;
        if !meta.is_file() {
            return Err(copy_text("beFs.readRegularCapped.notRegular", &[]).into());
        }
        // 先看长度再决定读不读：拒绝一个巨型文件之前不先读 `cap + 1` 进内存。
        // 早退不取代下面那道 `take(cap + 1)`：`metadata` 与 `read_to_end` 之间文件还会长（TOCTOU），两道一起才完整。
        if meta.len() > cap {
            return Err(copy_text(
                "beFs.readRegularCapped.tooBigSized",
                &[
                    ("cap", &cap.to_string()),
                    ("size", &(meta.len()).to_string()),
                ],
            )
            .into());
        }
        let f = std::fs::File::open(path).map_err(io)?;
        let mut buf = Vec::new();
        // take(cap+1)：读到 cap+1 就知道超限了，不必读满整个（可能无界的）文件
        f.take(cap + 1).read_to_end(&mut buf).map_err(io)?;
        if buf.len() as u64 > cap {
            return Err(copy_text(
                "beFs.readRegularCapped.tooBig",
                &[("cap", &cap.to_string())],
            )
            .into());
        }
        Ok(buf)
    }

    /// 安全读一份 JSON（同 [`Ro::read_regular_capped`]，剥掉 UTF-8 BOM 再解）：读不了 ⇒ 那一句是原因；解不开 ⇒「内容无法解析」，解析器原话进 `raw`。
    pub(crate) fn read_json_capped(path: &Path, cap: u64) -> Result<serde_json::Value, Said> {
        let b = Self::read_regular_capped(path, cap)?;
        serde_json::from_slice(b.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(&b))
            .map_err(|e| Said::with_raw(copy_text("reason.content.unparsable", &[]), e))
    }

    /// 文件 mtime 的毫秒时间戳；任何读取失败都退化成 `0`（排序时沉到最旧：读失败与「1970 年的文件」混成同一个值，要改得连同调用点的排序语义一起想）。
    pub(crate) fn mtime_ms(p: &Path) -> i64 {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    /// unix 权限位（`st_mode` 全字，含文件类型位；调用方按需 `& 0o777`）。别的平台没有这一维 ⇒ `None`。
    pub(crate) fn mode(m: &Metadata) -> Option<u32> {
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
    pub(crate) fn uid(m: &Metadata) -> Option<u32> {
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

    /// unix 的设备号（同一个文件系统内才相等）。别的平台 ⇒ `None`。
    pub(crate) fn dev(m: &Metadata) -> Option<u64> {
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

/// 只读打开的文件：只实现 `Read` 与 `Seek`，不实现 `Write`（`readonly_guard` 有一条现打的自检）。
#[derive(Debug)]
pub(crate) struct RoFile(std::fs::File);

impl RoFile {
    pub(crate) fn metadata(&self) -> io::Result<Metadata> {
        self.0.metadata()
    }
}

impl Read for RoFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        self.0.read_to_end(buf)
    }
    fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize> {
        self.0.read_to_string(buf)
    }
}

impl Seek for RoFile {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.0.seek(pos)
    }
}

/// 拿描述符的那一面只给 unix：目录锁要对它 `flock`（`platform::lock`）。描述符本身换不回一个能写的文件 ——
/// 那一步要点名 `std::fs`，只有本目录点得了。
#[cfg(unix)]
impl std::os::fd::AsFd for RoFile {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.0.as_fd()
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/platform/fs/ro_tests.rs"]
mod tests;
