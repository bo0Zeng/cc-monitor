//! 自动起算的帧面宿主：`autostart-read` / `autostart-set`（这台账号库里每个订阅号的开关 · 时段 · 下一次 · 正在发 · 上次 · 失败 ·
//! 停着不起算）＋ 把账号库 · 额度账 · 轮换 · 活会话读成判定要的那几样（[`look_with`]，醒点 `autostart_waker` 也读它）＋
//! 记一次起算的结果（[`record`]）。判「该不该发、醒在哪一刻」只在 `accounts::quota::autostart::plan`。

use crate::accounts::quota::autostart::{
    self, AutoConf, AutostartFail, AutostartFailed, AutostartRow, LocalClock, Memory, WindowSeen,
};
use crate::accounts::quota::rotation::{self, SessionRotationState};
use crate::faces::rotation_face::Ctx;
use serde_json::{json, Value};
use std::path::PathBuf;

type Answer = Result<Value, (&'static str, String)>;

/// 账号库是这一家的（凭据文件那一家）。
pub(crate) const AGENT: &str = crate::accounts::upstream_select::CREDENTIALS_FILE_AGENT;

/// 一个能起算的号此刻的样子。
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Cand {
    pub(crate) id: String,
    /// 那个号的配置目录（交给 `ccm --account-dir`）。
    pub(crate) dir: PathBuf,
    pub(crate) conf: AutoConf,
    /// 额度账上它的那两样；从没出过数 ⇒ `None`。
    pub(crate) seen: Option<WindowSeen>,
    /// 额度账上它最近一次的时刻。
    pub(crate) seen_at: Option<u64>,
    /// 要重新登录（只对开着的号判）。
    pub(crate) needs_login: bool,
}

/// 醒点每一步现读的事实。
#[derive(Debug, Clone, Default)]
pub(crate) struct Look {
    pub(crate) accounts: Vec<Cand>,
    /// 此刻活着、发不出去的会话，和它们池里最早回来的那一刻。
    pub(crate) stuck: Vec<(String, u64)>,
}

// ── 生产那一份 ────────────────────────────────────────────────────────────

/// 这台账号库里能起算的号：订阅号（不含账号 0 —— 它没有自己的配置目录 —— 也不含按量号）及其配置目录。
fn candidates(ctx: &Ctx) -> Vec<(String, PathBuf)> {
    ctx.hop
        .library()
        .accounts
        .into_iter()
        .filter(|a| !ctx.is_api(AGENT, &a.id))
        .filter_map(|a| Some((a.id, a.dir?)))
        .collect()
}

pub(crate) fn look_with(ctx: &Ctx, now: u64) -> Look {
    let book = ctx.hop.store.now();
    let lib = ctx.hop.library();
    let slot = crate::agents::window_slot_of(AGENT);
    let slot_of = |w: &str| slot.and_then(|f| f(w));
    let row = |a: &str| (ctx.rows)(AGENT, a);
    let accounts = candidates(ctx)
        .into_iter()
        .map(|(id, dir)| {
            let conf = book.autostart.get(&id).cloned().unwrap_or_default();
            let o = ctx.hop.quota.entry(AGENT, &id);
            let needs_login = conf.enabled
                && ctx.hop.show_facts(AGENT, &lib, &id, &row).login
                    == crate::accounts::quota::show::LoginState::NeedsLogin;
            Cand {
                seen: o.as_ref().map(|o| autostart::seen_of(&o.reading, &slot_of)),
                seen_at: o.map(|o| o.seen_at),
                id,
                dir,
                conf,
                needs_login,
            }
        })
        .collect();
    let live = (ctx.live)();
    let stuck = book
        .sessions
        .keys()
        .filter(|sid| live.contains(*sid))
        .filter_map(|sid| {
            let view = ctx.hop.view(&book, sid, &row, &|s| live.contains(s), now);
            let SessionRotationState::Present(v) = view else {
                return None;
            };
            Some((sid.clone(), v.blocked?.earliest?.at))
        })
        .collect();
    Look { accounts, stuck }
}

/// 记一次起算的结果（成 ⇒ 上次 ＝ 那一刻、清掉失败；不成 ⇒ 失败 ＝ 码 ＋ 那一刻）。醒点调它（写口只从本文件进）。
pub(crate) fn record(account: &str, r: Result<(), AutostartFail>, at: u64) {
    let store = rotation::RotationStore::at(rotation::path_now());
    let wrote = rotation::autostart_change(&store, |b| {
        let Some(e) = b.autostart.get_mut(account) else {
            return;
        };
        match r {
            Ok(()) => {
                e.last_at = Some(at);
                e.failed = None;
            }
            Err(code) => e.failed = Some(AutostartFailed { code, at }),
        }
    });
    if let Err(e) = wrote {
        tracing::warn!("自动起算 {account}：记不下结果：{e}");
    }
}

// ── 帧面命令 ──────────────────────────────────────────────────────────────

fn bad(detail: &str) -> (&'static str, String) {
    ("bad_args", crate::common::contract::malformed(detail))
}

/// 这台设成「关主窗口即停后端」：停着的时候不起算。
fn inactive_here() -> bool {
    crate::control::exit_policy::read_now().kill_on_exit()
}

/// `autostart-read`：这台每个订阅号那一行。
pub(crate) fn answer_read() -> Value {
    read_with(
        &Ctx::here(),
        autostart::memory(),
        crate::accounts::quota::now_unix(),
        &crate::platform::local_time::utc_offset_at,
        inactive_here(),
    )
}

pub(crate) fn read_with(
    ctx: &Ctx,
    mem: &Memory,
    now: u64,
    local: LocalClock<'_>,
    inactive: bool,
) -> Value {
    let path = ctx.hop.store.path();
    let (state, reason) = match path.map(rotation::read_at) {
        None => (
            "unreadable",
            Some(copy_core::copy_text("beRotation.read.noHome", &[])),
        ),
        Some(rotation::Read::Absent) => ("absent", None),
        Some(rotation::Read::Present(_)) => ("present", None),
        Some(rotation::Read::Unreadable(e)) => ("unreadable", Some(e)),
    };
    let rows: Vec<AutostartRow> = look_with(ctx, now)
        .accounts
        .iter()
        .map(|c| {
            let p = autostart::plan(&c.conf, c.seen, mem.tried(&c.id), now, local);
            autostart::row_of(&c.id, &c.conf, &p, mem.running(&c.id), inactive)
        })
        .collect();
    json!({
        "state": state,
        "reason": reason,
        "path": path.map(|p| p.display().to_string()),
        "now": now,
        "accounts": rows,
    })
}

/// `autostart-set`：改一个号的开关 / 时段（给了哪格改哪格）。应答同 `autostart-read`。
pub(crate) fn answer_set(args: &Value) -> Answer {
    let ctx = Ctx::here();
    set_with(&ctx, autostart::memory(), args)?;
    autostart::poke();
    Ok(answer_read())
}

pub(crate) fn set_with(
    ctx: &Ctx,
    mem: &Memory,
    args: &Value,
) -> Result<(), (&'static str, String)> {
    let o = args
        .as_object()
        .ok_or_else(|| bad("args must be an object {account, enabled?, window?}"))?;
    if let Some(k) = o
        .keys()
        .find(|k| !matches!(k.as_str(), "account" | "enabled" | "window"))
    {
        return Err(bad(&format!("unknown key `{k}`")));
    }
    let account = o
        .get("account")
        .and_then(Value::as_str)
        .ok_or_else(|| bad("`account` must be a string"))?;
    if !candidates(ctx).iter().any(|(id, _)| id == account) {
        return Err(bad(&format!(
            "`account`: {account:?} is not a subscription account on this machine"
        )));
    }
    let enabled = match o.get("enabled") {
        None => None,
        Some(Value::Bool(b)) => Some(*b),
        Some(_) => return Err(bad("`enabled` must be a boolean")),
    };
    let window = match o.get("window") {
        None => None,
        Some(v) => Some(autostart::window_from(v).map_err(|e| bad(&e))?),
    };
    if enabled.is_none() && window.is_none() {
        return Err(bad("give `enabled` or `window`"));
    }
    if ctx.hop.store.path().is_none() {
        return Err((
            "io_failed",
            copy_core::copy_text("beRotation.read.noHome", &[]),
        ));
    }
    rotation::autostart_change(&ctx.hop.store, |b| {
        let e = b.autostart.entry(account.to_string()).or_default();
        if let Some(on) = enabled {
            if on && !e.enabled {
                e.failed = None;
            }
            e.enabled = on;
        }
        if let Some(w) = window {
            e.window = w;
        }
    })
    .map_err(|e| ("io_failed", e))?;
    mem.forget(account);
    autostart::ring();
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/autostart_face_tests.rs"]
mod tests;
