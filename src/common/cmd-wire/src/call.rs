//! 一条命令的身份：入参类型选中那条命令，名字与回包类型都挂在它上面（[`Call`]）。
//! 下面那一张 `calls!` 表是命令身份唯一的声明：后端登记那一行照它取名、取回包，
//! 有 Rust 客户端的那几端（壳 · 文件窗口 · 手机核心）照它发、照它解。

use crate::assets::{AssetsSyncReq, AssetsSynced};
use crate::batch::{BatchReply, FilesBatchStopReply, FilesBatchStopReq};
use crate::bookmarks::{Bookmarks, BookmarksEditReq, BookmarksReadReq};
use crate::deploy::{DeployInstallReq, DeployUninstallReq, Installed, Uninstalled};
use crate::exit_policy::{ExitPolicy, ExitPolicyReadReq};
use crate::files::*;
use crate::plan::{PlanFiles, PlanFilesReq};
use crate::quota::{QuotaRead, QuotaReadReq};
use crate::rotation::*;
use crate::record::{HistoryRead, HistoryReadReq};
use crate::session::{HistoryTail, HistoryTailReq};
use crate::accounts::{AccountsList, AccountsListReq};
use crate::listing::{HistoryHead, HistoryListReq, HistoryRow};
use crate::search::{HistorySearchReply, HistorySearchReq};
use crate::session::{Preview, Sent, TerminalsList};
use crate::sync::Patch;
use crate::terminal::{
    SshCommand, TerminalInputReq, TerminalPreviewReq, TerminalSshReq, TerminalsListReq,
    UnfollowReq,
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

macro_rules! calls {
    ($($req:ty => $name:literal, $reply:ty;)*) => {
        $(impl Call for $req {
            const NAME: &'static str = $name;
            type Reply = $reply;
        })*
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
    // 记录那一格是原样的一团（`Value`）：各端照自己的读法再判（手机核心 `replies::mend_records` · 壳搬成品）。
    HistoryReadReq => "history-read", HistoryRead;
    HistoryTailReq => "history-tail", HistoryTail;
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
}

#[cfg(test)]
#[path = "../../../../tests/common/cmd-wire/call_tests.rs"]
mod tests;
