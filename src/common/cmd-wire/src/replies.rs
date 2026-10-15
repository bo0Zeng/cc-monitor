//! 客户端那一侧（手机嵌的 Rust 核心）按类型解回包：「命令 ⇒ 回包类型」那一张表（[`decode_reply`]）。
//! 表里的类型都住本包（[`crate::session`] · [`crate::files`] · [`crate::needs`] · [`crate::listing`] · [`crate::record`] …），就是后端那条命令 `impl Command` 回的那一个
//! （后端判据按 TS 名对，[`reply_names`]）。表里没有的命令照原样交（回包类型还住后端那几族）。

use crate::batch::BatchReply;
use crate::files::{
    FilesPathBytesReply, FilesPathReply, FilesStageChunkReply, FilesWrittenReply, LsReply,
    ReadChunkReply, ReadTextReply, StatReply,
};
use crate::listing::HistoryListReply;
use crate::needs::{Answered, NeedsList};
use crate::record::{HistoryLines, HistoryRead, OrUnread};
use crate::session::{
    Killed, ModeSwitched, ModelSwitched, NewFacts, Preview, Rendered, Sent, SessionCommands,
    SessionNew, TailPlan, TerminalsList,
};
use serde::Serialize;

fn canon<T: Serialize + serde::de::DeserializeOwned>(
    data: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let typed: T = serde_json::from_value(data).map_err(|e| e.to_string())?;
    serde_json::to_value(&typed).map_err(|e| e.to_string())
}

/// 一张表出两样：客户端按类型解回包的 [`decode_reply`] · 判据拿来对后端 `impl Command` 的 [`reply_names`]（同一张表，不会一边多一条）。
macro_rules! replies {
    ($($cmd:literal => $ty:ty,)*) => {
        /// 回包类型住这里的那几条命令（按命令名排）。
        pub const COMMANDS: &[&str] = &[$($cmd),*];

        /// 客户端那一侧（手机嵌的 Rust 核心）：`cmd` 的回包 `data` 按这里的类型解。
        /// 解得开 ⇒ `Some(Ok(按类型重写的那一份))`；形状对不上 ⇒ `Some(Err(serde 那一句))`；回包类型不住这里 ⇒ `None`（原样用）。
        pub fn decode_reply(
            cmd: &str,
            data: serde_json::Value,
        ) -> Option<Result<serde_json::Value, String>> {
            Some(match cmd {
                $($cmd => canon::<$ty>(data),)*
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

replies! {
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
    "history-list" => HistoryListReply,
    "history-read" => HistoryRead<OrUnread>,
    "history-tail" => TailPlan,
    "kill" => Killed,
    "launch-render-cli" => Rendered,
    "needs-answer" => Answered,
    "session-commands" => SessionCommands,
    "session-model" => ModelSwitched,
    "session-new" => SessionNew,
    "session-new-facts" => NewFacts,
    "session-permission" => ModeSwitched,
    "session-say" => Sent,
    "sessions-needs" => NeedsList,
    "terminal-input" => Sent,
    "terminal-preview" => Preview,
    "terminals-list" => TerminalsList,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 表里每一条都解得动（名字没写错），表外的不碰。
    #[test]
    fn every_listed_command_is_decoded_and_others_pass_through() {
        for cmd in COMMANDS {
            assert!(
                decode_reply(cmd, serde_json::Value::Null).is_some(),
                "{cmd}"
            );
        }
        assert!(decode_reply("history-facts", serde_json::json!({})).is_none());
    }

    /// `needs-answer` 的回包按两端同一个类型（`needs::Answered`）解：闭集一个词，别的字解不开。
    #[test]
    fn needs_answer_reply_is_the_shared_type() {
        let ok = serde_json::json!({"result": "answered"});
        assert_eq!(decode_reply("needs-answer", ok.clone()), Some(Ok(ok)));
        assert!(matches!(
            decode_reply("needs-answer", serde_json::json!({"result": "maybe"})),
            Some(Err(_))
        ));
    }

    /// 缺一格 ⇒ 解不开；多出来的格不拦（回包不 deny_unknown_fields：新版多一格不算对不上，形状指纹另管）。
    #[test]
    fn a_missing_cell_is_refused_and_the_rewrite_is_the_same_bytes() {
        let tail = serde_json::json!({"total": 3, "tail_from": 1, "split_at": 10, "end": 30, "file": "0123456789abcdef"});
        assert_eq!(decode_reply("history-tail", tail.clone()), Some(Ok(tail)));
        let short = serde_json::json!({"total": 3, "tail_from": 1, "split_at": 10, "end": 30});
        assert!(matches!(decode_reply("history-tail", short), Some(Err(_))));
        let refused = serde_json::json!({"result": "refused", "why": "not_yours", "said": "x"});
        assert_eq!(
            decode_reply("terminal-input", refused.clone()),
            Some(Ok(refused))
        );
        assert!(matches!(
            decode_reply("terminal-input", serde_json::json!({"result": "maybe"})),
            Some(Err(_))
        ));
    }
}
