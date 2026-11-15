//! 那个号的凭据文件：读 · 续期时的锁 · 整份原子写回；另一手是那个号的 MCP 登录文件（见下面那一段）。只碰这两份文件与那几把锁。
//!
//! **写这份文件只有一处**（[`rewrite`]）：在那一家整份写时拿的那把锁里（`LoginFace::write_lock`）重读盘 → 调用方看此刻的样子
//! 决定写什么（写前比对就在这一步）→ 整份原子写。续期写回经它；那一家自己存盘也拿同一把、也在锁里重读，
//! 所以两边谁都不会拿一份旧的整份盖掉对方刚写进去的那一格。写锁有人持着 ⇒ 当场答 `Busy`，不等。
//!
//! 锁与那个号自己的 claude 用同一套（这样两边不会同时拿同一个刷新令牌去续）：
//! 配置目录里那一把（`<目录>/<inside>`）＋ 配置目录旁边那一把（`<目录><beside>`），都是 `mkdir` 成功即持有、`rmdir` 即放；
//! 放在守卫的 `Drop` 里（`work` panic 或提前返回也放）。已在的锁目录修改时刻早过那一家的门限（`LoginFace::lock_stale_ms`）⇒
//! 持有方已经不在（被杀 / 崩了），删了重拿，收回了哪几把交给 `work` 记进续期那行日志。
//! 拿不到 ⇒ 当场说「正在被别的进程续」，不等（后端零定时器）。另在跨进程锁里做（同一台的几个后端进程之间）：
//! 我方几个后端之间不会两个同时判同一把过期；与那一家自己的进程之间，「判过期 → 删 → 重拿」不是原子的，与 proper-lockfile 自己的做法同一个窗口。

use crate::agents::LoginFace;
use crate::common::said::Said;
use crate::platform::fs::{AccountCreds, CredsLock, Own, RenewLock, Ro};
use copy_core::copy_text;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 读一次凭据文件。三态：不在 / 是个 JSON 对象 / 读不出来或不是 JSON 对象（只给一句原因，不带内容）。
pub(crate) type Read = crate::common::own_state::Read<Map<String, Value>>;

/// 一个号的凭据文件（住那个号的配置目录，那一家登记的文件名）：状态（读不出来就不续、不覆盖；那一家的格式，读上限照表）。
const DOC: crate::common::own_state::Doc<Map<String, Value>> =
    crate::common::own_state::Doc::of(&relay_route_core::ACCOUNT_CREDS_FILE);

pub(crate) fn creds_path(dir: &Path, face: &LoginFace) -> PathBuf {
    dir.join(face.creds_file)
}

pub(crate) fn read(dir: &Path, face: &LoginFace) -> Read {
    read_at(&creds_path(dir, face))
}

fn read_at(path: &Path) -> Read {
    DOC.read_with(path, |b| match serde_json::from_slice::<Value>(&b) {
        Ok(Value::Object(m)) => Ok(m),
        Ok(_) => Err(copy_text("beOauthStore.read.notJson", &[]).into()),
        // 解析错误的文本可能带着原文片段 ⇒ 只说「不是 JSON」。
        Err(_) => Err(copy_text("beOauthStore.read.notJson", &[]).into()),
    })
}

/// 持着续期锁时调 `work`；锁有人持着 ⇒ `Err(Busy)`，一把都不留。
pub(crate) enum Locked<T> {
    Held(T),
    Busy,
}

/// 拿锁时当无主收回的那几把（「里面那把」/「旁边那把」），交给 `work`。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Reclaimed(pub(crate) Vec<&'static str>);

/// 这个号的凭据窄句柄（从写句柄铸；能写的只有那份凭据文件与那两把续期锁目录）。
pub(crate) fn creds_of(own: &Own, dir: &Path, face: &LoginFace) -> Result<AccountCreds, Said> {
    use crate::agents::WriteLock;
    use crate::platform::fs::WriteLockForm;
    own.account_creds(
        dir,
        face.creds_file,
        face.lock_inside,
        face.lock_beside,
        face.write_lock.map(|w| match w {
            WriteLock::Mkdir(n) => (n, WriteLockForm::Dir),
            WriteLock::File(n) => (n, WriteLockForm::File),
        }),
    )
}

