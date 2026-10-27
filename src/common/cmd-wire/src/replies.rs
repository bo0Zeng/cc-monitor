//! 「命令 ⇒ 回包类型」那一张表：表里的类型都住本包（[`crate::session`] · [`crate::files`] · [`crate::needs`] · [`crate::listing`] · [`crate::record`] · [`crate::search`] …），
//! 就是后端那条命令 `impl Command` 回的那一个（后端判据按 TS 名对，[`reply_names`]）；手机由同一份类型生成的 Kotlin 类解（`gen-kotlin-wire.mjs`）。
//! 手机嵌的 Rust 核心不先解再转：回包原样交 Kotlin，只有记录那一格在这里一条一条判认不认得（[`mend_records`]）。

use crate::record::{OrUnread, Record};
use serde::Deserialize;

/// 表里那几个类型（只有判据 · 导出 TS 那一档用这张表）。
#[cfg(any(test, feature = "ts"))]
mod table {
    pub(super) use crate::accounts::AccountsList;
    pub(super) use crate::batch::BatchReply;
    pub(super) use crate::bookmarks::Bookmarks;
    pub(super) use crate::files::{
        FilesPathBytesReply, FilesPathReply, FilesStageChunkReply, FilesWrittenReply, LsReply,
        ReadChunkReply, ReadTextReply, StatReply,
    };
    pub(super) use crate::listing::{HistoryHead, HistoryRow};
    pub(super) use crate::needs::{Answered, NeedsList};
    pub(super) use crate::quota::QuotaRead;
    pub(super) use crate::record::{HistoryLines, HistoryRead};
    pub(super) use crate::rotation::{
        RotDone, RotStrategyId, RotStrategyRev, RotationFactsReply, RotationStrategiesReply,
        RotationTempReply, RotationTryReply, RotationViewReply,
    };
    pub(super) use crate::search::HistorySearchReply;
    pub(super) use crate::session::{
        HistoryTail, Killed, ModeSwitched, ModelSwitched, OpenFacts, Opened, Preview, Sent,
        SessionCommands, TerminalsList,
    };
    pub(super) use crate::sync::Patch;
}
#[cfg(any(test, feature = "ts"))]
use table::*;

/// 一张表出两样（判据 · 导出那一档用）：按类型解再写回的 [`typed_text`] · 拿来对后端 `impl Command` 的 [`reply_names`]（同一张表，不会一边多一条）。
macro_rules! replies {
    ($($cmd:literal => $ty:ty,)*) => {
        /// 回包类型住这里的那几条命令（按命令名排）。
        #[cfg(any(test, feature = "ts"))]
        pub const COMMANDS: &[&str] = &[$($cmd),*];

        /// `cmd` 的回包按这里的类型解、再照类型写成一行（格按类型的顺序 —— 手机生成的类写回要与它逐字节同）。
        /// 解得开 ⇒ `Some(Ok(那一行))`；形状对不上 ⇒ `Some(Err(serde 那一句))`；回包类型不住这里 ⇒ `None`。
        #[cfg(any(test, feature = "ts"))]
        pub fn typed_text(cmd: &str, data: serde_json::Value) -> Option<Result<String, String>> {
            fn text<T: serde::Serialize + serde::de::DeserializeOwned>(data: serde_json::Value) -> Result<String, String> {
                let typed: T = serde_json::from_value(data).map_err(|e| e.to_string())?;
                serde_json::to_string(&typed).map_err(|e| e.to_string())
            }
            Some(match cmd {
                $($cmd => text::<$ty>(data),)*
                _ => return None,
            })
        }

        /// `(命令, 解它用的那个类型的 TS 名)`：后端判据对着那条命令 `impl Command` 的回包类型核（同一个名 ⇒ 同一个类型）。
        #[cfg(any(test, feature = "ts"))]
        pub fn reply_names() -> Vec<(&'static str, String)> {
            let cfg = ts_rs::Config::default();
            vec![$(($cmd, <$ty as ts_rs::TS>::name(&cfg)),)*]
        }
    };
}

/// 带记录的那几条回包（`history-read` · `history-tail` 的 `rows[].record`，`history-lines` 的 `lines[].record`）：
/// 记录那一格解不开完整记录的，换成认不出那一条（[`OrUnread::of`]：带原样的类名与摘录，同桌面那几句）；解得开的、别的格都原样不动。
/// 后端照原样的一团写记录、记录的形状不进形状指纹：那一家新出一种记录不该让整页读不出，这一条判定只在这里。
pub fn mend_records(cmd: &str, data: &mut serde_json::Value) {
    let list = match cmd {
        "history-read" | "history-tail" => "rows",
        "history-lines" => "lines",
        _ => return,
    };
    let Some(items) = data.get_mut(list).and_then(serde_json::Value::as_array_mut) else {
        return;
    };
    for rec in items.iter_mut().filter_map(|i| i.get_mut("record")) {
        if Record::deserialize(&*rec).is_err() {
            let mended = OrUnread::of(std::mem::take(rec));
            *rec = serde_json::to_value(&mended).unwrap_or_default();
        }
    }
}

