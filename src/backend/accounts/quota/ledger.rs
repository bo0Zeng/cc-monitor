//! 额度账：这台每个号最近一次看到的额度快照 ＋ 看到的时刻，落 `~/.cc-monitor/quota.json`。
//!
//! - 同一种事实两个来源：中转经手的回包头（上游选择的 `observe` 交到 [`record_seen`]，来源 `headers`）· 官方客户端报的用量
//!   （帧命令 `quota-probe` 交到 [`record_probe`]，来源 `usage`）。两者记进同一条账：**按窗口合并**（同名窗口新的盖旧的，
//!   别的窗口留着），每个窗口记自己几点、从哪看到的；状态 · 被拒 · 卡在哪 · 超额那几格只有回包头说得出，只随它变。
//! - 两个写者（常驻里的中转 · 一次性的 CLI）可能在两个进程里：「读盘 → 合并 → 原子替换」只在 [`write_merged`] 一处、在跨进程锁里做，
//!   谁也冲不掉谁的窗口。常驻那份内存账在用的那一刻比一次盘上的戳（不定时）：别人写过就读进来合并。
//! - 「变了」只看显示得出来的几格（各窗口取整的百分比 · 重置时刻 · 状态 · 被拒 · 卡在哪 · 超额）：变了才推一帧、才立刻落盘；
//!   没变的观测只刷新内存里的时刻，盘上那份的时刻至多落后 [`PERSIST_EVERY`] 秒。
//! - 很久没流量的号照实只有「最后一次看到是几点」，不编。

use crate::agents::QuotaReading;
use crate::common::said::Said;
use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub(crate) const FILE_NAME: &str =
    relay_route_core::file_name_of(relay_route_core::QUOTA_LEDGER_REL);

/// 显示没变时，盘上那份「看到的时刻」至多落后这么多秒。
pub(crate) const PERSIST_EVERY: u64 = 60;

/// 一个号的一条账。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Observed {
    /// 路由第 1 段（哪一家）。
    pub(crate) agent: String,
    /// 路由第 2 段（哪个号；`_` ＝ 起会话时没说是哪个号）。
    pub(crate) account: String,
    /// 最后一次看到这个号（任何一个来源）的时刻（unix 秒）。
    pub(crate) seen_at: u64,
    pub(crate) reading: QuotaReading,
    /// 各窗口最后一次几点、从哪看到的（窗口名 → …）；没列的窗口 ＝ 与 `seen_at` 同一刻、来自回包头。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) windows_seen: BTreeMap<String, WindowSeen>,
    /// 各窗口用量的走势（窗口名 → …）：估「几点到」只按它（[`Observed::eta`]）。只在账上，不上 `quota-read`。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) trend: BTreeMap<String, Trend>,
}

/// 一个窗口用量的走势：此刻这个值头一次看到的时刻 ＋ 同一窗口里上一个**不同**的值（又看到一次同一个值不算新采样：说不出涨多快）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct Trend {
    pub(crate) since: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) prev: Option<Sample>,
}

/// 一次采样：用量（比例）与它头一次看到的时刻。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct Sample {
    pub(crate) used: f64,
    pub(crate) since: u64,
}

/// 估「几点到」只看最近这么多秒里头一次看到的值（再早的涨法不作数）。
pub(crate) const EST_FRESH: u64 = 30 * 60;

/// 两个读数的重置时刻差在这么多秒以内 ⇒ 同一个窗口（来源不同时重置时刻可能差几秒）。
const SAME_WINDOW_SLACK: u64 = 300;

fn same_window(a: &crate::agents::QuotaWindow, b: &crate::agents::QuotaWindow) -> bool {
    matches!((a.resets_at, b.resets_at), (Some(x), Some(y)) if x.abs_diff(y) <= SAME_WINDOW_SLACK)
}

/// 新读数 `new`（`at` 看到的）进来之后这个窗口的走势：值没变 ⇒ 照旧；同一窗口里值变了 ⇒ 旧值成了上一次；
/// 窗口换了 · 头一次看到 · 说不出用量 ⇒ 从这一刻重新记。
fn step(
    old: Option<(&crate::agents::QuotaWindow, Option<Trend>, u64)>,
    new: &crate::agents::QuotaWindow,
    at: u64,
) -> Trend {
    let fresh = Trend {
        since: at,
        prev: None,
    };
    let Some((o, t, old_at)) = old else {
        return fresh;
    };
    let since = t.map_or(old_at, |t| t.since);
    match (o.used, new.used) {
        (Some(u0), Some(u1)) if same_window(o, new) => {
            if (u0 - u1).abs() < 1e-9 {
                t.unwrap_or(Trend { since, prev: None })
            } else {
                Trend {
                    since: at,
                    prev: Some(Sample { used: u0, since }),
                }
            }
        }
        _ => fresh,
    }
}