/// 在续期锁里做一件事：跨进程锁（配置目录）→ 里面那一把 → 旁边那一把（各自登记了才拿）→ `work`（收那把跨进程锁，写回要它）→ 守卫倒序放掉。
pub(crate) fn with_refresh_lock<T>(
    creds: &AccountCreds,
    face: &LoginFace,
    work: impl FnOnce(&Reclaimed, &CredsLock) -> T,
) -> Result<Locked<T>, Said> {
    let held = creds.lock()?;
    let stale = face.lock_stale_ms;
    let mut reclaimed = Reclaimed::default();
    let mut holds = Holds(Vec::new());
    for (which, name) in [
        (RenewLock::Inside, "里面那把"),
        (RenewLock::Beside, "旁边那把"),
    ] {
        // 那一家没登记这一把（续期不拿锁的家）⇒ 不拿、不建。
        if creds.lock_path(&which).is_none() {
            continue;
        }
        match take(creds, which, stale, Some((name, &mut reclaimed)))? {
            Some(g) => holds.0.push(g),
            None => return Ok(Locked::Busy),
        }
    }
    Ok(Locked::Held(work(&reclaimed, &held)))
}

/// 持着的那几把续期锁（后拿的先放：落地时倒序一把把放，正常返回 · 提前返回 · panic 都走这里）。
struct Holds<'a>(Vec<Hold<'a>>);

impl Drop for Holds<'_> {
    fn drop(&mut self) {
        while self.0.pop().is_some() {}
    }
}

// ───────────────────────── MCP 登录那份文件（`AccountsFace::mcp_logins`） ─────────────────────────
//
// 与订阅号凭据分开的一手：文件名 · 格式 · 锁都照那一家的 `McpLogins` 声明（可以与凭据同一份文件，也可以另一份）。
// **写这份文件只有一处**（[`rewrite_login`]）：配置目录的跨进程锁 → 声明的那几把锁依次拿 → 重读盘 → 调用方看此刻的原文
// 决定写什么（写前比对就在这一步）→ 整份原子写 → 倒序放锁。任一把有人持着 ⇒ `Busy`，一个字节不写。

/// 一个号那份 MCP 登录文件：原文 ＋ 读出的各条（键 → 一条）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LoginsText {
    pub(crate) text: String,
    pub(crate) entries: Map<String, Value>,
}

/// MCP 登录文件（读上限同凭据文件那一类：同一类登录小文件）。
const LOGINS_DOC: crate::common::own_state::Doc<LoginsText> =
    crate::common::own_state::Doc::of(&relay_route_core::ACCOUNT_CREDS_FILE);

/// 读一个号那份 MCP 登录文件。三态：不在 / 读得懂 / 读不出来或那一家读不懂（只给一句原因，不带内容）。
pub(crate) fn read_logins(
    dir: &Path,
    logins: &crate::agents::McpLogins,
) -> crate::common::own_state::Read<LoginsText> {
    read_logins_at(&dir.join(logins.file), logins)
}

fn read_logins_at(
    path: &Path,
    logins: &crate::agents::McpLogins,
) -> crate::common::own_state::Read<LoginsText> {
    LOGINS_DOC.read_with(path, |b| {
        let text = String::from_utf8(b)
            .map_err(|_| Said::from(copy_text("reason.content.unparsable", &[])))?;
        let entries = (logins.read)(&text).map_err(Said::from)?;
        Ok(LoginsText { text, entries })
    })
}

/// 持着的一把 MCP 登录锁（铸时登记的第几把；落地即放：`mkdir` 那一形删锁目录，文件锁那一形关描述符）。
enum LoginHold<'a> {
    Dir(&'a crate::platform::fs::McpLoginsFile, usize),
    File(#[expect(dead_code, reason = "只为持着：落地即放")] crate::platform::fs::FileLockHeld),
}

impl Drop for LoginHold<'_> {
    /// 放不掉要留一行日志：那把锁会一直挡着那个号自己写这份文件，直到它过期被收回。
    fn drop(&mut self) {
        if let LoginHold::Dir(f, i) = self {
            if let Err(e) = f.drop_dir_lock(*i) {
                tracing::warn!("[oauth] {}: {e}", login_lock_shown(f, *i));
            }
        }
    }
}

/// 持着的那一串（后拿的先放：落地时倒序一把把放，正常返回 · 提前返回 · panic 都走这里）。
struct LoginHolds<'a>(Vec<LoginHold<'a>>);

impl Drop for LoginHolds<'_> {
    fn drop(&mut self) {
        while self.0.pop().is_some() {}
    }
}

fn login_lock_shown(f: &crate::platform::fs::McpLoginsFile, i: usize) -> String {
    f.lock_at(i)
        .map(|(p, _)| p.display().to_string())
        .unwrap_or_default()
}

