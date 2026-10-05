//! 自动起算：每个号一个开关（缺省关）＋ 可选时段。开着、且这个号此刻没有在计时的 5h 窗口（从没见过重置时刻，
//! 或上一个窗口已过）⇒ 这台后端替它经 `ccm` 起一次官方 `claude -p`（最便宜的模型、最短一句、走中转），窗口就此开始计时，
//! 回包头照常经中转进额度账。
//!
//! - 判「该不该发、醒在哪一刻」只在 [`plan`]（纯函数）：醒的期限 ＝ 开着的号里最早那个重置时刻（或时段起点）；没有开着的号就不醒。
//! - 一个空闲期只试一次（[`StartPlan::episode`]）：发了没出数（要重新登录 / 中转不在 / claude 不在 …）⇒ 记 `failed`、不重试，
//!   等下一个真期限（额度账变了 · 改了开关或时段 · 后端重起）。
//! - 发那一趟（经 `ccm` 起那一家的程序）住 [`super::autostart_send`]；醒点与读写帧住 `faces/autostart_waker.rs` / `faces/autostart_face.rs`。
//! - 设置住 `rotation.json`（[`super::rotation::Book::autostart`]）；发那一趟的工作目录是家里的 [`relay_route_core::AUTOSTART_DIR_REL`]，
//!   历史与会话列表按它把这些会话藏掉（`observe::history_query::hidden_cwd`）。

use crate::agents::QuotaReading;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

/// 工作目录在家里的名字。
pub(crate) const DIR_NAME: &str =
    relay_route_core::file_name_of(relay_route_core::AUTOSTART_DIR_REL);

/// 一天的分钟数（时段的终点可以写 `24:00`）。
const DAY_MINUTES: u16 = 24 * 60;

/// 只在某个时段内起算（这台的本地钟）：`from` · `to` 都是 `HH:MM`，`to` 可以是 `24:00`；`from > to` ⇒ 跨午夜。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct DayWindow {
    pub from: String,
    pub to: String,
}

/// 起算没成的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum AutostartFail {
    /// 这台的中转没在跑（不经中转发出去的那一句不进额度账）。
    RelayDown,
    /// 这个号要重新登录（登录文件或账号身份拿不到）。
    NeedsLogin,
    /// 起不了 `claude`（不在 PATH 上）。
    NoClaude,
    /// 那一趟过了期限没结束（已杀）。
    TimedOut,
    /// 跑完了，额度账上没出这个号的新数。
    NoReading,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct AutostartFailed {
    pub code: AutostartFail,
    #[cfg_attr(test, ts(type = "number"))]
    pub at: u64,
}

/// 一个号的自动起算（`rotation.json` 里 `autostart.<号>`）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutoConf {
    pub(crate) enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) window: Option<DayWindow>,
    /// 上一次起算成功（发出去、额度账出了新数）的时刻。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) last_at: Option<u64>,
    /// 上一次起算没成（成了就清掉）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) failed: Option<AutostartFailed>,
}

/// 下一次起算：某一刻 ／ 现在 ／ 那一刻在时段外、到时段起点再发。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum AutostartNext {
    At(#[cfg_attr(test, ts(type = "number"))] u64),
    Now,
    #[serde(rename_all = "camelCase")]
    OutsideWindow {
        #[cfg_attr(test, ts(type = "number"))]
        at: u64,
    },
}

/// `autostart-read` 里一个号的那一行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct AutostartRow {
    pub account: String,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub window: Option<DayWindow>,
    /// 开着才有；一个空闲期已经试过没成 ⇒ 缺（等下一个真期限）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub next: Option<AutostartNext>,
    /// 那一趟正在跑。
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub last_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub failed: Option<AutostartFailed>,
    /// 这台设成「关主窗口即停后端」：停着的时候不起算。
    pub inactive: bool,
}

// ── 时段 ─────────────────────────────────────────────────────────────────

/// `HH:MM` ⇒ 一天里的第几分钟（`00:00` ..= `24:00`）。
fn minute_of(s: &str) -> Option<u16> {
    let (h, m) = s.split_once(':')?;
    if h.len() != 2 || m.len() != 2 {
        return None;
    }
    let (h, m): (u16, u16) = (h.parse().ok()?, m.parse().ok()?);
    let at = h.checked_mul(60)?.checked_add(m)?;
    (m < 60 && at <= DAY_MINUTES).then_some(at)
}