/// 一个窗口的数从哪来。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "QuotaSource"
    )
)]
pub enum Source {
    /// 中转经手的回包头。
    Headers,
    /// 官方客户端报的用量（`quota-probe`）。
    Usage,
}

/// 一个窗口几点、从哪看到的。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WindowSeen {
    pub(crate) at: u64,
    pub(crate) from: Source,
}

impl Observed {
    /// ★ 按走势估这个窗口几点用到 `target`（比例）：同一窗口有上一次不同的采样 · 此刻这个值是最近 [`EST_FRESH`] 秒里头一次看到的 ·
    /// 在涨 ⇒ 按这两点的斜率外推。只有一个采样 · 涨在那之前 · 不涨 · 已经到了 · 到之前窗口先重置 · 外推的时刻已过 ⇒ `None`（没根据就不给）。
    pub(crate) fn eta(&self, name: &str, target: f64, now: u64) -> Option<u64> {
        let w = self.reading.windows.iter().find(|w| w.name == name)?;
        let u1 = w.used?;
        let t = self.trend.get(name)?;
        let p = t.prev?;
        if now.saturating_sub(t.since) > EST_FRESH
            || u1 <= p.used
            || t.since <= p.since
            || u1 >= target
        {
            return None;
        }
        let per_sec = (u1 - p.used) / (t.since - p.since) as f64;
        let at = t.since + ((target - u1) / per_sec).round() as u64;
        (at > now && w.resets_at.is_none_or(|r| at < r)).then_some(at)
    }

    /// 一个窗口几点、从哪看到的（没列 ⇒ 这一条的时刻、回包头）。
    pub(crate) fn window_seen(&self, name: &str) -> WindowSeen {
        self.windows_seen.get(name).copied().unwrap_or(WindowSeen {
            at: self.seen_at,
            from: Source::Headers,
        })
    }
}

