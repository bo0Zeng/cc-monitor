//! 额度账：这台每个号最近一次从回包头看到的额度快照 ＋ 看到的时刻，落 `~/.cc-monitor/quota.json`。
//!
//! - 写者只有中转那一路（上游选择的 `observe` 把读好的快照交到 [`record_seen`]）；读者是帧命令 `quota-read`（现读盘）与轮换（内存里那一份）。
//! - 「变了」只看显示得出来的几格（各窗口取整的百分比 · 重置时刻 · 状态 · 被拒 · 卡在哪 · 超额）：变了才推一帧、才立刻落盘；
//!   没变的观测只刷新内存里的时刻，盘上那份的时刻至多落后 [`PERSIST_EVERY`] 秒。
//! - 很久没流量的号照实只有「最后一次看到是几点」，不编。

use crate::agents::QuotaReading;
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
    /// 看到这份快照的时刻（unix 秒）。
    pub(crate) seen_at: u64,
    pub(crate) reading: QuotaReading,
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
        Self {
            path,
            state: Mutex::new(State { mem, on_disk }),
            bell: None,
        }
    }

    /// 显示变了就在这个通道上加一（订阅者据此推帧）。
    pub(crate) fn ringing(mut self, bell: Arc<tokio::sync::watch::Sender<u64>>) -> Self {
        self.bell = Some(bell);
        self
    }

    /// 内存里那个号的最近一条。
    pub(crate) fn entry(&self, agent: &str, account: &str) -> Option<Observed> {
        let g = self.state.lock().unwrap_or_else(|e| e.into_inner());
        g.mem
            .get(&(agent.to_string(), account.to_string()))
            .cloned()
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

/// 记一条观测（中转那一路的写口）。显示变了 ⇒ 推一下、立刻落盘；没变 ⇒ 盘上的时刻落后够久才落。
/// 落盘失败只出声，不影响这一发（额度账不是请求的前提）。
pub(crate) fn record_seen(
    ledger: &Ledger,
    agent: &str,
    account: &str,
    reading: QuotaReading,
    now: u64,
) {
    let key = (agent.to_string(), account.to_string());
    let (changed, persist, entry) = {
        let mut g = ledger.state.lock().unwrap_or_else(|e| e.into_inner());
        let changed = g
            .mem
            .get(&key)
            .is_none_or(|old| shown(&old.reading) != shown(&reading));
        let entry = Observed {
            agent: key.0.clone(),
            account: key.1.clone(),
            seen_at: now,
            reading,
        };
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
            if let Err(e) = write_entry(p, entry) {
                tracing::warn!("[quota] {e}");
            }
        }
    }
    if changed {
        if let Some(t) = &ledger.bell {
            t.send_modify(|n| *n = n.wrapping_add(1));
        }
    }
}

/// 读一次盘。三态：没有（还没看到过）/ 读得懂 / 读不懂（不覆盖）。
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

/// 在跨进程锁里：读盘 → 换掉那一个号那一条 → `O_EXCL` 临时文件 → 写满 → 原子挪过去。读不懂的那份不覆盖。
fn write_entry(path: &Path, entry: Observed) -> Result<(), String> {
    use std::io::Write as _;
    let dir = path.parent().ok_or_else(|| {
        copy_text(
            "beQuotaLedger.write.noParent",
            &[("path", &path.display().to_string())],
        )
    })?;
    crate::common::own_dir::ensure_private_dir(dir).map_err(|e| {
        copy_text(
            "beQuotaLedger.write.failed",
            &[("path", &path.display().to_string()), ("e", &e.to_string())],
        )
    })?;
    let _lock = crate::platform::lock::hold(dir)?;
    let mut book = match read_at(path) {
        Read::Absent => Book::default(),
        Read::Present(b) => b,
        Read::Unreadable(e) => {
            return Err(copy_text(
                "beQuotaLedger.write.unreadable",
                &[("path", &path.display().to_string()), ("e", &e)],
            ))
        }
    };
    book.accounts
        .retain(|e| !(e.agent == entry.agent && e.account == entry.account));
    book.accounts.push(entry);
    book.accounts
        .sort_by(|a, b| (&a.agent, &a.account).cmp(&(&b.agent, &b.account)));
    let failed = |e: &dyn std::fmt::Display| {
        copy_text(
            "beQuotaLedger.write.failed",
            &[("path", &path.display().to_string()), ("e", &e.to_string())],
        )
    };
    let body = serde_json::to_string(&book).map_err(|e| failed(&e))?;
    let tmp = dir.join(format!(
        "{FILE_NAME}.{}.{:?}.tmp",
        std::process::id(),
        std::thread::current().id()
    ));
    let result = (|| {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| failed(&e))?;
        f.write_all(body.as_bytes())
            .and_then(|()| f.write_all(b"\n"))
            .and_then(|()| f.sync_all())
            .map_err(|e| failed(&e))?;
        drop(f);
        std::fs::rename(&tmp, path).map_err(|e| failed(&e))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// 帧命令 `quota-read`：现读这台的额度账（不读内存 —— 一次性 CLI 那一形里内存是空的）。
pub(crate) fn answer_read() -> serde_json::Value {
    let path = path_from(&|k| std::env::var(k).ok());
    answer_of(path.as_deref(), super::now_unix())
}

pub(crate) fn answer_of(path: Option<&Path>, now: u64) -> serde_json::Value {
    let (state, reason, accounts) = match path.map(read_at) {
        None => (
            "unreadable",
            Some(copy_text("beQuotaLedger.read.noHome", &[])),
            vec![],
        ),
        Some(Read::Absent) => ("absent", None, vec![]),
        Some(Read::Present(b)) => ("present", None, b.accounts),
        Some(Read::Unreadable(e)) => ("unreadable", Some(e), vec![]),
    };
    serde_json::json!({
        "state": state,
        "reason": reason,
        "path": path.map(|p| p.display().to_string()),
        "now": now,
        "accounts": accounts,
    })
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/ledger_tests.rs"]
mod tests;
