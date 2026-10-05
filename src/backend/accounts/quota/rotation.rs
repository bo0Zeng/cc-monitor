//! 轮换：额度满了（或到阈值）就把会话钉到轮换里下一个号上，不重启进程。落 `~/.cc-monitor/rotation.json`。
//!
//! - 每台一份默认轮换（顺序 · 勾了哪几个 · 换号时机）；缺省只有「起始账号」那一格 ⇒ 缺省不轮换。
//! - 每个会话跟随默认，或用自己那一份（切回跟随时自己那一份留着）；按会话 id 存，后端重启后还在。
//! - 每个会话此刻钉在哪个号、从什么时候起、换号记录；会话换了起它的号（重启换号 / 换号恢复）⇒ 钉号随之清掉。
//! - 判「换不换、换谁」只在 [`super::decide`]；这里只有存取。

use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub(crate) const FILE_NAME: &str = relay_route_core::file_name_of(relay_route_core::ROTATION_REL);

/// 每个会话至多留几条换号记录（最早的先丢）。
pub(crate) const HISTORY_KEPT: usize = 32;

/// 「到 N% 换」的 N 收哪些值。
pub(crate) const THRESHOLD_RANGE: std::ops::RangeInclusive<u8> = 50..=99;

/// 换号时机。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum RotationWhen {
    /// 上游拒了才换（缺省）。
    #[default]
    Full,
    /// 用量到 `n`% 就换（下一发起）。
    Threshold { n: u8 },
}

/// 轮换列表里的一格：起始账号占位，或一个号（路由第 2 段）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum RotationSlot {
    /// `{"start": true}`：起这个会话的那个号。
    Start(StartSlot),
    Named(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct StartSlot {
    pub start: bool,
}

/// 一份轮换：顺序 · 勾了哪几个 · 换号时机。起始账号占位恒算勾上。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct Rotation {
    pub order: Vec<RotationSlot>,
    pub enabled: Vec<String>,
    pub when: RotationWhen,
}

impl Default for Rotation {
    /// 缺省：只有起始账号 ⇒ 不轮换。
    fn default() -> Self {
        Self {
            order: vec![RotationSlot::Start(StartSlot { start: true })],
            enabled: Vec::new(),
            when: RotationWhen::Full,
        }
    }
}

impl Rotation {
    /// 这个会话实际的轮换池（按序、去重）：占位换成起始账号，具名的只取勾上的。
    pub(crate) fn pool(&self, start: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for slot in &self.order {
            let a = match slot {
                RotationSlot::Start(_) => start,
                RotationSlot::Named(a) if self.enabled.iter().any(|e| e == a) => a.as_str(),
                RotationSlot::Named(_) => continue,
            };
            if !out.iter().any(|o| o == a) {
                out.push(a.to_string());
            }
        }
        out
    }
}

/// 为什么跳过一个号（换号记录与「切换」结果里的原因码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum Unready {
    /// 拿不到这个号能用的登录（没登录 · 续期失败 · 读不出账号身份）。
    NeedsLogin,
    /// 按量号在这台的 key 表里没有 key。
    NeedsKey,
    /// 这一发的请求体里账号身份那一格认不准，换不了。
    UnsureBody,
}

/// 为什么换（或为什么没换成）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum SwitchWhy {
    /// 原号被拒（满了）；`w` 卡住的那个窗口的语义位（`5h` / `7d`），说不出 ⇒ 缺。
    Full {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        #[cfg_attr(test, ts(optional))]
        w: Option<String>,
    },
    /// 原号用量到了阈值。
    Threshold { n: u8 },
    /// 用户「现在就换」，不重启。
    ManualHot,
    /// 用户「现在就换」，重启。
    ManualRestart,
    /// 轮到这个号时跳过了它。
    Skipped { account: String, reason: Unready },
    /// 订阅号都满了，留在原号的付费超额上。
    ToOverage,
}

/// 一条换号记录。`from == to` 的是没换成的那几种（跳过 · 留在超额）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct SwitchRecord {
    #[cfg_attr(test, ts(type = "number"))]
    pub at: u64,
    pub from: String,
    pub to: String,
    pub why: SwitchWhy,
    /// 那一刻原号几点重置（知道才有；记下就不随后来的数变）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub from_resets_at: Option<u64>,
}

