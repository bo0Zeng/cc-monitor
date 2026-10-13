//! 那个号的凭据文件：读 · 续期时的锁 · 整份原子写回。只碰这一份文件与那几把锁目录。
//!
//! **写这份文件只有一处**（[`rewrite`]）：在那一家整份写时拿的那把锁里（`LoginFace::write_lock`）重读盘 → 调用方看此刻的样子
//! 决定写什么（写前比对就在这一步）→ 整份原子写。续期写回、各号之间同步 MCP 登录都经它；那一家自己存盘也拿同一把、也在锁里重读，
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

/// 读盘的上限（一份令牌）。
const MAX_BYTES: u64 = 1 << 20;

pub(crate) fn creds_path(dir: &Path, face: &LoginFace) -> PathBuf {
    dir.join(face.creds_file)
}

pub(crate) fn read(dir: &Path, face: &LoginFace) -> Read {
    read_at(&creds_path(dir, face))
}

fn read_at(path: &Path) -> Read {
    crate::common::own_state::read_bytes(path, MAX_BYTES).and_then(|b| {
        match serde_json::from_slice::<Value>(&b) {
            Ok(Value::Object(m)) => Read::Present(m),
            Ok(_) => Read::Unreadable(copy_text("beOauthStore.read.notObject", &[]).into()),
            // 解析错误的文本可能带着原文片段 ⇒ 只说「不是 JSON」。
            Err(_) => Read::Unreadable(copy_text("beOauthStore.read.notJson", &[]).into()),
        }
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
    own.account_creds(
        dir,
        face.creds_file,
        face.lock_inside,
        face.lock_beside,
        face.write_lock,
    )
}

/// 在续期锁里做一件事：跨进程锁（配置目录）→ 里面那一把 → 旁边那一把 → `work`（收那把跨进程锁，写回要它）→ 守卫倒序放掉。
pub(crate) fn with_refresh_lock<T>(
    creds: &AccountCreds,
    face: &LoginFace,
    work: impl FnOnce(&Reclaimed, &CredsLock) -> T,
) -> Result<Locked<T>, Said> {
    let held = creds.lock()?;
    let stale = face.lock_stale_ms;
    let mut reclaimed = Reclaimed::default();
    let Some(_inside) = take(
        creds,
        RenewLock::Inside,
        stale,
        Some(("里面那把", &mut reclaimed)),
    )?
    else {
        return Ok(Locked::Busy);
    };
    let _beside = match creds.lock_path(&RenewLock::Beside) {
        None => None,
        Some(_) => match take(
            creds,
            RenewLock::Beside,
            stale,
            Some(("旁边那把", &mut reclaimed)),
        )? {
            Some(g) => Some(g),
            None => return Ok(Locked::Busy),
        },
    };
    Ok(Locked::Held(work(&reclaimed, &held)))
}

/// 续某一条登录（`key`）时那一家拿的那把锁里做一件事：跨进程锁（配置目录）→ 那一把 → `work`（收那把跨进程锁，写要它）。
/// 那一把有人持着（那个号自己的 claude 正在续这一条）⇒ `Busy`：它续完存盘会再来一个文件事件，到时再同步。
pub(crate) fn with_entry_lock<T>(
    creds: &AccountCreds,
    entries: &crate::agents::EntryLogins,
    key: &str,
    work: impl FnOnce(&CredsLock) -> T,
) -> Result<Locked<T>, Said> {
    let held = creds.lock()?;
    let which = RenewLock::OneLogin((entries.renew_lock)(key));
    let Some(_entry) = take(creds, which, entries.renew_lock_stale_ms, None)? else {
        return Ok(Locked::Busy);
    };
    Ok(Locked::Held(work(&held)))
}

/// 在那一家的整份写锁里改这份凭据文件（见模块头注）：拿写锁（登记了的话）→ 重读盘 → `edit` 看此刻的样子，
/// 回 `(Some(新的整份), r)` ⇒ 整份原子写；`(None, r)` ⇒ 不写 → 放锁。写锁有人持着 ⇒ `Busy`（一个字节不写）。
/// `held` 是配置目录那把跨进程锁（续期锁 / 那一条的续期锁里拿到的那一把）。
pub(crate) fn rewrite<T>(
    creds: &AccountCreds,
    face: &LoginFace,
    held: &CredsLock,
    edit: impl FnOnce(Read) -> (Option<Map<String, Value>>, T),
) -> Result<Locked<T>, Said> {
    let _write = match creds.lock_path(&RenewLock::Write) {
        None => None,
        Some(_) => match take(creds, RenewLock::Write, face.write_lock_stale_ms, None)? {
            Some(g) => Some(g),
            None => return Ok(Locked::Busy),
        },
    };
    let (doc, r) = edit(read_at(creds.file()));
    if let Some(doc) = doc {
        write_doc(creds, held, &doc)?;
    }
    Ok(Locked::Held(r))
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
    let stale = Duration::from_millis(stale_ms);
    if make(creds, &which)? {
        return Ok(Some(Hold(creds, which)));
    }
    let age = creds
        .lock_path(&which)
        .and_then(|p| Ro::metadata(&p).and_then(|m| m.modified()).ok())
        .and_then(|t| crate::common::time::now().duration_since(t).ok());
    if !age.is_some_and(|a| a > stale) || creds.drop_lock(&which).is_err() || !make(creds, &which)?
    {
        return Ok(None);
    }
    if let Some((name, r)) = reclaimed {
        r.0.push(name);
    }
    Ok(Some(Hold(creds, which)))
}

/// `mkdir`：建成 ⇒ `true`；已在 ⇒ `false`。
fn make(creds: &AccountCreds, which: &RenewLock) -> Result<bool, Said> {
    creds.make_lock(which).map_err(|e| {
        Said::with_raw(
            copy_text(
                "beOauthStore.lock.failed",
                &[
                    (
                        "path",
                        &creds
                            .lock_path(which)
                            .map(|p| p.display().to_string())
                            .unwrap_or_default(),
                    ),
                    ("why", &copy_core::io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })
}

/// 整份原子写（只经 [`rewrite`]：`held` 是那把跨进程锁；出生即只给本人 → 写满 → 原子挪过去）。
fn write_doc(creds: &AccountCreds, held: &CredsLock, doc: &Map<String, Value>) -> Result<(), Said> {
    let body = serde_json::to_vec(&Value::Object(doc.clone())).map_err(|e| {
        Said::with_raw(
            copy_text(
                "beOauthStore.write.failed",
                &[("path", &creds.file().display().to_string())],
            ),
            e,
        )
    })?;
    creds.write(held, &body)
}
