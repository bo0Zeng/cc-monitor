//! `rotation.json` 第三版：只存会话那一侧的事 —— 用哪份策略（加改过的设置）· 用户拖出来的顺序 ·
//! 策略上一判交的 `memo` 与叫醒 · 换号记录（[`Book::saw`] 记新会话 · [`Session::push_log`] 记换号）。没有任何换号的想法：上限、软硬这些都在策略文件的设置里。

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
    /// 新会话用哪份（策略目录里的文件名去掉 `.rhai`）；空 ⇒ 新会话不轮换，只用开会话的号。
    #[serde(default)]
    pub(crate) for_new: String,
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
    pub(crate) strategy: Uses,
    /// 用户拖出来的「接下来先用谁」。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) temp: Vec<String>,
    /// 策略上一判交的，原样存、下一判原样交回。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) memo: Option<Value>,
    #[serde(default, skip_serializing_if = "Wake::is_empty")]
    pub(crate) wake: Wake,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) log: Vec<Log>,
}

/// 会话用的那份策略：文件 id（空 ⇒ 这个会话不轮换）· 打开（或跟上）那一刻的内容版本 · 改过的设置（整份，只在改过时有）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Uses {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) rev: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) draft: Option<Value>,
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

/// 一条换号记录：只记后端看得见的事；策略那一句 `why` 跟着存一份（显示用，后端不解）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(export, export_to = "../../frontend/ui/generated/", rename = "RotLog")
)]
pub(crate) struct Log {
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) at: u64,
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) cause: Cause,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) why: Option<String>,
}

/// 后端看得见的换号原因。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "k", rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "RotLogCause"
    )
)]
pub(crate) enum Cause {
    /// 策略交的先后换了第一个。
    Order,
    /// 在用的号事实上发不出去了。
    Unusable,
    /// 照用户拖出来的顺序。
    Temp,
    /// 用户点了「现在用」。
    UseNow,
    /// 停发（`from == to`），到 `until`。
    Hold {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional, type = "number"))]
        until: Option<u64>,
    },
    /// 策略出错，走了兜底。
    Error,
}

/// 「看见」的时刻隔多久才刷新一次（秒）。
pub(crate) const SEEN_REFRESH: u64 = 86_400;
/// 没草稿 · 没拖过顺序 · 没换过号的会话，多久没被看见就从账本里清掉（秒）。
pub(crate) const DROP_AFTER: u64 = 7 * 86_400;

impl Session {
    /// 一个新会话：照 `strategy` 那份、从开会话用的号起。
    pub(crate) fn fresh(agent: &str, start: &str, now: u64, strategy: &str) -> Self {
        Self {
            agent: agent.to_string(),
            start: start.to_string(),
            current: start.to_string(),
            since: now,
            seen: now,
            strategy: Uses {
                id: strategy.to_string(),
                rev: String::new(),
                draft: None,
            },
            temp: Vec::new(),
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

    /// 用这份策略的会话（按 sid）。
    pub(crate) fn in_use(&self, id: &str) -> Vec<String> {
        self.sessions
            .iter()
            .filter(|(_, s)| s.strategy.id == id)
            .map(|(sid, _)| sid.clone())
            .collect()
    }

    /// 中转第一次看见这个会话 / 会话换了开它的号 ⇒ 记下（新会话照 `forNew`；换了开它的号 ⇒ 在用的号 · 拖出来的顺序 · memo · 叫醒清掉）。
    /// 每天头一次看见已知的会话 ⇒ 刷新「看见」的时刻；新会话被看见时顺手清旧会话。改了 ⇒ `true`。
    pub(crate) fn saw(&mut self, sid: &str, agent: &str, start: &str, now: u64) -> bool {
        let for_new = self.for_new.clone();
        match self.sessions.get_mut(sid) {
            None => {
                self.drop_stale(now);
                self.sessions
                    .insert(sid.to_string(), Session::fresh(agent, start, now, &for_new));
                true
            }
            Some(s) if s.start != start || s.agent != agent => {
                s.agent = agent.to_string();
                s.start = start.to_string();
                s.current = start.to_string();
                s.since = now;
                s.seen = now;
                s.temp.clear();
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

    /// 清旧会话：没草稿 · 没拖过顺序 · 没换过号 · 超过 [`DROP_AFTER`] 没被看见 ⇒ 删（再来一发照新会话记回来）。
    pub(crate) fn drop_stale(&mut self, now: u64) -> usize {
        let before = self.sessions.len();
        self.sessions.retain(|_, s| {
            !(s.strategy.draft.is_none()
                && s.temp.is_empty()
                && s.log.is_empty()
                && now.saturating_sub(s.last_seen()) > DROP_AFTER)
        });
        before - self.sessions.len()
    }
}
