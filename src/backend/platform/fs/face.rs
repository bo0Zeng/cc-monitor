//! 文件管理面的写：[`Face`] · [`Resolved`] · [`FaceFile`]；分叉那一处只会新建的 [`NewFile`]。
//!
//! - [`Resolved`] 是「已过路径解析」的那一格：根底下已打开的那一层目录（unix 是目录句柄）＋ 最后一段名字。只能由路径解析造
//!   （`common/root_path.rs::walk_in_root` 从根句柄逐段往下开；删会话那一道 [`Resolved::vouch`]，谁许调由 `readonly_guard` 的构造判据钉）。
//!   [`Face`] 的写方法只收它，不收 `&Path` ⇒ 没过路径解析的路径递不进来。
//! - unix 上每个写动词都是 `*at` 一族、相对那个已开的目录句柄：判过之后半路哪一层被换成链接，动的仍是判过的那一层。
//!   Windows 那一臂仍按路径字符串动手（这一形在 Windows 上没闭合、没在真 Windows 上跑过）。
//! - [`Face`] 只有分派那一层造（`wire/spec.rs::with_face`：按档装处理器的 `prepare`〔帧面 · CLI 面〕与资产域那扇门都经它；另有帧面给传输台的那一份），
//!   按命令表声明的那一族（typed 的 `Facing` 那一档，只有文件管理写的那一族用）递给处理函数；别的命令拿不到这个值，也就写不出来。
//! - 写方法是文件管理面的动词闭集：`O_EXCL` 新建 · 就地覆盖写 · 建目录 · 改名（含不覆盖改名两形）·
//!   删文件 · 删空目录 · 改权限 · 建链接 · 落盘目录；外加暂存区那一层（后端自家目录，`own_dir`）。打开文件一律不跟链接（`O_NOFOLLOW`），做进方法里。
//! - [`NewFile`] 只会 `O_EXCL` 新建一份此前不存在的文件、一次写满：分叉写分支记录用，拿不到改 · 删 · 覆盖的能力。
//!   造只许分叉那两条命令（`session-fork` · `session-open`）在命令表里造。

use std::ffi::OsStr;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use super::ro::Perms;

/// 已过路径解析的一格：显示与读用的路径（`Deref` 成 `&Path`）＋ unix 上已开的父目录句柄与最后一段名字（写只经它们）。
#[derive(Clone, Debug)]
pub struct Resolved {
    path: PathBuf,
    #[cfg(unix)]
    dir: std::sync::Arc<std::os::fd::OwnedFd>,
    #[cfg(unix)]
    name: std::ffi::OsString,
}

impl PartialEq for Resolved {
    fn eq(&self, other: &Resolved) -> bool {
        self.path == other.path
    }
}
impl Eq for Resolved {}

impl Resolved {
    /// 从已解到底的根 `real_root` 往下逐段开 `inner`（只许普通段，每一段都不跟链接、必须是目录），停在那一层，
    /// 最后一段 `name` 不开。只给路径解析调（`common/root_path.rs`）。
    pub(crate) fn walk(real_root: &Path, inner: &Path, name: &OsStr) -> io::Result<Resolved> {
        #[cfg(unix)]
        let dir = {
            let mut dir = at::open_dir(None, real_root.as_os_str())?;
            for c in inner.components() {
                match c {
                    std::path::Component::Normal(seg) => dir = at::open_dir(Some(&dir), seg)?,
                    _ => return Err(io::Error::from(io::ErrorKind::InvalidInput)),
                }
            }
            std::sync::Arc::new(dir)
        };
        Ok(Resolved {
            path: real_root.join(inner).join(name),
            #[cfg(unix)]
            dir,
            #[cfg(unix)]
            name: name.to_os_string(),
        })
    }

    /// 担保 `p` 是要动的那一格（按路径开它的上一层）。只许删会话那一道调（构造判据钉）。
    pub(crate) fn vouch(p: PathBuf) -> io::Result<Resolved> {
        let (Some(dir), Some(name)) = (p.parent(), p.file_name()) else {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        };
        Resolved::walk(dir, Path::new(""), name)
    }

    pub(crate) fn into_path_buf(self) -> PathBuf {
        self.path
    }

    /// 同一层目录里的另一个名字（暂存旁名 · 备份 · `.part` 那几样）：目录句柄不变 ⇒ 仍是已过路径解析的。
    /// `name` 必须恰好是一段普通名字（不含分隔符，不是 `.` / `..`），否则 `None`。
    pub(crate) fn sibling(&self, name: impl AsRef<OsStr>) -> Option<Resolved> {
        let name = name.as_ref();
        let mut parts = Path::new(name).components();
        match (parts.next(), parts.next()) {
            (Some(std::path::Component::Normal(n)), None) if n == name => Some(Resolved {
                path: self.path.with_file_name(name),
                #[cfg(unix)]
                dir: self.dir.clone(),
                #[cfg(unix)]
                name: name.to_os_string(),
            }),
            _ => None,
        }
    }
}

