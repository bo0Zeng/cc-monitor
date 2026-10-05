//! 自动起算的醒点（常驻后端里一条线程）：只醒在真期限上 —— 开着的号里最早那个重置时刻 / 时段起点（到点替它发那一句），
//! 以及卡住的会话里最早恢复的那一刻（到点推一次 `rotation_changed`，界面据此把提示条换成「可续发」）；都没有 ⇒ 无期限地等。
//! 有动静（额度账变了 · 轮换变了 · 改了开关或时段）就投一下信箱，醒来重算一次期限。

use crate::accounts::quota::autostart::{self, AutostartFail, LocalClock, Memory};
use crate::accounts::quota::{autostart_send, rotation};
use crate::faces::autostart_face::{self, Cand, Look, AGENT};
use crate::faces::rotation_face::Ctx;
use crate::platform::child::ChildFail;
use std::collections::BTreeMap;
use std::path::Path;

/// 醒点够得着的外面：钟 · 本地钟 · 现读 · 中转在不在 · 起那一趟 · 记结果 · 推会话变了。生产那一份见 [`start`]；判据全换假的。
pub(crate) struct World<'a> {
    pub(crate) now: &'a dyn Fn() -> u64,
    pub(crate) local: LocalClock<'a>,
    pub(crate) look: &'a dyn Fn() -> Look,
    pub(crate) relay_up: &'a dyn Fn() -> bool,
    /// 用这个配置目录起那一趟（回退出码）。
    pub(crate) send: &'a dyn Fn(&Path) -> Result<i32, ChildFail>,
    pub(crate) record: &'a dyn Fn(&str, Result<(), AutostartFail>, u64),
    pub(crate) push_session: &'a dyn Fn(&str),
}

/// 醒点：每一步现读、发到点的、推到点的，回下一个期限。
pub(crate) struct Waker<'m> {
    mem: &'m Memory,
    /// 卡住的会话 → 它最早回来的那一刻（到点推一次）。
    pending: BTreeMap<String, u64>,
}

impl<'m> Waker<'m> {
    pub(crate) fn new(mem: &'m Memory) -> Self {
        Self {
            mem,
            pending: BTreeMap::new(),
        }
    }

    /// 走一步：到点的号各发一次、到点的卡住会话各推一次；回下一个要醒的时刻（没有 ⇒ `None`）。
    /// 发过一次 ⇒ 回「此刻」，让调用方现读一遍再算（新的重置时刻在额度账上）。
    pub(crate) fn step(&mut self, w: &World<'_>) -> Option<u64> {
        let now = (w.now)();
        let look = (w.look)();
        let mut wake: Option<u64> = None;
        let mut soonest = |t: u64| wake = Some(wake.map_or(t, |w| w.min(t)));
        for c in &look.accounts {
            let p = autostart::plan(&c.conf, c.seen, self.mem.tried(&c.id), now, w.local);
            if p.due {
                self.send_one(w, c, p.episode);
                soonest((w.now)());
            } else if let Some(t) = p.wake {
                soonest(t);
            }
        }
        let due: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, at)| **at <= now)
            .map(|(sid, _)| sid.clone())
            .collect();
        for sid in &due {
            (w.push_session)(sid);
        }
        self.pending = look
            .stuck
            .into_iter()
            .filter(|(sid, at)| *at > now && !due.contains(sid))
            .collect();
        for at in self.pending.values() {
            soonest(*at);
        }
        wake
    }

    fn send_one(&self, w: &World<'_>, c: &Cand, episode: u64) {
        let start = (w.now)();
        self.mem.note_tried(&c.id, episode);
        let r = if !(w.relay_up)() {
            Err(AutostartFail::RelayDown)
        } else if c.needs_login {
            Err(AutostartFail::NeedsLogin)
        } else {
            self.mem.set_running(&c.id, true);
            autostart::ring();
            let ran = (w.send)(&c.dir);
            self.mem.set_running(&c.id, false);
            let after = (w.look)();
            let now_of = after.accounts.iter().find(|a| a.id == c.id);
            let heard = now_of.and_then(|a| a.seen_at).is_some_and(|t| t >= start);
            autostart_send::verdict(&ran, heard, now_of.is_some_and(|a| a.needs_login))
        };
        if let Err(code) = r {
            tracing::warn!("自动起算 {}：没成（{code:?}）", c.id);
        }
        (w.record)(&c.id, r, start);
        autostart::ring();
    }
}

// ── 生产那一份 ────────────────────────────────────────────────────────────

fn relay_up_here() -> bool {
    std::env::var(crate::relay::ENV_PORT)
        .ok()
        .and_then(|s| s.trim().parse::<u16>().ok())
        .is_some_and(crate::relay::our_relay_listening)
}

fn send_here(account_dir: &Path) -> Result<i32, ChildFail> {
    let me = std::env::current_exe().map_err(ChildFail::Io)?;
    let dir = autostart::dir_from(&|k| std::env::var(k).ok())
        .ok_or_else(|| ChildFail::Io(std::io::Error::from(std::io::ErrorKind::NotFound)))?;
    autostart_send::run_ccm(&me, AGENT, account_dir, &dir)
}

/// 等到 `wake`（`None` ⇒ 无期限）或有动静。信箱没了 ⇒ `false`。
fn wait_for(pokes: &std::sync::mpsc::Receiver<()>, wake: Option<u64>, now: u64) -> bool {
    use std::sync::mpsc::RecvTimeoutError;
    match wake {
        None => pokes.recv().is_ok(),
        Some(t) if t <= now => true,
        Some(t) => {
            let secs = t - now;
            !matches!(
                pokes.recv_timeout(std::time::Duration::from_secs(secs)),
                Err(RecvTimeoutError::Disconnected)
            )
        }
    }
}

/// 起醒点（常驻那条载体上调一次）：一条线程只醒在真期限上；额度账 · 轮换一变就投一下信箱。
pub fn start() {
    let pokes = autostart::open_inbox();
    let mut quota = crate::accounts::quota::ledger::bell().subscribe();
    tokio::spawn(async move {
        while quota.changed().await.is_ok() {
            autostart::poke();
        }
    });
    let mut rot = rotation::changes().subscribe();
    tokio::spawn(async move {
        loop {
            match rot.recv().await {
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    autostart::poke()
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
    let spawned = std::thread::Builder::new()
        .name("autostart".into())
        .spawn(move || {
            let now = crate::accounts::quota::now_unix;
            let local = crate::platform::local_time::utc_offset_at;
            let look = || autostart_face::look_with(&Ctx::here(), now());
            let record = autostart_face::record;
            let push = |sid: &str| {
                let _ = rotation::changes().send(sid.to_string());
            };
            let world = World {
                now: &now,
                local: &local,
                look: &look,
                relay_up: &relay_up_here,
                send: &send_here,
                record: &record,
                push_session: &push,
            };
            let mut waker = Waker::new(autostart::memory());
            loop {
                let wake = waker.step(&world);
                if !wait_for(&pokes, wake, now()) {
                    break;
                }
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("自动起算：醒点线程起不来：{e}");
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/autostart_waker_tests.rs"]
mod tests;