/// 能不能不重启换号（会话一级的原因码；目标号接不上另在「切换」结果里说）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum InPlace {
    Ok,
    /// 这台的中转没见过这个会话的请求（没走中转）。
    NoRelay,
    /// 这台没建账号库。
    MachineNotMulti,
    /// 这一家没有可换的账号。
    AgentHasNoAccounts,
}

/// 一个号 ＋ 一个时刻。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct AccountAt {
    pub account: String,
    #[cfg_attr(test, ts(type = "number"))]
    pub at: u64,
}

/// 这个会话发不出去了：轮换里没有能接的号；`earliest` ＝ 最早回来的那个（说不出 ⇒ 缺）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct Blocked {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub earliest: Option<AccountAt>,
}

/// 会话的「账号」格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct AccountCell {
    /// 起这个会话进程的那个号（`_` ＝ 起会话时没说是哪个号，原样）。
    pub start: String,
    /// 此刻走的号。
    pub current: String,
    #[cfg_attr(test, ts(type = "number"))]
    pub since: u64,
    /// 换号记录，先的在前。
    pub history: Vec<SwitchRecord>,
    pub in_place: InPlace,
}

/// `rotation-session-read` 里一个会话的那一份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct SessionRotation {
    pub agent: String,
    /// 跟随这台的默认轮换。
    pub follow: bool,
    /// 这个会话自己那一份（跟随时也留着）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub custom: Option<Rotation>,
    pub account: AccountCell,
    /// 此刻的号触发了会换到谁；没有 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub next: Option<String>,
    /// 发不出去了（被拒、轮换里没有能接的）；能发 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub blocked: Option<Blocked>,
    /// 这台可用、不在这个会话轮换里的按量号；没有 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub fallback_api: Option<String>,
}

/// 一个会话在这台查得到吗：查得到 ⇒ 那一份；中转没见过 ⇒ 只说能不能不重启换。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum SessionRotationState {
    Present(Box<SessionRotation>),
    #[serde(rename_all = "camelCase")]
    Absent {
        in_place: InPlace,
    },
}

/// 「切换」里一个会话的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum SwitchOutcome {
    #[serde(rename = "done")]
    Switched,
    /// 没动它（不重启换不成立：`noRelay` · `machineNotMulti` · `agentHasNoAccounts`）。
    Skipped { code: String },
    /// 动了没成（`targetNeedsLogin` · `targetNeedsKey` · 重启换那一路的失败码原样）。
    #[serde(rename = "failed")]
    NotSwitched { code: String },
}

/// 一个会话在这台的轮换状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionEntry {
    /// 路由第 1 段（哪一家）。
    pub(crate) agent: String,
    /// 起这个会话进程的那个号（路由第 2 段）。
    pub(crate) start: String,
    /// 此刻走的号。
    pub(crate) current: String,
    /// 从什么时候起走 `current`。
    pub(crate) since: u64,
    /// 跟随这台的默认轮换。
    pub(crate) follow: bool,
    /// 这个会话自己那一份（切回跟随时留着）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) custom: Option<Rotation>,
    #[serde(default)]
    pub(crate) history: Vec<SwitchRecord>,
}

impl SessionEntry {
    pub(crate) fn fresh(agent: &str, start: &str, now: u64) -> Self {
        Self {
            agent: agent.to_string(),
            start: start.to_string(),
            current: start.to_string(),
            since: now,
            follow: true,
            custom: None,
            history: Vec::new(),
        }
    }
}

/// 整份 `rotation.json`。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Book {
    /// 这台的默认轮换；`None` ＝ 没动过（[`Rotation::default`]）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) default: Option<Rotation>,
    #[serde(default)]
    pub(crate) sessions: BTreeMap<String, SessionEntry>,
}

impl Book {
    pub(crate) fn default_rotation(&self) -> Rotation {
        self.default.clone().unwrap_or_default()
    }

    /// 这个会话此刻按哪一份轮换。
    pub(crate) fn rotation_of(&self, s: &SessionEntry) -> Rotation {
        match (&s.custom, s.follow) {
            (Some(c), false) => c.clone(),
            _ => self.default_rotation(),
        }
    }