impl std::ops::Deref for Resolved {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for Resolved {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

/// 非 unix 臂按路径开文件的三种开法（都只写）：新建（已在即失败）· 截断已在的 · 开已在的（落盘用）。
#[cfg_attr(unix, allow(dead_code))]
enum Open {
    New,
    Truncate,
    Existing,
}

#[cfg_attr(unix, allow(dead_code))]
fn by_path(how: Open) -> std::fs::OpenOptions {
    let mut o = std::fs::OpenOptions::new();
    o.write(true);
    match how {
        Open::New => o.create_new(true),
        Open::Truncate => o.truncate(true),
        Open::Existing => &mut o,
    };
    o
}

/// 文件管理面的写句柄（零大小）。造只经 [`Face::issue`]；克隆只给已经拿着它的那一处（传输台把它带进起跑的那趟任务）。
#[derive(Clone, Debug)]
pub struct Face(());

impl Face {
    /// 分派那一层造（构造判据钉住只许那两份）。
    pub(crate) fn issue() -> Face {
        Face(())
    }

    /// `O_EXCL` 新建（不跟链接）：已在（含一条链接）⇒ `AlreadyExists`，一个字节不动。
    pub(crate) fn create_new(&self, at: &Resolved) -> io::Result<FaceFile> {
        #[cfg(unix)]
        {
            let flags = libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL;
            at::open(&at.dir, &at.name, flags).map(FaceFile)
        }
        #[cfg(not(unix))]
        {
            by_path(Open::New).open(&at.path).map(FaceFile)
        }
    }

    /// 就地覆盖写（截断，不跟链接）：只给换内容那一个原语（硬链接 · 属主 · Windows 不许换名上位时）。
    pub(crate) fn overwrite(&self, at: &Resolved) -> io::Result<FaceFile> {
        #[cfg(unix)]
        {
            at::open(&at.dir, &at.name, libc::O_WRONLY | libc::O_TRUNC).map(FaceFile)
        }
        #[cfg(not(unix))]
        {
            by_path(Open::Truncate).open(&at.path).map(FaceFile)
        }
    }

    /// 建一层目录（已在 ⇒ `AlreadyExists`）。
    pub(crate) fn make_dir(&self, at: &Resolved) -> io::Result<()> {
        #[cfg(unix)]
        {
            at::mkdir(&at.dir, &at.name, 0o777)
        }
        #[cfg(not(unix))]
        {
            std::fs::create_dir(&at.path)
        }
    }

    /// 上传暂存区那两层：后端自家目录（[`super::own::own_dir`]，同 `Own` 那一份规矩：建的那一下 0700；已在必须是本人的真目录，放宽了收回 0700）。
    pub(crate) fn own_dir(&self, at: &Resolved) -> Result<(), copy_core::said::Said> {
        super::own::own_dir(&at.path)
    }

    /// 改名（目标已在会被顶掉，同 `rename(2)`）。
    pub(crate) fn move_file(&self, from: &Resolved, to: &Resolved) -> io::Result<()> {
        #[cfg(unix)]
        {
            at::rename(
                &from.dir,
                &from.name,
                &to.dir,
                &to.name,
                at::Rename::Replace,
            )
        }
        #[cfg(windows)]
        {
            std::fs::rename(&from.path, &to.path).map_err(|e| held_as_busy(e, &to.path))
        }
        #[cfg(not(any(unix, windows)))]
        {
            std::fs::rename(&from.path, &to.path)
        }
    }

    /// 不覆盖改名：`to` 已在（含一条链接）⇒ `AlreadyExists`，一个字节不动；那块盘没有这件原语 ⇒
    /// [`super::noreplace_unsupported`] 认得出的错，调用方自己决定退回什么。
    pub(crate) fn move_noreplace(&self, from: &Resolved, to: &Resolved) -> io::Result<()> {
        #[cfg(unix)]
        {
            at::rename(
                &from.dir,
                &from.name,
                &to.dir,
                &to.name,
                at::Rename::NoReplace,
            )
        }
        #[cfg(not(unix))]
        {
            super::rename_noreplace(&from.path, &to.path)
        }
    }

