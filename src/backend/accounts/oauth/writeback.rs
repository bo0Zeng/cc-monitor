//! **续成了、写回在等那一家放写锁**：新令牌不能因为碰上锁就丢（很多服务端一续就作废旧的刷新令牌）。
//!
//! - 续期那一发碰上那一家的整份写锁（它正在存盘，通常毫秒级）⇒ 新令牌先留在进程内存里，这一发照用、不被挡；
//!   期间再来的请求照用内存里那份，不再续一次（[`served`]）。
//! - 写回交给一条等待线程（[`defer`]）：盯那个号的配置目录，写锁目录被删 / 凭据文件被换的文件事件一来，就照
//!   「重读 → 比对 → 只换那一节 → 整份原子写」写回（[`super::write_back`]，与当场写回同一处）。不睡、不轮询。
//! - 等有总期限（`LoginFace::write_wait_ms`，登记在零定时器的期限表里）：到期那一刻再试一次（持有方早不在的锁这时已过期、会被收回）；
//!   还写不回 ⇒ 记一行日志，下一次要这个号的令牌时照实报一次写回失败（点名这个号可能要重新登录），内存里那份作废。
//!
//! ⚠ 买不到的：后端进程在等的期间退出 ⇒ 内存里那份跟着没了（盘上还是旧的；服务端若已作废旧的刷新令牌，那个号要重新登录）。

use super::{Renewed, Unusable, WriteBack};
use crate::agents::LoginFace;
use creds_core::SecretKey;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// 一个号（按配置目录）此刻的样子。
enum State {
    /// 新令牌在内存里，写回在等。
    Waiting(Renewed),
    /// 等到期限也没写回：下一次要这个号的令牌时报一次。
    GaveUp,
}

static PENDING: Mutex<BTreeMap<PathBuf, State>> = Mutex::new(BTreeMap::new());

fn pending() -> std::sync::MutexGuard<'static, BTreeMap<PathBuf, State>> {
    PENDING.lock().unwrap_or_else(|e| e.into_inner())
}

/// 这个号此刻有没有一份在等写回的新令牌：有、还没到快过期 ⇒ 照用它；等到期限也没写回 ⇒ 报一次写回失败。
/// 都不是 ⇒ `None`（照常看盘）。
pub(crate) fn served(
    dir: &Path,
    face: &LoginFace,
    now_ms: u64,
) -> Option<Result<SecretKey, Unusable>> {
    let mut g = pending();
    match g.get(dir)? {
        State::Waiting(r) => {
            let (access, expires_ms) = r.access();
            (expires_ms > now_ms.saturating_add(face.margin_ms)).then(|| Ok(access.clone()))
        }
        State::GaveUp => {
            g.remove(dir);
            Some(Err(Unusable::WriteFailed(crate::common::said::Said::from(
                copy_core::copy_text("beOauth.writeback.timedOut", &[]),
            ))))
        }
    }
}

/// 试写回一次。写进去了 · 让位给盘上更新的 · 没有在等的 ⇒ `true`（这个号不再等）；写锁还在 ⇒ `false`。
fn attempt(own: &crate::platform::fs::Own, dir: &Path, face: &LoginFace, account: &str) -> bool {
    let Some(r) = (match pending().get(dir) {
        Some(State::Waiting(r)) => Some(r.clone()),
        _ => None,
    }) else {
        return true;
    };
    let done = super::store::creds_of(own, dir, face).and_then(|creds| {
        let held = creds.lock()?;
        super::write_back(&creds, face, &held, &r)
    });
    match &done {
        Ok(super::store::Locked::Busy) => return false,
        Ok(super::store::Locked::Held(Ok(WriteBack::Written))) => {
            tracing::info!("[auth] 续期：号 {account} · 等写锁放开之后写回了")
        }
        Ok(super::store::Locked::Held(Ok(WriteBack::Yielded(_)))) => {
            tracing::info!("[auth] 续期：号 {account} · 等写锁放开之后让位（盘上已是更新的）")
        }
        Ok(super::store::Locked::Held(Err(_))) | Err(_) => {
            tracing::warn!("[auth] 续期：号 {account} · 等写锁放开之后写回失败")
        }
    }
    let mut g = pending();
    if matches!(g.get(dir), Some(State::Waiting(_))) {
        g.remove(dir);
    }
    drop(g);
    true
}

/// 续成了、写锁有人持着：新令牌记进内存，起一条等待线程写回（这个号已经有一条在等 ⇒ 只换成这一份新的）。
pub(crate) fn defer(
    own: &crate::platform::fs::Own,
    dir: &Path,
    face: &LoginFace,
    account: &str,
    r: Renewed,
) {
    {
        let mut g = pending();
        let waiting = matches!(g.get(dir), Some(State::Waiting(_)));
        g.insert(dir.to_path_buf(), State::Waiting(r));
        if waiting {
            return;
        }
    }
    let (own, dir, face, account) = (own.clone(), dir.to_path_buf(), *face, account.to_string());
    let spawned = std::thread::Builder::new()
        .name("oauth-writeback".to_string())
        .spawn(move || wait_and_write(own, dir, face, account));
    if let Err(e) = spawned {
        tracing::warn!("[auth] 续期：等写锁的线程起不来：{e}");
    }
}

/// 等待线程本体：先装耳朵再试一次（装好之前就放开了的也认得出）→ 事件里每来一批试一次 → 期限到再试最后一次。
fn wait_and_write(own: crate::platform::fs::Own, dir: PathBuf, face: LoginFace, account: String) {
    let try_once: Arc<dyn Fn() -> bool + Send + Sync> = {
        let (own, dir, account) = (own.clone(), dir.clone(), account.clone());
        Arc::new(move || attempt(&own, &dir, &face, &account))
    };
    let watched: Vec<PathBuf> = [face.write_lock, Some(face.creds_file)]
        .into_iter()
        .flatten()
        .map(|n| dir.join(n))
        .collect();
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let on_event = Arc::clone(&try_once);
    let ears = crate::platform::watch_file::watch(
        &[(dir.clone(), false)],
        move |p| watched.iter().any(|w| w == p),
        "oauth-writeback-ears",
        move |_| {
            if on_event() {
                let _ = tx.send(());
            }
        },
    );
    if let Err(e) = &ears {
        tracing::warn!("[auth] 续期：号 {account} 的配置目录盯不上，只在期限到时再试一次：{e}");
    }
    #[allow(
        clippy::disallowed_methods,
        reason = "续成了、写回在等那一家放整份写锁时那一次有界等待的期限（`LoginFace::write_wait_ms`，交给 `rx.recv_timeout(within)`）：锁一放由文件事件当场叫醒，到期只再试一次写…"
    )]
    let within = std::time::Duration::from_millis(face.write_wait_ms);
    #[allow(
        clippy::disallowed_methods,
        reason = "续期写回碰上那一家的整份写锁时那一次有界等待（`writeback::wait_and_write`）：新令牌先留在内存里照用，写锁目录被删 / 凭据文件被换的文件事件当场叫醒写回；一直没放开就要有个头，到期再试一次、照…"
    )]
    let done = try_once() || (ears.is_ok() && rx.recv_timeout(within).is_ok()) || try_once();
    drop(ears);
    if !done {
        let mut g = pending();
        if matches!(g.get(&dir), Some(State::Waiting(_))) {
            g.insert(dir.clone(), State::GaveUp);
        }
        drop(g);
        tracing::warn!(
            "[auth] 续期：号 {account} · 写锁一直没放开 · 新令牌没写回 · 这个号可能要重新登录"
        );
    }
}