    /// 中转第一次看见这个会话 / 会话换了起它的号 ⇒ 记下（换了起它的号 ⇒ 钉号清掉，从新号起算）。改了 ⇒ `true`。
    pub(crate) fn saw(&mut self, sid: &str, agent: &str, start: &str, now: u64) -> bool {
        match self.sessions.get_mut(sid) {
            None => {
                self.sessions
                    .insert(sid.to_string(), SessionEntry::fresh(agent, start, now));
                true
            }
            Some(s) if s.start != start || s.agent != agent => {
                s.agent = agent.to_string();
                s.start = start.to_string();
                s.current = start.to_string();
                s.since = now;
                true
            }
            Some(_) => false,
        }
    }

    /// 钉到 `to`，记一条（以及轮到时跳过的那几个）。
    pub(crate) fn pin(&mut self, sid: &str, rec: SwitchRecord, skipped: &[(String, Unready)]) {
        let Some(s) = self.sessions.get_mut(sid) else {
            return;
        };
        for (account, reason) in skipped {
            note(
                s,
                SwitchRecord {
                    at: rec.at,
                    from: rec.from.clone(),
                    to: rec.from.clone(),
                    why: SwitchWhy::Skipped {
                        account: account.clone(),
                        reason: *reason,
                    },
                    from_resets_at: None,
                },
            );
        }
        if rec.to != s.current {
            s.current = rec.to.clone();
            s.since = rec.at;
        }
        push(s, rec);
    }

    /// 没换成的那几种（跳过 · 留在超额）：自上一次真换号以来同一句已经记过 ⇒ 不再记。改了 ⇒ `true`。
    pub(crate) fn note_stuck(
        &mut self,
        sid: &str,
        rec: SwitchRecord,
        skipped: &[(String, Unready)],
    ) -> bool {
        let Some(s) = self.sessions.get_mut(sid) else {
            return false;
        };
        let mut changed = false;
        for (account, reason) in skipped {
            changed |= note(
                s,
                SwitchRecord {
                    at: rec.at,
                    from: rec.from.clone(),
                    to: rec.from.clone(),
                    why: SwitchWhy::Skipped {
                        account: account.clone(),
                        reason: *reason,
                    },
                    from_resets_at: None,
                },
            );
        }
        if rec.why == SwitchWhy::ToOverage {
            changed |= note(s, rec);
        }
        changed
    }
}

fn push(s: &mut SessionEntry, rec: SwitchRecord) {
    s.history.push(rec);
    let over = s.history.len().saturating_sub(HISTORY_KEPT);
    s.history.drain(..over);
}

/// 记一条「没换成」的：自上一次真换号以来已有同一句 ⇒ 不记。
fn note(s: &mut SessionEntry, rec: SwitchRecord) -> bool {
    let dup = s
        .history
        .iter()
        .rev()
        .take_while(|h| h.from == h.to)
        .any(|h| h.from == rec.from && h.why == rec.why);
    if !dup {
        push(s, rec);
    }
    !dup
}

// ── 落盘 ─────────────────────────────────────────────────────────────────

/// 这台的落点：`<家>/rotation.json`（家同额度账）。
pub(crate) fn path_from(get: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    super::ledger::path_from(get).map(|p| p.with_file_name(FILE_NAME))
}

pub(crate) fn path_now() -> Option<PathBuf> {
    path_from(&|k| std::env::var(k).ok())
}

/// 读一次盘。三态：没有（没动过）/ 读得懂 / 读不懂（不覆盖）。
#[derive(Debug)]
pub(crate) enum Read {
    Absent,
    Present(Book),
    Unreadable(String),
}

pub(crate) fn read_at(path: &Path) -> Read {
    match std::fs::read_to_string(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Read::Absent,
        Err(e) => Read::Unreadable(e.to_string()),
        Ok(s) => match serde_json::from_str::<Book>(&s) {
            Ok(b) => Read::Present(b),
            Err(e) => Read::Unreadable(e.to_string()),
        },
    }
}

type Stamp = (std::time::SystemTime, u64);

fn stamp(p: &Path) -> Option<Stamp> {
    let m = std::fs::metadata(p).ok()?;
    Some((m.modified().ok()?, m.len()))
}

/// 盘上那一份 ＋ 一份缓存（盘上的戳变了才重读）。`path = None` ⇒ 家推不出来，只在内存里。
pub(crate) struct RotationStore {
    path: Option<PathBuf>,
    cache: Mutex<(Option<Stamp>, Book)>,
}

impl RotationStore {
    pub(crate) fn at(path: Option<PathBuf>) -> Self {
        Self {
            path,
            cache: Mutex::new((None, Book::default())),
        }
    }