    /// 盘不认不覆盖改名时普通文件那一条退路：`link` ＋ `unlink`（`link` 在 `to` 已在时原子失败，NFS 上也成立）；
    /// `unlink` 失败 ⇒ 撤掉刚建的那个名字、回 `unlink` 的错。目录没有这条路（调用方判）。非 unix ⇒ `Unsupported`。
    pub(crate) fn move_by_link(&self, from: &Resolved, to: &Resolved) -> io::Result<()> {
        #[cfg(unix)]
        {
            at::link(&from.dir, &from.name, &to.dir, &to.name)?;
            if let Err(e) = at::unlink(&from.dir, &from.name, 0) {
                at::unlink(&to.dir, &to.name, 0).ok();
                return Err(e);
            }
            Ok(())
        }
        #[cfg(not(unix))]
        {
            let _ = (from, to);
            Err(io::Error::from(io::ErrorKind::Unsupported))
        }
    }

    /// 删一份文件（含链接本身，不跟）。
    pub(crate) fn delete_file(&self, at: &Resolved) -> io::Result<()> {
        #[cfg(unix)]
        {
            at::unlink(&at.dir, &at.name, 0)
        }
        #[cfg(not(unix))]
        {
            std::fs::remove_file(&at.path)
        }
    }

    /// 删一个空目录。
    pub(crate) fn delete_dir(&self, at: &Resolved) -> io::Result<()> {
        #[cfg(unix)]
        {
            at::unlink(&at.dir, &at.name, libc::AT_REMOVEDIR)
        }
        #[cfg(not(unix))]
        {
            std::fs::remove_dir(&at.path)
        }
    }

    /// 改权限（不跟链接：调用方先把路径解到底，那一格此刻是链接 ⇒ 失败）。
    pub(crate) fn set_permissions(&self, at: &Resolved, perms: Perms) -> io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            at::chmod(&at.dir, &at.name, perms.mode())
        }
        #[cfg(not(unix))]
        {
            std::fs::set_permissions(&at.path, perms)
        }
    }

    /// 在 `at` 建一条链接，内容 `target` 原样写（不解、不判，同 `cp -P`）。`at` 已在 ⇒ 系统拒。
    #[cfg(unix)]
    pub(crate) fn make_link(&self, target: &Path, at: &Resolved) -> io::Result<()> {
        at::symlink(target.as_os_str(), &at.dir, &at.name)
    }

    /// 把 `at` 那一份文件落盘（不跟链接；暂存件改名上位之前）。
    pub(crate) fn sync(&self, at: &Resolved) -> io::Result<()> {
        #[cfg(unix)]
        {
            at::open(&at.dir, &at.name, libc::O_RDONLY)?.sync_all()
        }
        #[cfg(not(unix))]
        {
            by_path(Open::Existing).open(&at.path)?.sync_all()
        }
    }