/// 拿一把 MCP 登录锁时出了错（不是「别人持着」）。
fn login_lock_failed(f: &crate::platform::fs::McpLoginsFile, i: usize, e: &std::io::Error) -> Said {
    lock_failed_at(&login_lock_shown(f, i), e)
}

/// 写一条 MCP 登录（`key`）：在那一家声明的锁里（[`crate::agents::McpLogins::lock`]，依次拿）重读盘 → `edit` 看此刻的样子，
/// 回 `(Some(新原文), r)` ⇒ 整份原子写；`(None, r)` ⇒ 不写 → 倒序放锁。任一把有人持着 ⇒ `Busy`（一个字节不写）：
/// 那一家写完会再来一个文件事件，到时再同步。
pub(crate) fn rewrite_login<T>(
    own: &Own,
    dir: &Path,
    logins: &crate::agents::McpLogins,
    key: &str,
    edit: impl FnOnce(crate::common::own_state::Read<LoginsText>) -> (Option<String>, T),
) -> Result<Locked<T>, Said> {
    use crate::agents::McpLoginLock;
    use crate::platform::fs::LoginLockForm;
    // 这一下要拿的那几把（名字照声明算好、铸进句柄；句柄只碰这几处）· 各自的过期门限（文件锁那一形不看）。
    let plan: Vec<(String, LoginLockForm, u64)> = logins
        .lock
        .iter()
        .map(|l| match *l {
            McpLoginLock::WholeMkdir { name, stale_ms } => {
                (name.to_string(), LoginLockForm::Dir, stale_ms)
            }
            McpLoginLock::EntryMkdir { name, stale_ms } => {
                (name(key), LoginLockForm::Dir, stale_ms)
            }
            McpLoginLock::EntryFile { name } => (name(key), LoginLockForm::File, 0),
            McpLoginLock::WholeFile { name } => (name.to_string(), LoginLockForm::File, 0),
        })
        .collect();
    let names: Vec<(String, LoginLockForm)> =
        plan.iter().map(|(n, f, _)| (n.clone(), *f)).collect();
    let f = own.mcp_logins_file(dir, logins.file, &names)?;
    let held = f.lock()?;
    let mut holds = LoginHolds(Vec::new());
    for (i, (_, form, stale_ms)) in plan.iter().enumerate() {
        let got = match form {
            LoginLockForm::Dir => take_mkdir(
                f.lock_at(i).map(|(p, _)| p.to_path_buf()),
                *stale_ms,
                || f.make_dir_lock(i).map_err(|e| login_lock_failed(&f, i, &e)),
                || f.drop_dir_lock(i),
            )?
            .map(|_| LoginHold::Dir(&f, i)),
            LoginLockForm::File => f
                .try_file_lock(i)
                .map_err(|e| login_lock_failed(&f, i, &e))?
                .map(LoginHold::File),
        };
        match got {
            Some(h) => holds.0.push(h),
            None => return Ok(Locked::Busy),
        }
    }
    let (text, r) = edit(read_logins_at(f.file(), logins));
    if let Some(t) = text {
        f.write(&held, t.as_bytes())?;
    }
    Ok(Locked::Held(r))
}

/// 在那一家的整份写锁里改这份凭据文件（见模块头注）：拿写锁（登记了的话）→ 重读盘 → `edit` 看此刻的样子，
/// 回 `(Some(新的整份), r)` ⇒ 整份原子写；`(None, r)` ⇒ 不写 → 放锁。写锁有人持着 ⇒ `Busy`（一个字节不写）。
/// `held` 是配置目录那把跨进程锁（续期锁里拿到的那一把）。
pub(crate) fn rewrite<T>(
    creds: &AccountCreds,
    face: &LoginFace,
    held: &CredsLock,
    edit: impl FnOnce(Read) -> (Option<Map<String, Value>>, T),
) -> Result<Locked<T>, Said> {
    let _write = match creds.write_lock_form() {
        None => None,
        Some(crate::platform::fs::WriteLockForm::Dir) => {
            match take(creds, RenewLock::Write, face.write_lock_stale_ms, None)? {
                Some(g) => Some(WriteHold::Dir(g)),
                None => return Ok(Locked::Busy),
            }
        }
        Some(crate::platform::fs::WriteLockForm::File) => match creds.try_file_lock() {
            Ok(Some(g)) => Some(WriteHold::File(g)),
            Ok(None) => return Ok(Locked::Busy),
            Err(e) => return Err(lock_failed(creds, &RenewLock::Write, &e)),
        },
    };
    let (doc, r) = edit(read_at(creds.file()));
    if let Some(doc) = doc {
        write_doc(creds, held, &doc)?;
    }
    Ok(Locked::Held(r))
}