    pub(crate) fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// 此刻那一份（盘上动过就重读；读不懂 ⇒ 当作缺省，不轮换）。
    pub(crate) fn now(&self) -> Book {
        let mut g = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let Some(p) = self.path.as_deref() else {
            return g.1.clone();
        };
        let s = stamp(p);
        if s.is_none() || s != g.0 {
            g.1 = match read_at(p) {
                Read::Present(b) => b,
                Read::Absent | Read::Unreadable(_) => Book::default(),
            };
            g.0 = s;
        }
        g.1.clone()
    }

    /// 在跨进程锁里读盘 → 改 → 原子写回 → 缓存跟上；改到的会话各响一下（[`changes`]）。读不懂的那一份不覆盖。
    /// 外面只经两扇门进来：中转那一路（[`relay_change`]）与帧面那一路（[`face_change`]）。
    fn change<R>(&self, f: impl FnOnce(&mut Book) -> R) -> Result<R, String> {
        let mut g = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let (r, before, after) = match self.path.as_deref() {
            None => {
                let before = g.1.clone();
                let r = f(&mut g.1);
                (r, before, g.1.clone())
            }
            Some(path) => {
                let (r, before, after) = write_locked(path, f)?;
                g.0 = stamp(path);
                g.1 = after.clone();
                (r, before, after)
            }
        };
        drop(g);
        ring_changed(&before, &after);
        Ok(r)
    }
}

/// 中转那一路的写口（上游选择换号：第一次看见会话 · 钉号 · 记一条）。
pub(crate) fn relay_change<R>(
    store: &RotationStore,
    f: impl FnOnce(&mut Book) -> R,
) -> Result<R, String> {
    store.change(f)
}

/// 帧面那一路的写口（改默认轮换 · 改会话轮换 · 现在就换）。
pub(crate) fn face_change<R>(
    store: &RotationStore,
    f: impl FnOnce(&mut Book) -> R,
) -> Result<R, String> {
    store.change(f)
}

fn ring_changed(before: &Book, after: &Book) {
    let default_moved = before.default != after.default;
    for (sid, s) in &after.sessions {
        let moved = before.sessions.get(sid) != Some(s) || (default_moved && s.follow);
        if moved {
            let _ = changes().send(sid.clone());
        }
    }
}

/// 进程里那条「某个会话的轮换 / 账号格变了」的通道（流连接订它推 `rotation_changed`）。
pub(crate) fn changes() -> &'static tokio::sync::broadcast::Sender<String> {
    static TX: std::sync::OnceLock<tokio::sync::broadcast::Sender<String>> =
        std::sync::OnceLock::new();
    TX.get_or_init(|| tokio::sync::broadcast::channel::<String>(256).0)
}

fn write_locked<R>(path: &Path, f: impl FnOnce(&mut Book) -> R) -> Result<(R, Book, Book), String> {
    use std::io::Write as _;
    let shown = path.display().to_string();
    let failed = |e: &dyn std::fmt::Display| {
        copy_text(
            "beRotation.write.failed",
            &[("path", &shown), ("e", &e.to_string())],
        )
    };
    let dir = path
        .parent()
        .ok_or_else(|| copy_text("beRotation.write.noParent", &[("path", &shown)]))?;
    crate::common::own_dir::ensure_private_dir(dir).map_err(|e| failed(&e))?;
    let _lock = crate::platform::lock::hold(dir)?;
    let before = match read_at(path) {
        Read::Absent => Book::default(),
        Read::Present(b) => b,
        Read::Unreadable(e) => {
            return Err(copy_text(
                "beRotation.write.unreadable",
                &[("path", &shown), ("e", &e)],
            ))
        }
    };
    let mut after = before.clone();
    let r = f(&mut after);
    if after == before {
        return Ok((r, before, after));
    }
    let body = serde_json::to_string(&after).map_err(|e| failed(&e))?;
    let tmp = dir.join(format!(
        "{FILE_NAME}.{}.{:?}.tmp",
        std::process::id(),
        std::thread::current().id()
    ));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| failed(&e))?;
        file.write_all(body.as_bytes())
            .and_then(|()| file.write_all(b"\n"))
            .and_then(|()| file.sync_all())
            .map_err(|e| failed(&e))?;
        drop(file);
        std::fs::rename(&tmp, path).map_err(|e| failed(&e))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result.map(|()| (r, before, after))
}