/// ★ 合并的唯一判法：`add` 的窗口逐个并进 `into`（同名窗口看到得晚的那份留下，一样晚 ⇒ `add` 的）；
/// `headers` ⇒ 状态 · 被拒 · 卡在哪 · 超额那几格也换成 `add` 的（只有回包头说得出它们）。
pub(crate) fn merge(into: &mut Observed, add: &Observed, headers: bool) {
    let mut meta: BTreeMap<String, WindowSeen> = into
        .reading
        .windows
        .iter()
        .map(|w| (w.name.clone(), into.window_seen(&w.name)))
        .collect();
    for w in &add.reading.windows {
        let m = add.window_seen(&w.name);
        // 带着走势来的（盘上那份 · 内存里那份）照它的；只有一发读数的照旧值推一步。
        let trend = |old: Option<(&crate::agents::QuotaWindow, Option<Trend>, u64)>| {
            add.trend
                .get(&w.name)
                .copied()
                .unwrap_or_else(|| step(old, w, m.at))
        };
        match into.reading.windows.iter_mut().find(|x| x.name == w.name) {
            Some(_) if meta.get(&w.name).is_some_and(|old| old.at > m.at) => {}
            Some(x) => {
                let old_at = meta.get(&w.name).map_or(into.seen_at, |o| o.at);
                let t = trend(Some((x, into.trend.get(&w.name).copied(), old_at)));
                into.trend.insert(w.name.clone(), t);
                *x = w.clone();
                meta.insert(w.name.clone(), m);
            }
            None => {
                into.trend.insert(w.name.clone(), trend(None));
                into.reading.windows.push(w.clone());
                meta.insert(w.name.clone(), m);
            }
        }
    }
    if headers {
        let r = &add.reading;
        into.reading.status = r.status;
        into.reading.refused = r.refused;
        into.reading.limiting.clone_from(&r.limiting);
        into.reading.resets_at = r.resets_at;
        into.reading.overage.clone_from(&r.overage);
    }
    into.seen_at = into.seen_at.max(add.seen_at);
    let at = into.seen_at;
    into.windows_seen = meta
        .into_iter()
        .filter(|(_, m)| !(m.at == at && m.from == Source::Headers))
        .collect();
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Book {
    pub(crate) accounts: Vec<Observed>,
}

type Key = (String, String);

/// 显示得出来的那几格（判「变了没」用）。
fn shown(r: &QuotaReading) -> String {
    let wins: Vec<String> = r
        .windows
        .iter()
        .map(|w| {
            format!(
                "{}:{:?}:{:?}",
                w.name,
                w.used.map(|u| (u * 100.0).round() as i64),
                w.resets_at
            )
        })
        .collect();
    format!(
        "{:?}|{}|{:?}|{:?}|{}|{:?}",
        r.status,
        r.refused,
        r.limiting,
        r.resets_at,
        wins.join(","),
        r.overage
            .as_ref()
            .map(|o| (o.status, o.in_use, &o.disabled))
    )
}

struct State {
    mem: BTreeMap<Key, Observed>,
    /// 盘上那一份每个号的 `seenAt`（判要不要落盘）。
    on_disk: BTreeMap<Key, u64>,
    /// 上一次读 / 写之后盘上那份文件的戳：变了 ＝ 别人（另一个进程）写过。
    stamp: Option<Stamp>,
}

/// 盘上那份文件的戳（修改时刻 · 长度 · 节点号）：原子替换每次换一个新节点，同一纳秒里写两次也分得出。
type Stamp = (std::time::SystemTime, u64, u64);

fn stamp(p: &Path) -> Option<Stamp> {
    let m = std::fs::metadata(p).ok()?;
    #[cfg(unix)]
    let node = std::os::unix::fs::MetadataExt::ino(&m);
    #[cfg(not(unix))]
    let node = 0;
    Some((m.modified().ok()?, m.len(), node))
}

/// 额度账本体。`path = None` ⇒ 家推不出来，只记在内存。
pub(crate) struct Ledger {
    path: Option<PathBuf>,
    state: Mutex<State>,
    bell: Option<Arc<tokio::sync::watch::Sender<u64>>>,
}

impl Ledger {
    /// 在这个落点开账：盘上已有的读进内存（重启后被拒的号仍记得几点重置）。
    pub(crate) fn at(path: Option<PathBuf>) -> Self {
        let mut mem = BTreeMap::new();
        if let Some(Read::Present(book)) = path.as_deref().map(read_at) {
            for e in book.accounts {
                mem.insert((e.agent.clone(), e.account.clone()), e);
            }
        }
        let on_disk = mem.iter().map(|(k, e)| (k.clone(), e.seen_at)).collect();
        let stamp = path.as_deref().and_then(stamp);
        Self {
            path,
            state: Mutex::new(State {
                mem,
                on_disk,
                stamp,
            }),
            bell: None,
        }
    }

    /// 用的那一刻比一次盘上的戳：别人写过 ⇒ 读进来、按窗口并进内存（内存里中转那几格照旧 —— 回包头只有中转这一个写者）。
    fn refresh(&self, g: &mut State) {
        let Some(p) = self.path.as_deref() else {
            return;
        };
        let s = stamp(p);
        if s.is_none() || s == g.stamp {
            return;
        }
        g.stamp = s;
        let Read::Present(book) = read_at(p) else {
            return;
        };
        for e in book.accounts {
            let key = (e.agent.clone(), e.account.clone());
            g.on_disk.insert(key.clone(), e.seen_at);
            match g.mem.get_mut(&key) {
                Some(m) => merge(m, &e, false),
                None => {
                    g.mem.insert(key, e);
                }
            }
        }
    }

    /// 显示变了就在这个通道上加一（订阅者据此推帧）。
    pub(crate) fn ringing(mut self, bell: Arc<tokio::sync::watch::Sender<u64>>) -> Self {
        self.bell = Some(bell);
        self
    }

    /// 那个号的最近一条（盘上别人写过 ⇒ 先并进来）。
    pub(crate) fn entry(&self, agent: &str, account: &str) -> Option<Observed> {
        let mut g = self.state.lock().unwrap_or_else(|e| e.into_inner());
        self.refresh(&mut g);
        g.mem
            .get(&(agent.to_string(), account.to_string()))
            .cloned()
    }

    fn ring(&self) {
        if let Some(t) = &self.bell {
            t.send_modify(|n| *n = n.wrapping_add(1));
        }
    }

    /// 落了盘之后：戳跟上；内存那一条换成盘上合并好的那份（内存里那条没更新过才换）。
    fn landed(&self, written: Observed, at: Option<Stamp>) {
        let mut g = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if at.is_some() {
            g.stamp = at;
        }
        let key = (written.agent.clone(), written.account.clone());
        g.on_disk.insert(key.clone(), written.seen_at);
        if g.mem.get(&key).is_none_or(|m| m.seen_at <= written.seen_at) {
            g.mem.insert(key, written);
        }
    }

    pub(crate) fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

/// 进程里那条「额度账显示变了」的通道（流连接订它推 `quota_changed`）。
pub(crate) fn bell() -> Arc<tokio::sync::watch::Sender<u64>> {
    static BELL: std::sync::OnceLock<Arc<tokio::sync::watch::Sender<u64>>> =
        std::sync::OnceLock::new();
    Arc::clone(BELL.get_or_init(|| Arc::new(tokio::sync::watch::channel::<u64>(0).0)))
}

/// 这台的落点：`<家>/quota.json`（家与 API key 凭据同一个出处）。推不出 ⇒ `None`。
pub(crate) fn path_from(get: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let home = crate::platform::paths::home_dir_from(&|k| get(k).map(Into::into));
    creds_core::store::monitor_data_dir(get(creds_core::store::DATA_DIR_ENV).as_deref(), home)
        .map(|d| d.join(FILE_NAME))
}

/// 记一条观测（中转那一路的写口，来源 `headers`）。显示变了 ⇒ 推一下、立刻落盘；没变 ⇒ 盘上的时刻落后够久才落。
/// 落盘失败只出声，不影响这一发（额度账不是请求的前提）。
pub(crate) fn record_seen(
    ledger: &Ledger,
    agent: &str,
    account: &str,
    reading: QuotaReading,
    now: u64,
) {
    let key = (agent.to_string(), account.to_string());
    let heard = Observed {
        agent: key.0.clone(),
        account: key.1.clone(),
        seen_at: now,
        reading,
        windows_seen: BTreeMap::new(),
        trend: BTreeMap::new(),
    };
    let (changed, persist, entry) = {
        let mut g = ledger.state.lock().unwrap_or_else(|e| e.into_inner());
        ledger.refresh(&mut g);
        let old = g.mem.get(&key).cloned();
        let entry = match &old {
            Some(o) => {
                let mut m = o.clone();
                merge(&mut m, &heard, true);
                m
            }
            None => {
                let mut h = heard.clone();
                merge_trend_fresh(&mut h);
                h
            }
        };
        let changed = old.is_none_or(|o| shown(&o.reading) != shown(&entry.reading));
        g.mem.insert(key.clone(), entry.clone());
        let stale = g
            .on_disk
            .get(&key)
            .is_none_or(|at| now.saturating_sub(*at) >= PERSIST_EVERY);
        let persist = ledger.path.is_some() && (changed || stale);
        if persist {
            g.on_disk.insert(key, now);
        }
        (changed, persist, entry)
    };
    if persist {
        if let Some(p) = ledger.path.as_deref() {
            match write_merged(p, &entry, true) {
                Ok((written, at)) => ledger.landed(written, at),
                Err(e) => tracing::warn!("[quota] {e}: {}", e.raw.as_deref().unwrap_or("")),
            }
        }
    }
    if changed {
        ledger.ring();
    }
}

/// 记一次官方客户端报的用量（`quota-probe` 那一路的写口，来源 `usage`）：只并窗口，不碰回包头那几格；一定落盘（落不了 ⇒ `Err`）。
/// 交回合并好的那一条。
pub(crate) fn record_probe(
    ledger: &Ledger,
    agent: &str,
    account: &str,
    windows: Vec<crate::agents::QuotaWindow>,
    now: u64,
) -> Result<Observed, Said> {
    let path = ledger
        .path
        .as_deref()
        .ok_or_else(|| Said::from(copy_text("beQuotaLedger.read.noHome", &[])))?;
    let windows_seen = windows
        .iter()
        .map(|w| {
            (
                w.name.clone(),
                WindowSeen {
                    at: now,
                    from: Source::Usage,
                },
            )
        })
        .collect();
    let probed = Observed {
        agent: agent.to_string(),
        account: account.to_string(),
        seen_at: now,
        reading: QuotaReading {
            status: None,
            refused: false,
            limiting: None,
            resets_at: None,
            windows,
            overage: None,
        },
        windows_seen,
        trend: BTreeMap::new(),
    };
    let before = ledger.entry(agent, account);
    let (written, at) = write_merged(path, &probed, false)?;
    let changed = before.is_none_or(|b| shown(&b.reading) != shown(&written.reading));
    ledger.landed(written.clone(), at);
    if changed {
        ledger.ring();
    }
    Ok(written)
}

/// 读一次盘。三态：没有（还没看到过）/ 读得懂 / 读不懂（不覆盖）。
pub(crate) type Read = crate::common::own_state::Read<Book>;

/// 读盘的上限（一台的号数有限，远到不了）。
const MAX_BYTES: u64 = 16 << 20;

pub(crate) fn read_at(path: &Path) -> Read {
    crate::common::own_state::read_json(path, MAX_BYTES)
}

/// ★ 盘上那份的唯一写法：在跨进程锁里读盘 → 那个号那一条按窗口并进 `add`（[`merge`]；`headers` 同它）→ `O_EXCL` 临时文件 →
/// 写满 → 原子挪过去 → 记下新的戳。读不懂的那份不覆盖。交回落盘的那一条与戳。
fn write_merged(
    path: &Path,
    add: &Observed,
    headers: bool,
) -> Result<(Observed, Option<Stamp>), Said> {
    let dir = path.parent().ok_or_else(|| {
        Said::from(copy_text(
            "beQuotaLedger.write.noParent",
            &[("path", &path.display().to_string())],
        ))
    })?;
    crate::common::own_dir::ensure_private_dir(dir).map_err(|e| {
        Said::with_raw(
            copy_text(
                "beQuotaLedger.write.failed",
                &[
                    ("path", &path.display().to_string()),
                    ("why", &copy_core::io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    let _lock = crate::platform::lock::hold(dir)?;
    let mut book = match read_at(path) {
        Read::Absent => Book::default(),
        Read::Present(b) => b,
        // 读不懂的那份不覆盖：那一句说清是哪份、没覆盖，读不懂的原因（已带原因词）与原话照它的。
        Read::Unreadable(e) => {
            return Err(
                e.wrap(|said| copy_text("beQuotaLedger.write.unreadable", &[("said", said)]))
            )
        }
    };
    let entry = match book
        .accounts
        .iter()
        .position(|e| e.agent == add.agent && e.account == add.account)
    {
        Some(i) => {
            let mut e = book.accounts.remove(i);
            merge(&mut e, add, headers);
            e
        }
        None => {
            let mut a = add.clone();
            merge_trend_fresh(&mut a);
            a
        }
    };
    book.accounts.push(entry.clone());
    book.accounts
        .sort_by(|a, b| (&a.agent, &a.account).cmp(&(&b.agent, &b.account)));
    crate::common::own_state::write_json(path, &book)?;
    Ok((entry, stamp(path)))
}

/// 头一次看到的那一条：每个窗口从这一刻起记走势（已带着的照旧）。
fn merge_trend_fresh(o: &mut Observed) {
    for w in &o.reading.windows {
        let at = o.window_seen(&w.name).at;
        o.trend.entry(w.name.clone()).or_insert(Trend {
            since: at,
            prev: None,
        });
    }
}

/// 帧命令 `quota-read` 的底子：现读这台的额度账（不读内存 —— 一次性 CLI 那一形里内存是空的）；显示态由帧面宿主补上。
pub(crate) fn answer_of(path: Option<&Path>, now: u64) -> serde_json::Value {
    let (state, why, accounts) = match path.map(read_at) {
        None => (
            "unreadable",
            Some(Said::from(copy_text("beQuotaLedger.read.noHome", &[]))),
            vec![],
        ),
        Some(Read::Absent) => ("absent", None, vec![]),
        Some(Read::Present(b)) => ("present", None, b.accounts),
        Some(Read::Unreadable(e)) => ("unreadable", Some(e), vec![]),
    };
    let (reason, detail) = crate::stream::detail::unreadable("quota-read", why.as_ref());
    serde_json::json!({
        "state": state,
        "reason": reason,
        "detail": detail,
        "path": path.map(|p| p.display().to_string()),
        "now": now,
        // 走势只在账上（估「几点到」用），不上线。
        "accounts": accounts.into_iter().map(|mut o| { o.trend.clear(); o }).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/ledger_tests.rs"]
mod tests;
