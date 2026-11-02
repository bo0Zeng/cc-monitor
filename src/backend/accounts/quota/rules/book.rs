//! `rotation.json` 第四版：每个会话在用哪个号 · 它载入的那份策略副本 · 会话侧的临时动作（拖出来的顺序 · 先不用的号）·
//! 策略上一判交的 `memo` 与叫醒 · 换号记录（[`Book::saw`] 记新会话 · [`Session::push_log`] 记换号）· 新会话默认载入哪份（`forNew`）。
//! 盘上副本按版本只存一份（顶层 `copies`，会话的 `uses` 只写键）；内存里各会话共用同一份（[`Arc`]）。
//! 版本那一格归读写口（声明 `relay_route_core::ROTATION_FILE`）。

/// 换号记录（盘上那一份 · `rotation-view` 带的同一形）：线上形住 `cmd-wire`。
pub(crate) use cmd_wire::rotation::{
    RotLog as Log, RotLogCause as Cause, RotSaw as Saw, RotSawAcct as SawAcct,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

/// 换号记录每个会话至多几条。
pub(crate) const LOG_KEPT: usize = 50;

/// 整份第四版（盘上那一形见 [`Disk`]）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "Disk", into = "Disk")]
pub(crate) struct Book {
    pub(crate) sessions: BTreeMap<String, Session>,
    /// 新会话载入哪份（策略目录里的文件名去掉 `.rhai`）；空 ⇒ 新会话不轮换，只用开会话的号。
    pub(crate) for_new: String,
}

/// 一个会话。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Session {
    pub(crate) agent: String,
    /// 起会话的号（路由第 2 段）。
    pub(crate) start: String,
    pub(crate) current: String,
    pub(crate) since: u64,
    pub(crate) seen: u64,
    /// 这个会话载入的那份策略（缺 ⇒ 不轮换）：载入之后文件再改也不跟，「载入新版」才换。
    pub(crate) uses: Option<Arc<Copy>>,
    /// 用户拖出来的「接下来先用谁」。
    pub(crate) temp: Vec<String>,
    /// 这个会话先不用的号（会话侧的临时动作：策略不变，会话没了就没了）。
    pub(crate) off: Vec<String>,
    /// 策略上一判交的，原样存、下一判原样交回。
    pub(crate) memo: Option<Value>,
    pub(crate) wake: Wake,
    pub(crate) log: Vec<Log>,
}

/// 盘上那一形：副本收在 `copies`（键 `<id>@<rev>`；同一键内容不同 ⇒ 后面那份键尾加 `~<n>`），会话的 `uses` 只写键。
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Disk {
    #[serde(default)]
    sessions: BTreeMap<String, DiskSession>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    copies: BTreeMap<String, Copy>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    for_new: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DiskSession {
    agent: String,
    start: String,
    current: String,
    since: u64,
    #[serde(default)]
    seen: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    uses: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    temp: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    off: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    memo: Option<Value>,
    #[serde(default, skip_serializing_if = "Wake::is_empty")]
    wake: Wake,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    log: Vec<Log>,
}

impl From<Book> for Disk {
    fn from(b: Book) -> Self {
        let mut copies: BTreeMap<String, Copy> = BTreeMap::new();
        let mut key_of = |c: &Arc<Copy>| -> String {
            let base = format!("{}@{}", c.id, c.rev);
            let mut n = 1;
            loop {
                let k = if n == 1 {
                    base.clone()
                } else {
                    format!("{base}~{n}")
                };
                match copies.get(&k) {
                    Some(have) if have == c.as_ref() => return k,
                    Some(_) => n += 1,
                    None => {
                        copies.insert(k.clone(), Copy::clone(c));
                        return k;
                    }
                }
            }
        };
        let sessions = b
            .sessions
            .into_iter()
            .map(|(sid, s)| {
                let uses = s.uses.as_ref().map(&mut key_of);
                (
                    sid,
                    DiskSession {
                        agent: s.agent,
                        start: s.start,
                        current: s.current,
                        since: s.since,
                        seen: s.seen,
                        uses,
                        temp: s.temp,
                        off: s.off,
                        memo: s.memo,
                        wake: s.wake,
                        log: s.log,
                    },
                )
            })
            .collect();
        Disk {
            sessions,
            copies,
            for_new: b.for_new,
        }
    }
}

impl TryFrom<Disk> for Book {
    type Error = String;

    /// 会话引用了 `copies` 里没有的那一份 ⇒ 读不懂；没有会话引用的那几份不留（下一次写就不在了）。
    fn try_from(d: Disk) -> Result<Self, String> {
        let copies: BTreeMap<String, Arc<Copy>> = d
            .copies
            .into_iter()
            .map(|(k, c)| (k, Arc::new(c)))
            .collect();
        let sessions = d
            .sessions
            .into_iter()
            .map(|(sid, s)| {
                let uses = match s.uses {
                    None => None,
                    Some(k) => Some(
                        copies
                            .get(&k)
                            .cloned()
                            .ok_or_else(|| format!("session {sid}: no copy {k}"))?,
                    ),
                };
                Ok((
                    sid,
                    Session {
                        agent: s.agent,
                        start: s.start,
                        current: s.current,
                        since: s.since,
                        seen: s.seen,
                        uses,
                        temp: s.temp,
                        off: s.off,
                        memo: s.memo,
                        wake: s.wake,
                        log: s.log,
                    },
                ))
            })
            .collect::<Result<_, String>>()?;
        Ok(Book {
            sessions,
            for_new: d.for_new,
        })
    }
}

/// 一份策略的副本：哪份文件 · 那时的内容版本 · 代码 · 设置 · 时区。`form` 原样存（本层不认，配置编辑层照它画）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Copy {
    pub(crate) id: String,
    pub(crate) rev: String,
    /// 代码那一段，前面垫着空行（报第几行时与文件对得上）。
    pub(crate) code: String,
    pub(crate) settings: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tz: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) time_varying: bool,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub(crate) form: Value,
}

