//! 一条命令的身份：入参类型选中那条命令，名字与回包类型都挂在它上面（[`Call`]）。
//! 下面那一张 `calls!` 表是命令身份唯一的声明：后端登记那一行照它取名、取回包，
//! 有 Rust 客户端的那几端（壳 · 文件窗口 · 手机核心）照它发、照它解。

use crate::accounts::{AccountsList, AccountsListReq};
use crate::assets::{AssetsSyncReq, AssetsSynced};
use crate::batch::{BatchReply, FilesBatchStopReply, FilesBatchStopReq};
use crate::bookmarks::{Bookmarks, BookmarksEditReq, BookmarksReadReq};
use crate::deploy::{DeployInstallReq, DeployUninstallReq, Installed, Uninstalled};
use crate::exit_policy::{ExitPolicy, ExitPolicyReadReq};
use crate::files::*;
use crate::listing::{HistoryHead, HistoryListReq, HistoryRow};
use crate::needs::{AnswerReq, Answered, NeedsList, SessionsNeedsReq};
use crate::open::{OpenFactsReq, SessionOpenReq};
use crate::plan::{PlanFiles, PlanFilesReq};
use crate::quota::{QuotaRead, QuotaReadReq};
#[cfg(any(test, feature = "ts"))]
use crate::record::OrUnread;
use crate::record::{HistoryLines, HistoryLinesReq, HistoryRead, HistoryReadReq};
use crate::rotation::*;
use crate::search::{HistorySearchReply, HistorySearchReq};
use crate::session::{HistoryTail, HistoryTailReq};
use crate::session::{
    ModeSwitched, ModelSwitched, OpenFacts, Opened, Preview, Sent, SessionCommands,
    SessionCommandsReq, SessionModelReq, SessionPermissionReq, TerminalsList,
};
use crate::sync::Patch;
use crate::terminal::{
    SshCommand, TerminalInputReq, TerminalPreviewReq, TerminalSshReq, TerminalsListReq, UnfollowReq,
};
use crate::transfer::{
    TransferCopyAsk, TransferCopyReq, TransferDownloadAsk, TransferDownloadReq, TransferOpened,
    TransferStartReq, TransferStopReq, TransferUploadAsk, TransferUploadReq,
};
use serde::Serialize;

/// 一条命令的入参类型就选中了那条命令：名字与回包类型都挂在它上面。
pub trait Call: Serialize {
    /// 线上命令名。
    const NAME: &'static str;
    type Reply: Serialize + serde::de::DeserializeOwned;
}

/// 一行：`入参 => 名字, 回包;`。回包里记录那一格是原样的一团（`Value`）的那几条再写一格 `reads <类型>`：
/// 客户端按完整记录解的那一形（解不开的那一条成认不出那一条，[`crate::record::OrUnread`]），判据照它解。
/// 表还出两样（判据 · 导出 TS 那一档）：表里每条的名字 `COMMANDS` · 照类型解再写回的 `typed_text`（手机生成的 Kotlin 类写回要与它逐字节同）。
macro_rules! calls {
    ($($req:ty => $name:literal, $reply:ty $(, reads $read:ty)?;)*) => {
        $(impl Call for $req {
            const NAME: &'static str = $name;
            type Reply = $reply;
        })*

        /// 表里每条的名字（表的次序；一条命令有两层入参的出现两次）。
        #[cfg(any(test, feature = "ts"))]
        pub const COMMANDS: &[&str] = &[$($name),*];

        /// `cmd` 的回包按表里的类型（写了 `reads` ⇒ 那一形）解、再照类型写成一行（格按类型的顺序）。
        /// 解得开 ⇒ `Some(Ok(那一行))`；形状对不上 ⇒ `Some(Err(serde 那一句))`；表里没有这条 ⇒ `None`。
        #[cfg(any(test, feature = "ts"))]
        pub fn typed_text(cmd: &str, data: serde_json::Value) -> Option<Result<String, String>> {
            fn text<T: Serialize + serde::de::DeserializeOwned>(data: serde_json::Value) -> Result<String, String> {
                let typed: T = serde_json::from_value(data).map_err(|e| e.to_string())?;
                serde_json::to_string(&typed).map_err(|e| e.to_string())
            }
            $(if cmd == $name {
                return Some(text::<reads!($reply $(, $read)?)>(data));
            })*
            None
        }
    };
}

/// 客户端解回包用的那一形：写了 `reads` ⇒ 它，否则就是回包类型。
#[cfg(any(test, feature = "ts"))]
macro_rules! reads {
    ($reply:ty) => {
        $reply
    };
    ($reply:ty, $read:ty) => {
        $read
    };
}

