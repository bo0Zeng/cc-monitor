//! 后端**自己的**状态（数据目录 `~/.cc-monitor` 底下）的写：[`Own`] · [`OwnPath`] · [`OwnLock`]。
//!
//! - [`Own`] 在后端启动时造一次（[`Own::issue`]，根取 `platform::paths::data_home`），顺着入口递给写自家状态的那几处；
//!   别处拿不到这个值，也就写不出来。谁许造它由 `readonly_guard` 的构造判据钉（只许启动入口）。
//! - [`OwnPath`] 只能由 [`Own`] 从根往下拼（相对名、不许 `..`、不许绝对路径）⇒ 方法收的路径落不到根外。
//! - 方法只有这几件：建一层目录（0700，已在不动）· 拿目录的跨进程锁 · 原子写（要那个目录的锁）· 删文件 · 改名。
//!   原子写：旁名出生即只给本人（`O_EXCL` ＋ 0600）→ 写满 → `sync_all` → 原子挪过去；挪不过去就删掉自己建的旁名。
//!   旁名带 pid ＋ 线程 ＋ 进程内序号，同一进程几个线程同时写同一份也不撞。

use copy_core::copy_text;
use copy_core::said::Said;
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// 写自家状态的句柄。克隆便宜（只有两个根）；造只经 [`Own::issue`]。
///
/// 两个根，都是后端自己的家：
/// - 家目录底下的 `.cc-monitor`（[`Own::at_home`]，收 `relay_route_core` 那张表里「相对家目录」的名字，如 `.cc-monitor/chores.json`）；
/// - 数据目录（[`Own::root`] / [`Own::path`]，`CCM_DATA_DIR` 覆盖、否则同上那一个；历史注解 · 凭据文件 · 起会话账号记录住这里）。
#[derive(Clone, Debug)]
pub struct Own {
    /// 用户家目录；`None` = 这台没有家目录（`HOME` / `USERPROFILE` 都没有）。
    home: Option<PathBuf>,
    /// 数据目录；没有 ⇒ `None`，各写口照它们自己的「无主目录」那一句报。
    data: Option<PathBuf>,
}

/// 家目录底下后端那一层的名字（`at_home` 收的名字必须以它打头）。
const HOME_DIR_NAME: &str = ".cc-monitor";

/// 数据目录底下的一条路径。只能由 [`Own::root`] / [`Own::path`] / [`OwnPath::join`] 拼出来。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OwnPath(PathBuf);

/// 持着的那个目录的跨进程锁。原子写要它，而且要的是**那份文件所在目录**的这一把。
/// 两种拿法：等到拿到为止（[`Own::lock`]，`platform::lock`）· 拿不到就算（[`Own::try_lock`]，常驻后端抢门牌那一把，随进程活着）。
pub struct OwnLock {
    dir: PathBuf,
    _held: Held,
}

/// 锁本体（落地即放：只为它的 `Drop` 拿着，不读）。
enum Held {
    Waited {
        _lock: crate::platform::lock::DirLock,
    },
    Tried {
        _lock: own_chan::Held,
    },
}

/// 相对名：至少一段、每段都是普通名字（没有 `..` · `.` · 根 · 盘符）。
fn relative_ok(rel: &str) -> bool {
    let p = Path::new(rel);
    p.components().next().is_some() && p.components().all(|c| matches!(c, Component::Normal(_)))
}

impl OwnPath {
    pub(crate) fn as_path(&self) -> &Path {
        &self.0
    }

    /// 往下拼一段相对名；不是相对名（空 · 绝对 · 含 `..`）⇒ 拒。
    pub(crate) fn join(&self, rel: &str) -> Result<OwnPath, Said> {
        if !relative_ok(rel) {
            return Err(copy_text("beOwnState.path.notRelative", &[("rel", rel)]).into());
        }
        Ok(OwnPath(self.0.join(rel)))
    }
}