    /// 把 `at` 所在那一层目录落盘（换名上位之后：目录项也要落到盘上）。非 unix 无此事 ⇒ `Ok`。
    pub(crate) fn sync_dir(&self, at: &Resolved) -> io::Result<()> {
        #[cfg(unix)]
        at::open(&at.dir, OsStr::new("."), libc::O_RDONLY | libc::O_DIRECTORY)?.sync_all()?;
        let _ = at;
        Ok(())
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

/// unix 的 `*at` 一族：每一下都相对一个已开的目录句柄、只认一段名字。
#[cfg(unix)]
mod at {
    use std::ffi::{CString, OsStr};
    use std::io;
    use std::os::fd::{AsRawFd as _, FromRawFd as _, OwnedFd};
    use std::os::unix::ffi::OsStrExt as _;

    fn c(name: &OsStr) -> io::Result<CString> {
        Ok(CString::new(name.as_bytes())?)
    }

    fn fd_of(dir: Option<&OwnedFd>) -> libc::c_int {
        dir.map_or(libc::AT_FDCWD, |d| d.as_raw_fd())
    }

    fn ok(rc: libc::c_int) -> io::Result<()> {
        if rc == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    /// 开一层目录（不跟链接；`dir` 缺 ⇒ 按路径开根，根本身可以是链接）。Linux 用 `O_PATH`：只要能进、不必能读。
    pub(super) fn open_dir(dir: Option<&OwnedFd>, name: &OsStr) -> io::Result<OwnedFd> {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        let how = libc::O_PATH;
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        let how = libc::O_RDONLY;
        let nofollow = if dir.is_some() { libc::O_NOFOLLOW } else { 0 };
        let n = c(name)?;
        // SAFETY：`n` 以 NUL 结尾、活过这一次调用；回的描述符归这里的 `OwnedFd`。
        let fd = unsafe {
            libc::openat(
                fd_of(dir),
                n.as_ptr(),
                how | libc::O_DIRECTORY | libc::O_CLOEXEC | nofollow,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY：刚开出来的、只归这里。
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    /// 开一份文件（一律不跟链接；建出来的按 0666 ＆ umask）。
    pub(super) fn open(
        dir: &OwnedFd,
        name: &OsStr,
        flags: libc::c_int,
    ) -> io::Result<std::fs::File> {
        let n = c(name)?;
        // SAFETY：同上。
        let fd = unsafe {
            libc::openat(
                dir.as_raw_fd(),
                n.as_ptr(),
                flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o666 as libc::c_uint,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY：同上。
        Ok(std::fs::File::from(unsafe { OwnedFd::from_raw_fd(fd) }))
    }

    pub(super) fn mkdir(dir: &OwnedFd, name: &OsStr, mode: u32) -> io::Result<()> {
        let n = c(name)?;
        // SAFETY：同上。
        ok(unsafe { libc::mkdirat(dir.as_raw_fd(), n.as_ptr(), mode as libc::mode_t) })
    }

    pub(super) fn unlink(dir: &OwnedFd, name: &OsStr, flags: libc::c_int) -> io::Result<()> {
        let n = c(name)?;
        // SAFETY：同上。
        ok(unsafe { libc::unlinkat(dir.as_raw_fd(), n.as_ptr(), flags) })
    }

    /// 改权限，不跟链接（那一格此刻是链接 ⇒ 失败）。
    pub(super) fn chmod(dir: &OwnedFd, name: &OsStr, mode: u32) -> io::Result<()> {
        let n = c(name)?;
        // SAFETY：同上。
        ok(unsafe {
            libc::fchmodat(
                dir.as_raw_fd(),
                n.as_ptr(),
                mode as libc::mode_t,
                libc::AT_SYMLINK_NOFOLLOW,
            )
        })
    }

    pub(super) fn symlink(target: &OsStr, dir: &OwnedFd, name: &OsStr) -> io::Result<()> {
        let (t, n) = (c(target)?, c(name)?);
        // SAFETY：同上。
        ok(unsafe { libc::symlinkat(t.as_ptr(), dir.as_raw_fd(), n.as_ptr()) })
    }

    pub(super) fn link(fd: &OwnedFd, from: &OsStr, td: &OwnedFd, to: &OsStr) -> io::Result<()> {
        let (f, t) = (c(from)?, c(to)?);
        // SAFETY：同上。
        ok(unsafe { libc::linkat(fd.as_raw_fd(), f.as_ptr(), td.as_raw_fd(), t.as_ptr(), 0) })
    }

    pub(super) enum Rename {
        Replace,
        NoReplace,
    }

    pub(super) fn rename(
        fd: &OwnedFd,
        from: &OsStr,
        td: &OwnedFd,
        to: &OsStr,
        how: Rename,
    ) -> io::Result<()> {
        let (f, t) = (c(from)?, c(to)?);
        let (fd, td) = (fd.as_raw_fd(), td.as_raw_fd());
        match how {
            // SAFETY：同上。
            Rename::Replace => ok(unsafe { libc::renameat(fd, f.as_ptr(), td, t.as_ptr()) }),
            Rename::NoReplace => noreplace(fd, &f, td, &t),
        }
    }

    #[cfg(target_os = "linux")]
    fn noreplace(fd: libc::c_int, f: &CString, td: libc::c_int, t: &CString) -> io::Result<()> {
        // SAFETY：同上；musl 没有包装函数 ⇒ 裸调用。
        let rc = unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                fd,
                f.as_ptr(),
                td,
                t.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        ok(if rc == 0 { 0 } else { -1 })
    }

    #[cfg(target_vendor = "apple")]
    fn noreplace(fd: libc::c_int, f: &CString, td: libc::c_int, t: &CString) -> io::Result<()> {
        // SAFETY：同上。
        ok(unsafe { libc::renameatx_np(fd, f.as_ptr(), td, t.as_ptr(), libc::RENAME_EXCL) })
    }

    #[cfg(not(any(target_os = "linux", target_vendor = "apple")))]
    fn noreplace(_: libc::c_int, _: &CString, _: libc::c_int, _: &CString) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
}

/// 改名顶掉 `to` 没成时（Windows）：照 [`super::replace_refusal`] 认出「被别的程序开着」⇒ 换成「被占用」那一类（原错包在里面，进复制详情）。
/// 拒绝访问时探一下：按「删」（改名顶掉要的就是这一项访问权，winnt.h 的 `DELETE` ＝ `0x0001_0000`）打开 `to`、打不开的错号交给判定；探只开不改。
#[cfg(windows)]
fn held_as_busy(e: io::Error, to: &Path) -> io::Error {
    use std::os::windows::fs::OpenOptionsExt as _;
    let Some(code) = e.raw_os_error() else {
        return e;
    };
    let probe = (code == super::WIN_ACCESS_DENIED)
        .then(|| {
            std::fs::OpenOptions::new()
                .access_mode(0x0001_0000)
                .open(to)
                .err()
                .and_then(|p| p.raw_os_error())
        })
        .flatten();
    match super::replace_refusal(code, probe) {
        Some(k) => io::Error::new(k, e),
        None => e,
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/platform/fs/face_tests.rs"]
mod tests;
