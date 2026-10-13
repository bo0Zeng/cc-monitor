//! **每条长连接报「在看哪几个会话」**（那台后端的 `stream-watch {sids}`，`IPC-PROTOCOL.md`「流只发要看的会话」）：
//! 名单整份换；换了一条新连接（＝ 没报过 ＝ 全看）就照当前名单重报一次；应答 `from[]`（刚进名单的会话从第几行起上流）原样交出去，
//! 由看着那一条的去补 `[已有, seq)`。桌面（monitor `stream_watch.rs`，名单来自窗口与订阅）与手机（[`crate::feed`]，名单来自 [`Watched`]）同一份。

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

/// 那台后端的命令名。
pub const WATCH_CMD: &str = "stream-watch";

/// 一次报名单的期限：后端在连接内就地做完（只是换一张表、数几个游标），几秒足够；超了就等下一次变化 / 下一条连接。
pub const WATCH_BUDGET: Duration = Duration::from_secs(10);

/// 应答里的一格：这条会话从第 `seq` 行起上流。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WatchFrom {
    pub sid: String,
    /// 那份记录在那台机器上的绝对路径。
    pub path: String,
    pub seq: u64,
}

/// `branch` 的一格：要这个会话的主线外清单，手上已有那份记录（`path`）的前 `at` 条、摘要 `sum`（都照手上最后一帧 `session_branch`：
/// `at + off.len()` 与它的 `sum`）。手上没有 / 说不准 ⇒ [`BranchHeld::nothing`]（那台给整份）。摘要是那台铸的，这里只记、只交回。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BranchHeld {
    pub sid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sum: Option<String>,
}

impl BranchHeld {
    /// 要这个会话的清单、手上什么都没有。
    pub fn nothing(sid: &str) -> Self {
        BranchHeld {
            sid: sid.to_string(),
            path: None,
            at: 0,
            sum: None,
        }
    }
}

/// 请求的入参：整份名单 ＋ 要哪几个的主线外清单、手上各有什么（空 ＝ 一份都不要：不读清单的那一头就交空的，那台一帧清单都不推）。
pub fn args(sids: &BTreeSet<String>, branch: &[BranchHeld]) -> serde_json::Value {
    serde_json::json!({ "sids": sids, "branch": branch })
}

/// 应答 `{from: [{sid, path, seq}]}` ⇒ 那几格。形状不对的那一格不收（记一行）。
pub fn from_of(data: Option<&serde_json::Value>) -> Vec<WatchFrom> {
    let Some(rows) = data.and_then(|d| d.get("from")).and_then(|f| f.as_array()) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|r| {
            let got = (|| {
                Some(WatchFrom {
                    sid: r.get("sid")?.as_str()?.to_string(),
                    path: r.get("path")?.as_str()?.to_string(),
                    seq: r.get("seq")?.as_u64()?,
                })
            })();
            if got.is_none() {
                tracing::warn!("stream_watch 应答里一格形状不对，不收：{r}");
            }
            got
        })
        .collect()
}

/// 此刻有人在看的那几条（每条记有几处在看：一处放手不影响别处）。名单 = 计数大于零的那几条。
#[derive(Debug, Default, Clone)]
pub struct Watched {
    held: BTreeMap<String, u32>,
}

impl Watched {
    /// 多一处在看。回名单变没变。
    pub fn hold(&mut self, sid: &str) -> bool {
        let n = self.held.entry(sid.to_string()).or_insert(0);
        *n += 1;
        *n == 1
    }

    /// 少一处在看。回名单变没变（本来就没在看 ⇒ 不变）。
    pub fn release(&mut self, sid: &str) -> bool {
        match self.held.get_mut(sid) {
            Some(n) if *n > 1 => {
                *n -= 1;
                false
            }
            Some(_) => {
                self.held.remove(sid);
                true
            }
            None => false,
        }
    }

    pub fn sids(&self) -> BTreeSet<String> {
        self.held.keys().cloned().collect()
    }
}

#[cfg(test)]
#[path = "../../../tests/comms/resident-client/watch_tests.rs"]
mod tests;