calls! {
    LsReq => "files-ls", LsReply;
    StatReq => "files-stat", StatReply;
    HomeReq => "files-home", FilesPathReply;
    SizeReq => "files-size", SizeReply;
    ReadTextReq => "files-read-text", ReadTextReply;
    ReadChunkReq => "files-read-chunk", ReadChunkReply;
    GrepReq => "files-grep", GrepReply;
    FindReq => "files-find", FindReply;
    IndexStatusReq => "files-index-status", IndexStatusReply;
    RebuildReq => "files-index-rebuild", RebuildReply;
    BrowseReq => "files-browse", BrowseReply;
    FilesCreateReq => "files-create", FilesPathBytesReply;
    FilesMkdirReq => "files-mkdir", FilesPathReply;
    FilesRenameReq => "files-rename", FilesPathReply;
    FilesDeleteReq => "files-delete", BatchReply;
    FilesChmodReq => "files-chmod", BatchReply;
    FilesCopyReq => "files-copy", BatchReply;
    FilesExtractReq => "files-extract", FilesTreeReply;
    FilesWriteTextReq => "files-write-text", FilesWrittenReply;
    FilesDeleteSessionReq => "files-delete-session", FilesPathReply;
    FilesStageChunkReq => "files-stage-chunk", FilesStageChunkReply;
    FilesCommitTextReq => "files-commit-text", FilesWrittenReply;
    FilesCommitUploadReq => "files-commit-upload", FilesPathBytesReply;
    FilesBatchStopReq => "files-batch-stop", FilesBatchStopReply;
    BookmarksReadReq => "bookmarks-read", Bookmarks;
    BookmarksEditReq => "bookmarks-edit", Bookmarks;
    DeployInstallReq => "deploy-install", Installed;
    DeployUninstallReq => "deploy-uninstall", Uninstalled;
    PlanFilesReq => "plan-files", PlanFiles;
    TerminalSshReq => "terminal-ssh", SshCommand;
    UnfollowReq => "terminal-unfollow", ();
    ExitPolicyReadReq => "exit-policy-read", ExitPolicy;
    AssetsSyncReq => "assets-sync", AssetsSynced;
    // 记录那一格是原样的一团（`Value`）：各端照自己的读法再判（手机核心 `record::mend_records` · 壳搬成品）。
    HistoryReadReq => "history-read", HistoryRead, reads HistoryRead<OrUnread>;
    HistoryTailReq => "history-tail", HistoryTail, reads HistoryTail<OrUnread>;
    HistoryLinesReq => "history-lines", HistoryLines, reads HistoryLines<OrUnread>;
    // 传输台那三条各两层：窗口说给 monitor 的（`*Ask`）· monitor 补上那台转给本机后端的（`*Req`），名字同一个。
    TransferUploadAsk => "transfer-upload", TransferOpened;
    TransferUploadReq => "transfer-upload", TransferOpened;
    TransferDownloadAsk => "transfer-download", TransferOpened;
    TransferDownloadReq => "transfer-download", TransferOpened;
    TransferCopyAsk => "transfer-copy", BatchReply;
    TransferCopyReq => "transfer-copy", BatchReply;
    TransferStartReq => "transfer-start", ();
    TransferStopReq => "transfer-stop", ();
    QuotaReadReq => "quota-read", QuotaRead;
    RotationFactsReq => "rotation-facts", RotationFactsReply;
    RotationStrategiesReq => "rotation-strategies", RotationStrategiesReply;
    RotationViewReq => "rotation-view", RotationViewReply;
    RotationSettingsReq => "rotation-settings", RotStrategyRev;
    RotationUseReq => "rotation-use", RotDone;
    RotationReloadReq => "rotation-reload", RotDone;
    RotationForNewReq => "rotation-for-new", RotDone;
    RotationStrategyAddReq => "rotation-strategy-add", RotStrategyId;
    RotationStrategyCopyReq => "rotation-strategy-copy", RotStrategyId;
    RotationStrategyRenameReq => "rotation-strategy-rename", RotStrategyId;
    RotationStrategyTzReq => "rotation-strategy-tz", RotStrategyRev;
    RotationStrategyDeleteReq => "rotation-strategy-delete", RotDone;
    RotationTempReq => "rotation-temp", RotationTempReply;
    RotationUseNowReq => "rotation-use-now", RotDone;
    RotationTryReq => "rotation-try", RotationTryReply;
    TerminalsListReq => "terminals-list", TerminalsList;
    TerminalPreviewReq => "terminal-preview", Preview;
    TerminalInputReq => "terminal-input", Sent;
    HistorySearchReq => "history-search", HistorySearchReply;
    HistoryListReq => "history-list", Patch<HistoryHead, HistoryRow>;
    AccountsListReq => "accounts-list", AccountsList;
    AnswerReq => "needs-answer", Answered;
    SessionsNeedsReq => "sessions-needs", NeedsList;
    SessionCommandsReq => "session-commands", SessionCommands;
    SessionModelReq => "session-model", ModelSwitched;
    SessionPermissionReq => "session-permission", ModeSwitched;
    SessionOpenReq => "session-open", Opened;
    OpenFactsReq => "session-open-facts", OpenFacts;
}

#[cfg(test)]
#[path = "../../../../tests/common/cmd-wire/call_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../../tests/common/cmd-wire/reply_goldens_tests.rs"]
mod reply_goldens_tests;