impl Own {
    /// 启动时造一次：两个根取这台的家目录与数据目录（照进程环境）。
    pub fn issue() -> Own {
        Own::issue_with(&|k| std::env::var(k).ok())
    }

    /// 同 [`Own::issue`]，环境由 `get` 答（启动入口的判据喂注入的环境；只许启动入口调，同一条构造判据管）。
    pub fn issue_with(get: &dyn Fn(&str) -> Option<String>) -> Own {
        let home = crate::platform::paths::home_dir_from(&|k| get(k).map(Into::into));
        Own {
            data: creds_core::store::monitor_data_dir(
                get(creds_core::store::DATA_DIR_ENV).as_deref(),
                home.clone(),
            ),
            home,
        }
    }

    /// 数据目录本身；这台没有家目录 ⇒ `None`。
    pub(crate) fn root(&self) -> Option<OwnPath> {
        self.data.clone().map(OwnPath)
    }

    /// 数据目录底下的 `rel`。没有家目录 ⇒ 拒（`beOwnState.path.noHome`）；`rel` 不是相对名 ⇒ 拒。
    pub(crate) fn path(&self, rel: &str) -> Result<OwnPath, Said> {
        self.root()
            .ok_or_else(|| Said::from(copy_text("beOwnState.path.noHome", &[])))?
            .join(rel)
    }

    /// 家目录底下的 `rel`（`relay_route_core` 那张表的写法）：必须以 `.cc-monitor` 那一段打头、往下全是普通名字。
    /// 家目录那一层本身不给（`.cc-monitor` 之外一个字节都写不到）。没有家目录 ⇒ 拒。
    pub(crate) fn at_home(&self, rel: &str) -> Result<OwnPath, Said> {
        let home = self
            .home
            .as_ref()
            .ok_or_else(|| Said::from(copy_text("beOwnState.path.noHome", &[])))?;
        let first = Path::new(rel).components().next();
        if !relative_ok(rel) || first != Some(Component::Normal(HOME_DIR_NAME.as_ref())) {
            return Err(copy_text("beOwnState.path.notRelative", &[("rel", rel)]).into());
        }
        Ok(OwnPath(home.join(rel)))
    }

    /// 家目录底下后端那一层本身（`~/.cc-monitor`）。
    pub(crate) fn home_dir(&self) -> Result<OwnPath, Said> {
        self.at_home(HOME_DIR_NAME)
    }

    /// 建**一层**目录，建的那一下就是 0700；已在的不动；父目录不在 ⇒ 照实报错。
    pub(crate) fn ensure_dir(&self, dir: &OwnPath) -> std::io::Result<()> {
        create_dir_private(dir.as_path())
    }

    /// 拿 `dir` 的跨进程锁（阻塞到拿到为止；`dir` 要先在）。
    pub(crate) fn lock(&self, dir: &OwnPath) -> Result<OwnLock, crate::platform::lock::LockFail> {
        Ok(OwnLock {
            dir: dir.0.clone(),
            _held: Held::Waited {
                _lock: hold(dir.as_path())?,
            },
        })
    }

    /// 试拿 `dir` 的独占锁、不等：别人持着 ⇒ `Ok(None)`。常驻后端抢门牌用（拿到的那一把随进程活着，「谁在听」也在它里面写）。
    /// 本平台没有这一形 ⇒ `Err`（`Unsupported`）。
    pub(crate) fn try_lock(&self, dir: &OwnPath) -> std::io::Result<Option<OwnLock>> {
        Ok(own_chan::try_hold(dir.as_path())?.map(|h| OwnLock {
            dir: dir.0.clone(),
            _held: Held::Tried { _lock: h },
        }))
    }