/// 读一个时段：`null` ⇒ 全天；`{from, to}` 都是 `HH:MM`，`from` 不许是 `24:00`，起止不许同一时刻。
/// 不合法 ⇒ `Err(哪一格、为什么)`（英文诊断，不进文案表）。
pub(crate) fn window_from(v: &Value) -> Result<Option<DayWindow>, String> {
    if v.is_null() {
        return Ok(None);
    }
    let o = v
        .as_object()
        .ok_or("window must be null or an object {from, to}")?;
    if let Some(k) = o.keys().find(|k| !matches!(k.as_str(), "from" | "to")) {
        return Err(format!("window: unknown key `{k}`"));
    }
    let field = |k: &str| -> Result<(String, u16), String> {
        let s = o
            .get(k)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("window.{k} must be a string HH:MM"))?;
        let m =
            minute_of(s).ok_or_else(|| format!("window.{k} must be HH:MM within 00:00–24:00"))?;
        Ok((s.to_string(), m))
    };
    let ((from, f), (to, t)) = (field("from")?, field("to")?);
    if f == DAY_MINUTES {
        return Err("window.from must be before 24:00".into());
    }
    if f == t % DAY_MINUTES {
        return Err("window.from and window.to must differ".into());
    }
    Ok(Some(DayWindow { from, to }))
}

/// 本地钟：某一刻比 UTC 快多少秒（生产 = `platform::local_time::utc_offset_at`）。
pub(crate) type LocalClock<'a> = &'a dyn Fn(u64) -> i64;

const DAY: i64 = 86_400;

fn bounds(w: &DayWindow) -> Option<(i64, i64)> {
    Some((
        i64::from(minute_of(&w.from)?) * 60,
        i64::from(minute_of(&w.to)?) * 60,
    ))
}

/// `t` 在不在时段里（`[from, to)`；跨午夜照算）。没有时段 / 时段坏了 ⇒ 全天。
fn inside(w: Option<&DayWindow>, t: u64, local: LocalClock<'_>) -> bool {
    let Some((f, to)) = w.and_then(bounds) else {
        return true;
    };
    let s = (t as i64 + local(t)).rem_euclid(DAY);
    if f < to {
        f <= s && s < to
    } else {
        s >= f || s < to
    }
}

/// `t` 起（含）第一个在时段里的时刻：在里面就是 `t`，否则是下一个时段起点。
fn first_inside_from(w: Option<&DayWindow>, t: u64, local: LocalClock<'_>) -> u64 {
    if inside(w, t, local) {
        return t;
    }
    let Some((f, _)) = w.and_then(bounds) else {
        return t;
    };
    let loc = t as i64 + local(t);
    let mut start = loc - loc.rem_euclid(DAY) + f;
    if start <= loc {
        start += DAY;
    }
    // 换回 UTC：按起点那一刻的本地钟（跨夏令时那天也落在本地的同一个钟点上）。
    let guess = start - local(t);
    let at = start - local(u64::try_from(guess).unwrap_or(t));
    u64::try_from(at).unwrap_or(t).max(t)
}

// ── 判 ───────────────────────────────────────────────────────────────────

/// 额度账上这个号要看的两样：5h 那一窗几点重置 · 被拒到几点（被拒时别的窗口卡着，发了也白发）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct WindowSeen {
    pub(crate) five_hour_reset: Option<u64>,
    pub(crate) refused_until: Option<u64>,
}

/// 从一份额度快照取 [`WindowSeen`]。`slot` ＝ 窗口名 → 语义位（`5h` / `7d`，那一家的读法）。
pub(crate) fn seen_of(r: &QuotaReading, slot: &dyn Fn(&str) -> Option<&'static str>) -> WindowSeen {
    WindowSeen {
        five_hour_reset: r
            .windows
            .iter()
            .find(|w| slot(&w.name) == Some("5h"))
            .and_then(|w| w.resets_at),
        refused_until: r.refused.then_some(r.resets_at).flatten(),
    }
}

/// 一个号此刻的判。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StartPlan {
    /// 此刻就该发。
    pub(crate) due: bool,
    /// 给界面的「下一次」。
    pub(crate) next: Option<AutostartNext>,
    /// 这个号要后端在哪一刻醒（`due` 时 ＝ 此刻）；不用醒 ⇒ `None`。
    pub(crate) wake: Option<u64>,
    /// 这一段空闲期的身份（最后一个已过的重置时刻；从没见过 ⇒ 0）：一个空闲期只试一次。
    pub(crate) episode: u64,
}

