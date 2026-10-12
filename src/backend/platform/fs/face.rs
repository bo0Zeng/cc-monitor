//! 文件管理面的写：[`Face`] · [`Resolved`] · [`FaceFile`]；分叉那一处只会新建的 [`NewFile`]。
//!
//! - [`Resolved`] 是「已过路径解析」的那条路径：只能由 `control/files_write.rs` 的路径解析造（[`Resolved::vouch`]，
//!   谁许调由 `readonly_guard` 的构造判据钉）。[`Face`] 的写方法只收它，不收 `&Path` ⇒ 没过路径解析的路径递不进来。
//! - [`Face`] 只有分派那一层造（帧面 `stream/inbound/mod.rs` · CLI 面 `control/cli_control.rs`），
//!   按命令表声明的那一族（`Run::BlockingFace`，只有文件管理那一族用）递给处理函数；别的命令拿不到这个值，也就写不出来。
//! - 写方法是文件管理面的动词闭集：`O_EXCL` 新建 · 覆盖写（只给原地覆盖那一处）· 建目录 · 改名（含不覆盖改名两形）·
//!   删文件 · 删空目录 · 改权限 · 建链接；外加暂存区那一层带权限位的建目录。打开文件一律不跟链接（`O_NOFOLLOW`），做进方法里。
//! - [`NewFile`] 只会 `O_EXCL` 新建一份此前不存在的文件、一次写满：分叉写分支记录用，拿不到改 · 删 · 覆盖的能力。
//!   造只许分叉那两条命令（`session-fork` · `session-new`）在命令表里造。

use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use super::ro::Perms;

/// 已过路径解析的一条路径（词法不越根 ＋ 父目录解链接后仍在根里）。读的时候当 `&Path` 用（`Deref`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved(PathBuf);

impl Resolved {
    /// 担保 `p` 已过路径解析。只许 `control/files_write.rs` 的路径解析调（构造判据钉）。
    pub(crate) fn vouch(p: PathBuf) -> Resolved {
        Resolved(p)
    }

    pub(crate) fn as_path(&self) -> &Path {
        &self.0
    }

    pub(crate) fn into_path_buf(self) -> PathBuf {
        self.0
    }

    /// 同一个（已解开的）父目录里的另一个名字（暂存旁名 · 备份 · `.part` 那几样）：父目录不变 ⇒ 仍是已过路径解析的。
    /// `name` 必须恰好是一段普通名字（不含分隔符，不是 `.` / `..`），否则 `None`。
    pub(crate) fn sibling(&self, name: impl AsRef<std::ffi::OsStr>) -> Option<Resolved> {
        let name = name.as_ref();
        let mut parts = Path::new(name).components();
        match (parts.next(), parts.next()) {
            (Some(std::path::Component::Normal(n)), None) if n == name => {
                Some(Resolved(self.0.with_file_name(name)))
            }
            _ => None,
        }
    }
}

impl std::ops::Deref for Resolved {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for Resolved {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

/// 文件管理面的写句柄（零大小）。造只经 [`Face::issue`]；克隆只给已经拿着它的那一处（传输台把它带进起跑的那趟任务）。
#[derive(Clone, Debug)]
pub struct Face(());

/// 不跟链接的开法（最后一段是链接 ⇒ 失败）。
fn nofollow() -> std::fs::OpenOptions {
    let mut o = std::fs::OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        o.custom_flags(super::NO_FOLLOW);
    }
    o
}

impl Face {
    /// 分派那一层造（构造判据钉住只许那两份）。
    pub(crate) fn issue() -> Face {
        Face(())
    }

    /// `O_EXCL` 新建（不跟链接）：已在（含一条链接）⇒ `AlreadyExists`，一个字节不动。
    pub(crate) fn create_new(&self, at: &Resolved) -> io::Result<FaceFile> {
        nofollow()
            .write(true)
            .create_new(true)
            .open(at.as_path())
            .map(FaceFile)
    }

    /// 原地覆盖写（截断，不跟链接）：只给覆盖写那一处（链接数 · 属主不让换名上位时）。
    pub(crate) fn overwrite(&self, at: &Resolved) -> io::Result<FaceFile> {
        nofollow()
            .write(true)
            .truncate(true)
            .open(at.as_path())
            .map(FaceFile)
    }

    /// 整份写（跟着系统的默认开法，不原子）：只给 Windows 那一支（目标已在、换名上位顶不掉）。
    #[cfg(windows)]
    pub(crate) fn write_whole(&self, at: &Resolved, bytes: &[u8]) -> io::Result<()> {
        std::fs::write(at.as_path(), bytes)
    }