/// 持着的那把整份写锁（两形各自落地即放）。
enum WriteHold<'a> {
    Dir(#[expect(dead_code, reason = "只为持着：落地即放")] Hold<'a>),
    File(#[expect(dead_code, reason = "只为持着：落地即放")] crate::platform::fs::FileLockHeld),
}

/// 持着的一把 `mkdir` 锁：守卫离开作用域就放（正常返回 · 提前返回 · panic 都走这里）。
struct Hold<'a>(&'a AccountCreds, RenewLock);

impl Drop for Hold<'_> {
    /// 放不掉要留一行日志：那把锁会一直挡着那个号自己的 claude 续期 / 存盘，直到它过期被收回。
    fn drop(&mut self) {
        if let Err(e) = self.0.drop_lock(&self.1) {
            let shown = self
                .0
                .lock_path(&self.1)
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            tracing::warn!("[oauth] {shown}: {e}");
        }
    }
}

/// 拿一把 `mkdir` 锁：建成 ⇒ 持有；已在而修改时刻早过 `stale_ms` 毫秒 ⇒ 删了重拿（给了 `reclaimed` 就把那个名字记进去）；
/// 否则别人持着 ⇒ `None`。
fn take<'a>(
    creds: &'a AccountCreds,
    which: RenewLock,
    stale_ms: u64,
    reclaimed: Option<(&'static str, &mut Reclaimed)>,
) -> Result<Option<Hold<'a>>, Said> {
    let got = take_mkdir(
        creds.lock_path(&which),
        stale_ms,
        || {
            creds
                .make_lock(&which)
                .map_err(|e| lock_failed(creds, &which, &e))
        },
        || creds.drop_lock(&which),
    )?;
    match got {
        None => Ok(None),
        Some(was_stale) => {
            if let (true, Some((name, r))) = (was_stale, reclaimed) {
                r.0.push(name);
            }
            Ok(Some(Hold(creds, which)))
        }
    }
}

/// 拿一把 `mkdir` 锁的本体（凭据那几把 · MCP 登录那几把都经这一处）：建成 ⇒ `Some(false)`；
/// 已在而修改时刻（`path`）早过 `stale_ms` 毫秒 ⇒ 当无主收回（删了重建）⇒ `Some(true)`；否则别人持着 ⇒ `None`。
fn take_mkdir(
    path: Option<PathBuf>,
    stale_ms: u64,
    make: impl Fn() -> Result<bool, Said>,
    drop: impl FnOnce() -> std::io::Result<()>,
) -> Result<Option<bool>, Said> {
    #[expect(
        clippy::disallowed_methods,
        reason = "mkdir 锁的过期门限（续期锁 `LoginFace::lock_stale_ms` · 整份写锁 `write_lock_stale_ms` · MCP 登录那几把 `McpLoginLock::*Mkdir.stale_ms`）"
    )]
    let stale = Duration::from_millis(stale_ms);
    if make()? {
        return Ok(Some(false));
    }
    let age = path
        .and_then(|p| Ro::metadata(&p).and_then(|m| m.modified()).ok())
        .and_then(|t| crate::common::time::now().duration_since(t).ok());
    if !age.is_some_and(|a| a > stale) || drop().is_err() || !make()? {
        return Ok(None);
    }
    Ok(Some(true))
}

/// 拿那一把锁时出了错（不是「别人持着」）：那一句带锁的落点与原因词，原话跟着。
fn lock_failed(creds: &AccountCreds, which: &RenewLock, e: &std::io::Error) -> Said {
    lock_failed_at(
        &creds
            .lock_path(which)
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        e,
    )
}

/// 拿一把锁出了错的那一句（落点 ＋ 原因词，原话跟着）。
fn lock_failed_at(shown: &str, e: &std::io::Error) -> Said {
    Said::with_raw(
        copy_text(
            "beOauthStore.lock.failed",
            &[("path", shown), ("why", &copy_core::io_reason(e.kind()))],
        ),
        e,
    )
}

/// 整份原子写（只经 [`rewrite`]：`held` 是那把跨进程锁；出生即只给本人 → 写满 → 原子挪过去）。
fn write_doc(creds: &AccountCreds, held: &CredsLock, doc: &Map<String, Value>) -> Result<(), Said> {
    let body = serde_json::to_vec(&Value::Object(doc.clone())).map_err(|e| {
        Said::with_raw(
            copy_text(
                "beOwnState.write.encodeFailed",
                &[("path", &creds.file().display().to_string())],
            ),
            e,
        )
    })?;
    creds.write(held, &body)
}
