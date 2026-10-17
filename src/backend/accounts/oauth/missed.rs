//! **续不上那一下成了事实**：一个号续令牌没成 ⇒ 按它的配置目录记下（连同那一刻凭据文件的戳），
//! 换号那一侧问 [`missed`] 就知道它此刻接不接得上，不再每一发去续一次。
//!
//! - 端点不收刷新令牌 · 过期又没有刷新令牌 · 里面没有令牌 ⇒ [`Unrenewed::Manual`]（要重新登录）；
//!   没连上 · 回包读不懂 · 写不回 ⇒ [`Unrenewed::RetryAt`]（到点再试）；别的进程正在续（锁有人持着）不记。
//! - 凭据文件一动（重新登录了 · 那个号自己续过）⇒ 这一条不作数；续成了 ⇒ 删掉。
//! - 只在进程内存里（同 [`super::writeback`]）：后端重启后第一次要这个号时再续一次，照实再记。

use super::Unusable;
use crate::agents::LoginFace;
use crate::platform::fs::{Ro, Stamp};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// 一时没续上之后隔多久再试（秒）。
const AGAIN_AFTER: u64 = 5 * 60;

/// 续不上之后那个号此刻的样子。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unrenewed {
    /// 要重新登录（需手动）。
    Manual,
    /// 一时没续上：到这一刻（unix 秒）再试。
    RetryAt(u64),
}

static MISSED: Mutex<BTreeMap<PathBuf, (Option<Stamp>, Unrenewed)>> = Mutex::new(BTreeMap::new());

fn book() -> std::sync::MutexGuard<'static, BTreeMap<PathBuf, (Option<Stamp>, Unrenewed)>> {
    MISSED.lock().unwrap_or_else(|e| e.into_inner())
}

fn stamp(dir: &Path, face: &LoginFace) -> Option<Stamp> {
    Ro::stamp(&super::store::creds_path(dir, face))
}

/// ★ 这个号此刻是不是续不上（唯一的问法）：记着、凭据文件没动过、（一时没续上的）还没到再试的时刻 ⇒ 那一条。
pub(crate) fn missed(dir: &Path, face: &LoginFace, now: u64) -> Option<Unrenewed> {
    let mut g = book();
    let (at, m) = *g.get(dir)?;
    let live = at == stamp(dir, face)
        && match m {
            Unrenewed::Manual => true,
            Unrenewed::RetryAt(t) => t > now,
        };
    if !live {
        g.remove(dir);
    }
    live.then_some(m)
}

/// 续了一次之后记账（[`super::access_token`] 唯一调）：成了 ⇒ 删；没成 ⇒ 记成哪一种。
pub(super) fn note(
    dir: &Path,
    face: &LoginFace,
    now: u64,
    r: &Result<creds_core::SecretKey, Unusable>,
) {
    let m = match r {
        Ok(_) | Err(Unusable::Busy) => None,
        Err(Unusable::Refused(_) | Unusable::Expired | Unusable::NotLoggedIn) => {
            Some(Unrenewed::Manual)
        }
        Err(Unusable::NotRenewed(_) | Unusable::WriteFailed(_) | Unusable::Unreadable(_)) => {
            Some(Unrenewed::RetryAt(now.saturating_add(AGAIN_AFTER)))
        }
    };
    let mut g = book();
    match m {
        Some(m) => {
            g.insert(dir.to_path_buf(), (stamp(dir, face), m));
        }
        None if r.is_ok() => {
            g.remove(dir);
        }
        None => {}
    }
}