/// ★ 自动起算的**唯一判定**（纯函数）。`seen` ＝ 额度账上它的那两样（从没出过数 ⇒ `None`）；
/// `tried` ＝ 这个进程里它上一次试的是哪一段空闲期（没试过 ⇒ `None`）。
pub(crate) fn plan(
    conf: &AutoConf,
    seen: Option<WindowSeen>,
    tried: Option<u64>,
    now: u64,
    local: LocalClock<'_>,
) -> StartPlan {
    let seen = seen.unwrap_or_default();
    let known = [seen.five_hour_reset, seen.refused_until];
    let busy_until = known.iter().flatten().copied().filter(|t| *t > now).max();
    let episode = known
        .iter()
        .flatten()
        .copied()
        .filter(|t| *t <= now)
        .max()
        .unwrap_or(0);
    let idle = StartPlan {
        due: false,
        next: None,
        wake: None,
        episode,
    };
    if !conf.enabled {
        return idle;
    }
    let w = conf.window.as_ref();
    if let Some(t) = busy_until {
        // 窗口在计时（或被拒着）：醒在它到点那一刻；那一刻在时段外 ⇒ 醒在下一个时段起点。
        let at = first_inside_from(w, t, local);
        let next = if at == t {
            AutostartNext::At(t)
        } else {
            AutostartNext::OutsideWindow { at }
        };
        return StartPlan {
            next: Some(next),
            wake: Some(at),
            ..idle
        };
    }
    if tried == Some(episode) {
        return idle;
    }
    if inside(w, now, local) {
        return StartPlan {
            due: true,
            next: Some(AutostartNext::Now),
            wake: Some(now),
            ..idle
        };
    }
    let at = first_inside_from(w, now, local);
    StartPlan {
        next: Some(AutostartNext::OutsideWindow { at }),
        wake: Some(at),
        ..idle
    }
}

/// 这台家里的工作目录（[`DIR_NAME`]）；家推不出 ⇒ `None`。
pub(crate) fn dir_from(get: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    super::ledger::path_from(get).map(|p| p.with_file_name(DIR_NAME))
}

// ── 进程里的记忆：哪个号正在发 · 哪一段空闲期试过了 ───────────────────────────

/// 醒点线程与读帧共看的那一份（只在内存：后端重起 ⇒ 空闲的号当场再试一次）。
#[derive(Default)]
pub(crate) struct Memory {
    tried: std::sync::Mutex<std::collections::BTreeMap<String, u64>>,
    running: std::sync::Mutex<std::collections::BTreeSet<String>>,
}

impl Memory {
    pub(crate) fn tried(&self, account: &str) -> Option<u64> {
        self.tried
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(account)
            .copied()
    }

    pub(crate) fn note_tried(&self, account: &str, episode: u64) {
        self.tried
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(account.to_string(), episode);
    }

    /// 改了这个号的开关或时段 ⇒ 忘掉它试过哪一段（打开那一刻空闲就当场发）。
    pub(crate) fn forget(&self, account: &str) {
        self.tried
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(account);
    }

    pub(crate) fn running(&self, account: &str) -> bool {
        self.running
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(account)
    }

    pub(crate) fn set_running(&self, account: &str, on: bool) {
        let mut g = self.running.lock().unwrap_or_else(|e| e.into_inner());
        if on {
            g.insert(account.to_string());
        } else {
            g.remove(account);
        }
    }
}

/// 这个进程的那一份。
pub(crate) fn memory() -> &'static Memory {
    static M: std::sync::OnceLock<Memory> = std::sync::OnceLock::new();
    M.get_or_init(Memory::default)
}

/// 醒点的信箱：有动静就投一下（醒点醒来重算期限）。醒点没起（一次性进程）⇒ 投了也没人收。
fn inbox() -> &'static std::sync::Mutex<Option<std::sync::mpsc::Sender<()>>> {
    static TX: std::sync::OnceLock<std::sync::Mutex<Option<std::sync::mpsc::Sender<()>>>> =
        std::sync::OnceLock::new();
    TX.get_or_init(Default::default)
}

/// 投一下信箱。
pub(crate) fn poke() {
    if let Some(tx) = inbox().lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        let _ = tx.send(());
    }
}

/// 醒点起来时开信箱（取走收件那一头）。
pub(crate) fn open_inbox() -> std::sync::mpsc::Receiver<()> {
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    *inbox().lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
    rx
}

/// 进程里那条「自动起算显示变了」的通道（流连接订它推 `autostart_changed`）。
pub(crate) fn bell() -> std::sync::Arc<tokio::sync::watch::Sender<u64>> {
    static BELL: std::sync::OnceLock<std::sync::Arc<tokio::sync::watch::Sender<u64>>> =
        std::sync::OnceLock::new();
    std::sync::Arc::clone(
        BELL.get_or_init(|| std::sync::Arc::new(tokio::sync::watch::channel::<u64>(0).0)),
    )
}

/// 响一下（自动起算的设置或状态变了）。
pub(crate) fn ring() {
    bell().send_modify(|n| *n = n.wrapping_add(1));
}

/// 一个号的那一行（线上）。
pub(crate) fn row_of(
    account: &str,
    conf: &AutoConf,
    p: &StartPlan,
    running: bool,
    inactive: bool,
) -> AutostartRow {
    AutostartRow {
        account: account.to_string(),
        enabled: conf.enabled,
        window: conf.window.clone(),
        next: if running { None } else { p.next },
        running,
        last_at: conf.last_at,
        failed: conf.failed.clone(),
        inactive,
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/autostart_tests.rs"]
mod tests;