// ── 线上那一份轮换的读法（整份收、不合法整份拒并说哪一格） ─────────────────

/// 读一份轮换：键恰好 `order` · `enabled` · `when`。`start_slots` ＝ 起始账号占位该有几个（默认恰好 1；会话自己那份 0 或 1）。
/// `account_ok(号)` 判这一格当得了轮换里的号；`is_api(号)` 判按量号；`prior` 是改之前那一份：**新勾上的按量号挪到 `order` 末尾**（订阅号用完才轮到它）。
/// 不合法 ⇒ `Err(哪一格、为什么)`（英文诊断，不进文案表）。
pub(crate) fn rotation_from(
    v: &Value,
    start_slots: std::ops::RangeInclusive<usize>,
    account_ok: &dyn Fn(&str) -> bool,
    is_api: &dyn Fn(&str) -> bool,
    prior: Option<&Rotation>,
) -> Result<Rotation, String> {
    let o = v
        .as_object()
        .ok_or("rotation must be an object {order, enabled, when}")?;
    if let Some(k) = o
        .keys()
        .find(|k| !matches!(k.as_str(), "order" | "enabled" | "when"))
    {
        return Err(format!("unknown field `{k}`"));
    }
    let order_v = o
        .get("order")
        .and_then(Value::as_array)
        .ok_or("`order` must be an array")?;
    let mut order: Vec<RotationSlot> = Vec::new();
    let mut starts = 0usize;
    for (i, item) in order_v.iter().enumerate() {
        let slot = match item {
            Value::String(a) if account_ok(a) => RotationSlot::Named(a.clone()),
            Value::Object(m) if m.len() == 1 && m.get("start") == Some(&Value::Bool(true)) => {
                starts += 1;
                RotationSlot::Start(StartSlot { start: true })
            }
            _ => {
                return Err(format!(
                    "`order[{i}]` must be an account id or {{\"start\":true}}"
                ))
            }
        };
        if order.contains(&slot) && matches!(slot, RotationSlot::Named(_)) {
            return Err(format!("`order[{i}]` repeats an account"));
        }
        order.push(slot);
    }
    if !start_slots.contains(&starts) {
        return Err(format!(
            "`order` must hold {}..={} {{\"start\":true}} slot(s), got {starts}",
            start_slots.start(),
            start_slots.end()
        ));
    }
    let named = |a: &str| {
        order
            .iter()
            .any(|s| s == &RotationSlot::Named(a.to_string()))
    };
    let enabled_v = o
        .get("enabled")
        .and_then(Value::as_array)
        .ok_or("`enabled` must be an array")?;
    let mut enabled: Vec<String> = Vec::new();
    for (i, item) in enabled_v.iter().enumerate() {
        let a = item
            .as_str()
            .filter(|a| named(a))
            .ok_or_else(|| format!("`enabled[{i}]` must name an account listed in `order`"))?;
        if enabled.iter().any(|e| e == a) {
            return Err(format!("`enabled[{i}]` repeats an account"));
        }
        enabled.push(a.to_string());
    }
    let when = match o.get("when") {
        Some(Value::String(s)) if s == "full" => RotationWhen::Full,
        Some(Value::Object(m)) if m.len() == 1 && m.contains_key("threshold") => {
            let n = m["threshold"]
                .as_object()
                .filter(|t| t.len() == 1)
                .and_then(|t| t.get("n"))
                .and_then(Value::as_u64)
                .and_then(|n| u8::try_from(n).ok())
                .filter(|n| THRESHOLD_RANGE.contains(n))
                .ok_or("`when.threshold.n` must be an integer 50..=99")?;
            RotationWhen::Threshold { n }
        }
        _ => return Err("`when` must be \"full\" or {\"threshold\":{\"n\":50..=99}}".into()),
    };
    let was = |a: &str| prior.is_some_and(|p| p.enabled.iter().any(|e| e == a));
    let newly: Vec<RotationSlot> = enabled
        .iter()
        .filter(|a| is_api(a) && !was(a))
        .map(|a| RotationSlot::Named(a.clone()))
        .collect();
    order.retain(|s| !newly.contains(s));
    order.extend(newly);
    Ok(Rotation {
        order,
        enabled,
        when,
    })
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/rotation_tests.rs"]
mod tests;
