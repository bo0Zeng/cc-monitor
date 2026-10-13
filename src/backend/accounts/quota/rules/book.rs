//! `rotation.json` 第三版：只存会话那一侧的事 —— 用哪份策略（加改过的设置）· 用户拖出来的顺序 ·
//! 策略上一判交的 `memo` 与叫醒 · 换号记录。没有任何换号的想法：上限、软硬这些都在策略文件的设置里。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// 这一版的版本号。
pub(crate) const VERSION: u64 = 3;

/// 换号记录至多几条（照第二版）。
pub(crate) const LOG_KEPT: usize = 32;

/// 整份第三版。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Book {
    pub(crate) version: u64,
    /// 新会话用哪份（策略目录里的文件名去掉 `.rhai`）。
    pub(crate) for_new: String,
    /// 播种过的随包文件（内容哈希）：删了的不再播，包里出了新版也不覆盖。
    #[serde(default)]
    pub(crate) seeded: Vec<String>,
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

/// 会话用的那份策略：文件 id · 打开（或跟上）那一刻的内容版本 · 改过的设置（整份，只在改过时有）。
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
pub(crate) struct Log {
    pub(crate) at: u64,
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) cause: Cause,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) why: Option<String>,
}

/// 后端看得见的换号原因。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "k", rename_all = "camelCase", deny_unknown_fields)]
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
        until: Option<u64>,
    },
    /// 策略出错，走了兜底。
    Error,
}
