//! 那个号的凭据文件：读 · 续期时的锁 · 整份原子写回。只碰这一份文件与那两把锁目录。
//!
//! 锁与那个号自己的 claude 用同一套（这样两边不会同时拿同一个刷新令牌去续）：
//! 配置目录里那一把（`<目录>/<inside>`）＋ 配置目录旁边那一把（`<目录><beside>`），都是 `mkdir` 成功即持有、`rmdir` 即放；
//! 放在守卫的 `Drop` 里（`work` panic 或提前返回也放）。已在的锁目录修改时刻早过那一家的门限（`LoginFace::lock_stale_ms`）⇒
//! 持有方已经不在（被杀 / 崩了），删了重拿，收回了哪几把交给 `work` 记进续期那行日志。
//! 拿不到 ⇒ 当场说「正在被别的进程续」，不等（后端零定时器）。另在跨进程锁里做（同一台的几个后端进程之间）：
//! 我方几个后端之间不会两个同时判同一把过期；与那一家自己的进程之间，「判过期 → 删 → 重拿」不是原子的，与 proper-lockfile 自己的做法同一个窗口。

use crate::agents::LoginFace;
use crate::common::said::Said;
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
    crate::common::own_state::read_bytes(&creds_path(dir, face), MAX_BYTES).and_then(|b| {
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

/// 在续期锁里做一件事：跨进程锁（配置目录）→ 里面那一把 → 旁边那一把 → `work` → 守卫倒序放掉。
pub(crate) fn with_refresh_lock<T>(
    dir: &Path,
    face: &LoginFace,
    work: impl FnOnce(&Reclaimed) -> T,
) -> Result<Locked<T>, Said> {
    let _held = crate::platform::lock::hold(dir)?;
    let stale = Duration::from_millis(face.lock_stale_ms);
    let mut reclaimed = Reclaimed::default();
    let Some(_inside) = take(
        &dir.join(face.lock_inside),
        stale,
        "里面那把",
        &mut reclaimed,
    )?
    else {
        return Ok(Locked::Busy);
    };
    let beside = face.lock_beside.map(|suffix| {
        let mut s = dir.as_os_str().to_os_string();
        s.push(suffix);
        PathBuf::from(s)
    });
    let _beside = match beside.as_deref() {
        None => None,
        Some(b) => match take(b, stale, "旁边那把", &mut reclaimed)? {
            Some(g) => Some(g),
            None => return Ok(Locked::Busy),
        },
    };
    Ok(Locked::Held(work(&reclaimed)))
}

/// 持着的一把 `mkdir` 锁：守卫离开作用域就放（正常返回 · 提前返回 · panic 都走这里）。
struct Hold(PathBuf);

impl Drop for Hold {
    /// 放不掉要留一行日志：那把锁会一直挡着那个号自己的 claude 续期，直到它过期被收回。
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_dir(&self.0) {
            tracing::warn!("[oauth] {}: {e}", self.0.display());
        }
    }
}

/// 拿一把 `mkdir` 锁：建成 ⇒ 持有；已在而修改时刻早过 `stale` ⇒ 删了重拿（记进 `reclaimed`）；否则别人持着 ⇒ `None`。
fn take(
    p: &Path,
    stale: Duration,
    name: &'static str,
    reclaimed: &mut Reclaimed,
) -> Result<Option<Hold>, Said> {
    if make(p)? {
        return Ok(Some(Hold(p.to_path_buf())));
    }
    let age = std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| crate::common::time::now().duration_since(t).ok());
    if !age.is_some_and(|a| a > stale) || std::fs::remove_dir(p).is_err() || !make(p)? {
        return Ok(None);
    }
    reclaimed.0.push(name);
    Ok(Some(Hold(p.to_path_buf())))
}

/// `mkdir`：建成 ⇒ `true`；已在 ⇒ `false`。
fn make(p: &Path) -> Result<bool, Said> {
    match std::fs::create_dir(p) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(Said::with_raw(
            copy_text(
                "beOauthStore.lock.failed",
                &[
                    ("path", &p.display().to_string()),
                    ("why", &copy_core::io_reason(e.kind())),
                ],
            ),
            &e,
        )),
    }
}

/// 整份原子写回（只在续期锁里调；经 `own_state`：出生即只给本人 → 写满 → 原子挪过去）。
pub(crate) fn write_tokens(
    dir: &Path,
    face: &LoginFace,
    doc: &Map<String, Value>,
) -> Result<(), Said> {
    let path = creds_path(dir, face);
    let body = serde_json::to_vec(&Value::Object(doc.clone())).map_err(|e| {
        Said::with_raw(
            copy_text(
                "beOauthStore.write.failed",
                &[("path", &path.display().to_string())],
            ),
            e,
        )
    })?;
    crate::common::own_state::write(&path, &body)
}