    /// 整份原子写 `body`。`held` 必须是 `path` 所在那个目录的锁，不是 ⇒ 拒、一个字节不落。
    pub(crate) fn write(&self, path: &OwnPath, held: &OwnLock, body: &[u8]) -> Result<(), Said> {
        if path.0.parent() != Some(held.dir.as_path()) {
            return Err(copy_text(
                "beOwnState.write.lockMismatch",
                &[("path", &path.0.display().to_string())],
            )
            .into());
        }
        atomic_write(&path.0, body)
    }

    /// 删一份文件（不删目录）。方法名刻意不叫 `remove_file` / `rename`：那两个方法形是远端写那一层动词网的锚点
    /// （`readonly_guard::remote_write_layer`），本机这一层同名会被认成第二份远端写。
    pub(crate) fn delete_file(&self, path: &OwnPath) -> std::io::Result<()> {
        std::fs::remove_file(&path.0)
    }
}

/// **订阅号凭据的窄句柄**：一个号的配置目录里那一份凭据文件 ＋ 那一家写它、续它时拿的几把 `mkdir` 锁目录
/// （续期：配置目录里一把 · 旁边一把；整份写：配置目录里一把；续某一条 MCP 登录：配置目录里按那一条起名的一把）。
/// 能写的只有这几处：整份原子写那份文件（要配置目录的跨进程锁）· 建 / 删那几把锁目录。配置目录本身与里面别的文件一个都碰不到。
/// 只能由 [`Own::account_creds`] 造（写自家状态的句柄才铸得出它）；名字由登记那一家的那一形给（一段名字，不许带分隔符）。
pub(crate) struct AccountCreds {
    dir: PathBuf,
    file: PathBuf,
    inside: PathBuf,
    beside: Option<PathBuf>,
    write: Option<PathBuf>,
}

/// 配置目录的跨进程锁（[`AccountCreds::lock`]）：写那份凭据要它。
pub(crate) struct CredsLock {
    _held: crate::platform::lock::DirLock,
}

/// 那几把锁目录里的哪一把。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RenewLock {
    /// 配置目录里那一把续期锁（`<目录>/<inside>`）。
    Inside,
    /// 配置目录旁边那一把续期锁（`<目录><beside>`）。
    Beside,
    /// 配置目录里那一把整份写锁（`<目录>/<write>`）。
    Write,
    /// 配置目录里续某一条登录时那一家拿的那一把（`<目录>/<名字>`；名字必须是一段名字）。
    OneLogin(String),
}

/// 一段名字：非空、只一段普通名字（没有分隔符 · `..` · `.`）。
fn one_name(n: &str) -> bool {
    let mut c = Path::new(n).components();
    matches!(c.next(), Some(Component::Normal(_))) && c.next().is_none() && !n.contains(['/', '\\'])
}

impl Own {
    /// 铸一个号的凭据窄句柄：`dir` 是那个号的配置目录，`file` / `inside` / `write` 是那一家登记的凭据文件名与里面那几把锁的名字
    /// （一段名字），`beside` 是旁边那把锁接在目录名后面的后缀（不许带分隔符）。名字不合 ⇒ 拒。
    pub(crate) fn account_creds(
        &self,
        dir: &Path,
        file: &str,
        inside: &str,
        beside: Option<&str>,
        write: Option<&str>,
    ) -> Result<AccountCreds, Said> {
        let bad = |n: &str| Said::from(copy_text("beOwnState.path.notRelative", &[("rel", n)]));
        for n in [file, inside].into_iter().chain(write) {
            if !one_name(n) {
                return Err(bad(n));
            }
        }
        if let Some(sfx) = beside {
            if sfx.is_empty() || sfx.contains(['/', '\\']) {
                return Err(bad(sfx));
            }
        }
        Ok(AccountCreds {
            dir: dir.to_path_buf(),
            file: dir.join(file),
            inside: dir.join(inside),
            beside: beside.map(|sfx| {
                let mut s = dir.as_os_str().to_os_string();
                s.push(sfx);
                PathBuf::from(s)
            }),
            write: write.map(|n| dir.join(n)),
        })
    }
}