replies! {
    "accounts-list" => AccountsList,
    "bookmarks-edit" => Bookmarks,
    "bookmarks-read" => Bookmarks,
    "files-chmod" => BatchReply,
    "files-commit-upload" => FilesPathBytesReply,
    "files-create" => FilesPathBytesReply,
    "files-delete" => BatchReply,
    "files-home" => FilesPathReply,
    "files-ls" => LsReply,
    "files-mkdir" => FilesPathReply,
    "files-read-chunk" => ReadChunkReply,
    "files-read-text" => ReadTextReply,
    "files-rename" => FilesPathReply,
    "files-stage-chunk" => FilesStageChunkReply,
    "files-stat" => StatReply,
    "files-write-text" => FilesWrittenReply,
    "history-lines" => HistoryLines<OrUnread>,
    "history-list" => Patch<HistoryHead, HistoryRow>,
    "history-read" => HistoryRead<OrUnread>,
    "history-search" => HistorySearchReply,
    "history-tail" => HistoryTail<OrUnread>,
    "kill" => Killed,
    "needs-answer" => Answered,
    "quota-read" => QuotaRead,
    "rotation-facts" => RotationFactsReply,
    "rotation-for-new" => RotDone,
    "rotation-reload" => RotDone,
    "rotation-settings" => RotStrategyRev,
    "rotation-strategies" => RotationStrategiesReply,
    "rotation-strategy-add" => RotStrategyId,
    "rotation-strategy-copy" => RotStrategyId,
    "rotation-strategy-delete" => RotDone,
    "rotation-strategy-rename" => RotStrategyId,
    "rotation-strategy-tz" => RotStrategyRev,
    "rotation-temp" => RotationTempReply,
    "rotation-try" => RotationTryReply,
    "rotation-use" => RotDone,
    "rotation-use-now" => RotDone,
    "rotation-view" => RotationViewReply,
    "session-commands" => SessionCommands,
    "session-model" => ModelSwitched,
    "session-open-facts" => OpenFacts,
    "session-open" => Opened,
    "session-permission" => ModeSwitched,
    "sessions-needs" => NeedsList,
    "terminal-input" => Sent,
    "terminal-preview" => Preview,
    "terminals-list" => TerminalsList,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 照类型解再写回、读成一份（比 JSON 值，不比键序）。
    fn round_trip(cmd: &str, data: serde_json::Value) -> Option<Result<serde_json::Value, String>> {
        typed_text(cmd, data).map(|r| r.map(|t| serde_json::from_str(&t).expect("JSON")))
    }

    /// 表里每一条都解得动（名字没写错），表外的不碰。
    #[test]
    fn every_listed_command_has_a_type_and_others_have_none() {
        for cmd in COMMANDS {
            assert!(round_trip(cmd, serde_json::Value::Null).is_some(), "{cmd}");
        }
        assert!(round_trip("history-facts", serde_json::json!({})).is_none());
    }

    /// `needs-answer` 的回包按两端同一个类型（`needs::Answered`）解：闭集一个词，别的字解不开。
    #[test]
    fn needs_answer_reply_is_the_shared_type() {
        let ok = serde_json::json!({"result": "answered"});
        assert_eq!(round_trip("needs-answer", ok.clone()), Some(Ok(ok)));
        assert!(matches!(
            round_trip("needs-answer", serde_json::json!({"result": "maybe"})),
            Some(Err(_))
        ));
    }

    /// `quota-read` 线上金样（后端真出口写的那几份）每一份都解得开、按类型重写回来一个字节不差（展平的几格 · 可缺的几格都对得上）。
    #[test]
    fn every_quota_read_golden_is_decoded_byte_for_byte() {
        let g: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../tests/__fixtures__/quota-read.golden.json"
        ))
        .unwrap();
        let cases = g["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for c in cases {
            let reply = c["reply"].clone();
            assert_eq!(
                round_trip("quota-read", reply.clone()),
                Some(Ok(reply)),
                "{}",
                c["name"]
            );
        }
    }

    /// 缺一格 ⇒ 解不开；多出来的格不拦（回包不 deny_unknown_fields：新版多一格不算对不上，形状指纹另管）。
    #[test]
    fn a_missing_cell_is_refused_and_the_rewrite_is_the_same_bytes() {
        let tail = serde_json::json!({"total": 3, "tail_from": 1, "split_at": 10, "end": 30, "how": "resume", "from": 2,
            "rows": [{"end": 30, "hash": 7}], "next": 30, "eof": true,
            "held": {"path": "/p", "next": 3, "next_byte": 30, "mark": "m"}});
        assert_eq!(round_trip("history-tail", tail.clone()), Some(Ok(tail)));
        let short = serde_json::json!({"total": 3, "tail_from": 1, "split_at": 10, "end": 30});
        assert!(matches!(round_trip("history-tail", short), Some(Err(_))));
        let refused = serde_json::json!({"result": "refused", "why": "not_yours", "said": "x"});
        assert_eq!(
            round_trip("terminal-input", refused.clone()),
            Some(Ok(refused))
        );
        assert!(matches!(
            round_trip("terminal-input", serde_json::json!({"result": "maybe"})),
            Some(Err(_))
        ));
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/cmd-wire/reply_goldens_tests.rs"]
mod reply_goldens_tests;
