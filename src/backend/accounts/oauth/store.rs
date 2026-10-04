//! 那个号的凭据文件：读 · 续期时的锁 · 整份原子写回。只碰这一份文件与那两把锁目录。
//!
//! 锁与那个号自己的 claude 用同一套（这样两边不会同时拿同一个刷新令牌去续）：
//! 配置目录里那一把（`<目录>/<inside>`）＋ 配置目录旁边那一把（`<目录><beside>`），都是 `mkdir` 成功即持有、`rmdir` 即放。
//! 拿不到 ⇒ 当场说「正在被别的进程续」，不等（后端零定时器）。另在跨进程锁里做（同一台的几个后端进程之间）。

use crate::agents::LoginFace;
use copy_core::copy_text;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// 读一次凭据文件。
pub(crate) enum Read {
    /// 文件不在。
    Absent,
    Present(Map<String, Value>),
    /// 文件在但读不出来 / 不是 JSON 对象（只给一句原因，不带内容）。
    Unreadable(String),
}

pub(crate) fn creds_path(dir: &Path, face: &LoginFace) -> PathBuf {
    dir.join(face.creds_file)
}

pub(crate) fn read(dir: &Path, face: &LoginFace) -> Read {
    let p = creds_path(dir, face);
    match std::fs::read_to_string(&p) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Read::Absent,
        Err(e) => Read::Unreadable(e.kind().to_string()),
        Ok(s) => match serde_json::from_str::<Value>(&s) {
            Ok(Value::Object(m)) => Read::Present(m),
            Ok(_) => Read::Unreadable(copy_text("beOauthStore.read.notObject", &[])),
            // 解析错误的文本可能带着原文片段 ⇒ 只说「不是 JSON」。
            Err(_) => Read::Unreadable(copy_text("beOauthStore.read.notJson", &[])),
        },
    }
}

/// 持着续期锁时调 `work`；锁有人持着 ⇒ `Err(Busy)`，一把都不留。
pub(crate) enum Locked<T> {
    Held(T),
    Busy,
}

/// 在续期锁里做一件事：跨进程锁（配置目录）→ 里面那一把 → 旁边那一把 → `work` → 倒序放掉。
pub(crate) fn with_refresh_lock<T>(
    dir: &Path,
    face: &LoginFace,
    work: impl FnOnce() -> T,
) -> Result<Locked<T>, String> {
    let _held = crate::platform::lock::hold(dir)?;
    let inside = dir.join(face.lock_inside);
    if !take(&inside)? {
        return Ok(Locked::Busy);
    }
    let beside = face.lock_beside.map(|suffix| {
        let mut s = dir.as_os_str().to_os_string();
        s.push(suffix);
        PathBuf::from(s)
    });
    if let Some(b) = beside.as_deref() {
        match take(b) {
            Ok(true) => {}
            other => {
                release(&inside);
                return other.map(|_| Locked::Busy);
            }
        }
    }
    let out = work();
    if let Some(b) = beside.as_deref() {
        release(b);
    }
    release(&inside);
    Ok(Locked::Held(out))
}

/// 放一把 `mkdir` 锁（删自己刚建的那个空目录）。放不掉要留一行日志：那把锁会一直挡着那个号自己的 claude 续期。
fn release(p: &Path) {
    if let Err(e) = std::fs::remove_dir(p) {
        tracing::warn!("[oauth] {}: {e}", p.display());
    }
}

/// `mkdir` 一把锁：建成 ⇒ 持有；已在 ⇒ 别人持着。
fn take(p: &Path) -> Result<bool, String> {
    match std::fs::create_dir(p) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(copy_text(
            "beOauthStore.lock.failed",
            &[("path", &p.display().to_string()), ("e", &e.to_string())],
        )),
    }
}

/// 整份原子写回（只在续期锁里调）：出生即只给本人的临时文件 → 写满 → 原子挪过去；失败删自己的临时文件。
pub(crate) fn write_tokens(
    dir: &Path,
    face: &LoginFace,
    doc: &Map<String, Value>,
) -> Result<(), String> {
    use std::io::Write as _;
    let path = creds_path(dir, face);
    let failed = |e: &dyn std::fmt::Display| {
        copy_text(
            "beOauthStore.write.failed",
            &[("path", &path.display().to_string()), ("e", &e.to_string())],
        )
    };
    let body = serde_json::to_string(&Value::Object(doc.clone())).map_err(|e| failed(&e))?;
    let tmp = dir.join(format!(
        "{}.ccm.{}.tmp",
        face.creds_file,
        std::process::id()
    ));
    let result = (|| {
        let mut f = creds_core::perm::create_private(&tmp).map_err(|e| failed(&e))?;
        f.write_all(body.as_bytes())
            .and_then(|()| f.sync_all())
            .map_err(|e| failed(&e))?;
        drop(f);
        std::fs::rename(&tmp, &path).map_err(|e| failed(&e))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}