impl AccountCreds {
    /// 那份凭据文件（读用）。
    pub(crate) fn file(&self) -> &Path {
        &self.file
    }

    /// 那一把锁目录在哪（读它的修改时刻用；没登记 · 名字不是一段名字 ⇒ `None`）。
    pub(crate) fn lock_path(&self, which: &RenewLock) -> Option<PathBuf> {
        match which {
            RenewLock::Inside => Some(self.inside.clone()),
            RenewLock::Beside => self.beside.clone(),
            RenewLock::Write => self.write.clone(),
            RenewLock::OneLogin(n) => one_name(n).then(|| self.dir.join(n)),
        }
    }

    /// 拿配置目录的跨进程锁（阻塞到拿到为止）。
    pub(crate) fn lock(&self) -> Result<CredsLock, crate::platform::lock::LockFail> {
        Ok(CredsLock {
            _held: hold(&self.dir)?,
        })
    }

    /// 建那一把锁目录：建成 ⇒ `true`（持有）；已在 ⇒ `false`；那一把没登记 ⇒ `false`；名字不是一段名字 ⇒ `Err`。
    pub(crate) fn make_lock(&self, which: &RenewLock) -> std::io::Result<bool> {
        if let RenewLock::OneLogin(n) = which {
            if !one_name(n) {
                return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
            }
        }
        let Some(p) = self.lock_path(which) else {
            return Ok(false);
        };
        match std::fs::create_dir(p) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// 删那一把锁目录（只删空目录）。
    pub(crate) fn drop_lock(&self, which: &RenewLock) -> std::io::Result<()> {
        match self.lock_path(which) {
            Some(p) => std::fs::remove_dir(p),
            None => Ok(()),
        }
    }

    /// 整份原子写那份凭据（出生即只给本人）。`held` 是配置目录那一把（[`AccountCreds::lock`]）。
    pub(crate) fn write(&self, _held: &CredsLock, body: &[u8]) -> Result<(), Said> {
        atomic_write(&self.file, body)
    }
}

/// **诊断文件的窄句柄**：宿主交的那一份（`CCM_BACKEND_STDERR_LOG`，或一次性模式落的家里那一份）＋ 旁边那份旧的。
/// 能做的只有三件：`O_EXCL` 新建当前那份 · 把当前那份原子挪成旧的（盖掉上一份旧的）· 只追加一整行（不在就新建、只给本人）。
/// 不建目录、不截断、别的文件一个都碰不到。造只经 [`DiagLog::issue`]（只许启动入口，同一条构造判据管）。
#[derive(Clone, Debug)]
pub struct DiagLog {
    cur: PathBuf,
    old: PathBuf,
}

/// 新建出来的当前那份：只会写、只交得出 fd（给 fd 2 指过去用）。
#[derive(Debug)]
pub(crate) struct DiagFile(std::fs::File);

impl DiagLog {
    /// 认下交来的那一份（旧的那份落在同目录：`stderr.log` ⇒ `stderr.old.log`）。
    pub fn issue(cur: PathBuf) -> DiagLog {
        let old = cur.with_extension("old.log");
        DiagLog { cur, old }
    }

    /// 当前那份。
    pub(crate) fn cur(&self) -> &Path {
        &self.cur
    }

    /// 旧的那份。
    pub(crate) fn old(&self) -> &Path {
        &self.old
    }

    /// 当前那份原子挪成旧的：挪了 ⇒ `true`；当前那份不在 ⇒ `false`。
    pub(crate) fn roll(&self) -> std::io::Result<bool> {
        match std::fs::rename(&self.cur, &self.old) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// `O_EXCL` 新建当前那份（已在 ⇒ 照实报错，不截断）。
    pub(crate) fn create(&self) -> std::io::Result<DiagFile> {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.cur)
            .map(DiagFile)
    }