    /// 建一层目录（已在 ⇒ `AlreadyExists`）。
    pub(crate) fn make_dir(&self, at: &Resolved) -> io::Result<()> {
        std::fs::create_dir(at.as_path())
    }

    /// 建一层只给本人的目录（0700；已在不动）：上传暂存区那两层。
    pub(crate) fn make_dir_private(&self, at: &Resolved) -> io::Result<()> {
        super::own::create_dir_private(at.as_path())
    }

    /// 改名（目标已在会被顶掉，同 `rename(2)`）。
    pub(crate) fn move_file(&self, from: &Resolved, to: &Resolved) -> io::Result<()> {
        std::fs::rename(from.as_path(), to.as_path())
    }

    /// 不覆盖改名（[`super::rename_noreplace`]）。
    pub(crate) fn move_noreplace(&self, from: &Resolved, to: &Resolved) -> io::Result<()> {
        super::rename_noreplace(from.as_path(), to.as_path())
    }

    /// 盘不认不覆盖改名时普通文件那一条退路（[`super::rename_by_link`]）。
    pub(crate) fn move_by_link(&self, from: &Resolved, to: &Resolved) -> io::Result<()> {
        super::rename_by_link(from.as_path(), to.as_path())
    }

    /// 删一份文件（含链接本身，不跟）。
    pub(crate) fn delete_file(&self, at: &Resolved) -> io::Result<()> {
        std::fs::remove_file(at.as_path())
    }

    /// 删一个空目录。
    pub(crate) fn delete_dir(&self, at: &Resolved) -> io::Result<()> {
        std::fs::remove_dir(at.as_path())
    }

    /// 改权限（跟链接，同 `chmod(2)`；调用方先把路径解到底）。
    pub(crate) fn set_permissions(&self, at: &Resolved, perms: Perms) -> io::Result<()> {
        std::fs::set_permissions(at.as_path(), perms)
    }

    /// 在 `at` 建一条链接，内容 `target` 原样写（不解、不判，同 `cp -P`）。`at` 已在 ⇒ 系统拒。
    #[cfg(unix)]
    pub(crate) fn make_link(&self, target: &Path, at: &Resolved) -> io::Result<()> {
        std::os::unix::fs::symlink(target, at.as_path())
    }
}

/// 文件管理面开出来的一份文件（新建的或覆盖写的）：能读写、挪位置、改权限、落盘；
/// 下载落点那一份可以换成异步的（[`FaceFile::into_async`]）。
#[derive(Debug)]
pub struct FaceFile(std::fs::File);

impl FaceFile {
    pub(crate) fn set_permissions(&self, perms: Perms) -> io::Result<()> {
        self.0.set_permissions(perms)
    }

    pub(crate) fn sync_all(&self) -> io::Result<()> {
        self.0.sync_all()
    }

    /// 换成异步的那一份（只会写：下载落点往里写远端读来的字节）。
    pub(crate) fn into_async(self) -> FaceAsyncFile {
        FaceAsyncFile(tokio::fs::File::from_std(self.0))
    }
}

impl Read for FaceFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for FaceFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl Seek for FaceFile {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.0.seek(pos)
    }
}

/// [`FaceFile`] 的异步那一份：只实现 `AsyncWrite`。
#[derive(Debug)]
pub struct FaceAsyncFile(tokio::fs::File);

impl tokio::io::AsyncWrite for FaceAsyncFile {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<io::Result<usize>> {
        std::pin::Pin::new(&mut self.get_mut().0).poll_write(cx, buf)
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().0).poll_flush(cx)
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().0).poll_shutdown(cx)
    }
}

/// 分叉写分支记录那一只手：只会 `O_EXCL` 新建一份此前不存在的文件、一次写满。造只经 [`NewFile::issue`]。
#[derive(Debug)]
pub struct NewFile(());

impl NewFile {
    /// 分叉那两条命令在命令表里造（构造判据钉住只许那两处）。
    pub(crate) fn issue() -> NewFile {
        NewFile(())
    }

    /// `O_EXCL` 新建 `at` 并写满 `body`：已在 ⇒ `AlreadyExists`，不碰那一份。
    /// 写到一半失败 ⇒ 回那一下的错（半截文件留着，调用方报原因）。
    pub(crate) fn create(&self, at: &Path, body: &[u8]) -> Result<(), NewFileFail> {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(at)
            .map_err(NewFileFail::Create)?;
        f.write_all(body).map_err(NewFileFail::Write)
    }
}

/// [`NewFile::create`] 失败在哪一步。
#[derive(Debug)]
pub enum NewFileFail {
    Create(io::Error),
    Write(io::Error),
}

#[cfg(test)]
#[path = "../../../../tests/backend/platform/fs/face_tests.rs"]
mod tests;
