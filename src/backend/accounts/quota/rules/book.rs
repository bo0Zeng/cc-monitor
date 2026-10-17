//! `rotation.json` 第三版（基础功能层那一份）：每个会话在用哪个号 · 它载入的那份策略副本 · 会话侧的临时动作（拖出来的顺序 · 先不用的号）·
//! 策略上一判交的 `memo` 与叫醒 · 换号记录（[`Book::saw`] 记新会话 · [`Session::push_log`] 记换号）。

/// 换号记录（盘上那一份 · `rotation-view` 带的同一形）：线上形住 `cmd-wire`。
pub(crate) use cmd_wire::rotation::{RotLog as Log, RotLogCause as Cause};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// 这一版的版本号。
pub(crate) const BOOK_VERSION: u64 = 3;

/// 换号记录至多几条（照第二版）。
pub(crate) const LOG_KEPT: usize = 32;

/// 整份第三版。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Book {
    pub(crate) version: u64,
    #[serde(default)]
    pub(crate) sessions: BTreeMap<String, Session>,
}

/// 一个会话。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Session {
    pub(crate) agent: String,
    /// 起会话的号（路由第 2 段）。
    pub(crate) start: String,
    pub(crate) current: String,
    pub(crate) since: u64,
    #[serde(default)]
    pub(crate) seen: u64,
    /// 这个会话载入的那份策略（缺 ⇒ 不轮换）：载入之后文件再改也不跟，「载入新版」才换。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) uses: Option<Copy>,
    /// 用户拖出来的「接下来先用谁」。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) temp: Vec<String>,
    /// 这个会话先不用的号（会话侧的临时动作：策略不变，会话没了就没了）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) off: Vec<String>,
    /// 策略上一判交的，原样存、下一判原样交回。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) memo: Option<Value>,
    #[serde(default, skip_serializing_if = "Wake::is_empty")]
    pub(crate) wake: Wake,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) log: Vec<Log>,
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
        Self {
            version: BOOK_VERSION,
            ..Self::default()
        }
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
                    uses: for_new(),
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