    /// 只追加一整行（`O_APPEND`，不在就新建、只给本人；目录不在 ⇒ `Err`，不替它建）。
    pub(crate) fn append_line(&self, line: &[u8]) -> std::io::Result<()> {
        let mut o = std::fs::OpenOptions::new();
        o.append(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            o.mode(0o600);
        }
        o.open(&self.cur)?.write_all(line)
    }
}

/// 判据的「fd 2」替身要的两样（生产里 fd 2 那一份由 `platform::stderr_fd` 量、指，不经它们）。
#[cfg(test)]
impl DiagFile {
    /// 此刻多长（问不到 ⇒ `None`）。
    pub(crate) fn len(&self) -> Option<u64> {
        self.0.metadata().ok().map(|m| m.len())
    }

    /// 同一份再开一个把手（同一个打开文件描述）。
    pub(crate) fn try_clone(&self) -> std::io::Result<DiagFile> {
        self.0.try_clone().map(DiagFile)
    }
}

impl std::io::Write for DiagFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

#[cfg(unix)]
impl std::os::fd::AsFd for DiagFile {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.0.as_fd()
    }
}

/// 后端自家目录**建的那一下**给的权限位：只给本人（远端 SFTP 部署建目录用同一个数）。
pub(crate) const PRIVATE_DIR_MODE: u32 = 0o700;

/// 建**一层**目录、建的那一下就是 0700（unix：权限位随创建一起生效，没有「先按 umask 建出来再收窄」的那一段）；已在的不动。
/// 非 unix：照常建（目录的 ACL 按父目录继承）。[`Own::ensure_dir`] 的本体；文件管理面建暂存区那一处（`Face::make_dir_private`）也是它。
pub(crate) fn create_dir_private(dir: &Path) -> std::io::Result<()> {
    let mut b = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        b.mode(PRIVATE_DIR_MODE);
    }
    match b.create(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e),
    }
}

/// 拿一个目录的跨进程锁（阻塞到拿到为止）：本文件里两种句柄（[`Own::lock`] · [`AccountCreds::lock`]）都经这一处。
fn hold(dir: &Path) -> Result<crate::platform::lock::DirLock, crate::platform::lock::LockFail> {
    crate::platform::lock::hold(dir)
}

/// 进程内序号：同一线程连写两次也换一个旁名。
static SEQ: AtomicU64 = AtomicU64::new(0);

/// 原子写本体（全后端一份）。那一句只有路径与原因词，系统原话进 `raw`；都不带内容。
fn atomic_write(path: &Path, body: &[u8]) -> Result<(), Said> {
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
    let tmp_s = tmp.display().to_string();
    let path_s = path.display().to_string();
    let why = |e: &std::io::Error| copy_core::io_reason(e.kind());
    // 建不出来 ⇒ 那个名字上的文件不是我们的，什么都不删。
    let mut f = creds_core::perm::create_private(&tmp).map_err(|e| {
        let w = why(&e);
        Said::with_raw(
            copy_text(
                "beOwnState.write.tmpCreateFailed",
                &[("tmp", &tmp_s), ("why", &w)],
            ),
            &e,
        )
    })?;
    let result = f
        .write_all(body)
        .and_then(|()| f.sync_all())
        .map_err(|e| {
            let w = why(&e);
            Said::with_raw(
                copy_text(
                    "beOwnState.write.tmpWriteFailed",
                    &[("tmp", &tmp_s), ("why", &w)],
                ),
                &e,
            )
        })
        .and_then(|()| {
            drop(f);
            std::fs::rename(&tmp, path).map_err(|e| {
                let w = why(&e);
                Said::with_raw(
                    copy_text(
                        "beOwnState.write.renameFailed",
                        &[("tmp", &tmp_s), ("path", &path_s), ("why", &w)],
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
#[path = "../../../../tests/backend/platform/fs/own_tests.rs"]
mod tests;