/// 策略约的叫醒（上一判交的）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Wake {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) at: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) when: Vec<WakeWhen>,
}

impl Wake {
    pub(crate) fn is_empty(&self) -> bool {
        self.at.is_none() && self.when.is_empty()
    }
}

/// 某号某窗读数到多少再来问。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WakeWhen {
    pub(crate) acct: String,
    /// `5h` · `7d`。
    pub(crate) w: String,
    pub(crate) used_at_least: f64,
}

/// 「看见」的时刻隔多久才刷新一次（秒）。
pub(crate) const SEEN_REFRESH: u64 = 86_400;
/// 没拖过顺序 · 没去掉过号 · 没换过号的会话，多久没被看见就从账本里清掉（秒）。
pub(crate) const DROP_AFTER: u64 = 7 * 86_400;

impl Session {
    /// 一个新会话：从开会话用的号起。
    pub(crate) fn fresh(agent: &str, start: &str, now: u64) -> Self {
        Self {
            agent: agent.to_string(),
            start: start.to_string(),
            current: start.to_string(),
            since: now,
            seen: now,
            uses: None,
            temp: Vec::new(),
            off: Vec::new(),
            memo: None,
            wake: Wake::default(),
            log: Vec::new(),
        }
    }

    /// 最后一次看见它的时刻（没记过 ⇒ 按 `since`）。
    pub(crate) fn last_seen(&self) -> u64 {
        self.seen.max(self.since)
    }

    /// 记一条换号（至多留 [`LOG_KEPT`] 条，最早的先丢）。
    pub(crate) fn push_log(&mut self, l: Log) {
        self.log.push(l);
        let n = self.log.len();
        if n > LOG_KEPT {
            self.log.drain(..n - LOG_KEPT);
        }
    }
}

impl Book {
    /// 没有文件时那一份：新会话不轮换。
    pub(crate) fn fresh() -> Self {
        Self::default()
    }

    /// 中转第一次看见这个会话 ⇒ 记下、载入 `for_new` 那份副本（缺 ⇒ 不轮换）；会话换了开它的号 ⇒ 在用的号 · 临时动作 · memo · 叫醒清掉（副本照留）。
    /// 每天头一次看见已知的会话 ⇒ 刷新「看见」的时刻；新会话被看见时顺手清旧会话。改了 ⇒ `true`。
    pub(crate) fn saw(
        &mut self,
        sid: &str,
        agent: &str,
        start: &str,
        now: u64,
        for_new: impl FnOnce() -> Option<Copy>,
    ) -> bool {
        match self.sessions.get_mut(sid) {
            None => {
                self.drop_stale(now);
                let s = Session {
                    uses: for_new().map(Arc::new),
                    ..Session::fresh(agent, start, now)
                };
                self.sessions.insert(sid.to_string(), s);
                true
            }
            Some(s) if s.start != start || s.agent != agent => {
                s.agent = agent.to_string();
                s.start = start.to_string();
                s.current = start.to_string();
                s.since = now;
                s.seen = now;
                s.temp.clear();
                s.off.clear();
                s.memo = None;
                s.wake = Wake::default();
                true
            }
            Some(s) if now.saturating_sub(s.last_seen()) >= SEEN_REFRESH => {
                s.seen = now;
                true
            }
            Some(_) => false,
        }
    }

    /// 清旧会话：没拖过顺序 · 没去掉过号 · 没换过号 · 超过 [`DROP_AFTER`] 没被看见 ⇒ 删（再来一发照新会话记回来）。
    pub(crate) fn drop_stale(&mut self, now: u64) -> usize {
        let before = self.sessions.len();
        self.sessions.retain(|_, s| {
            !(s.temp.is_empty()
                && s.off.is_empty()
                && s.log.is_empty()
                && now.saturating_sub(s.last_seen()) > DROP_AFTER)
        });
        before - self.sessions.len()
    }
}

#[cfg(test)]
#[path = "../../../../../tests/backend/accounts/quota/rules/book_tests.rs"]
mod tests;
