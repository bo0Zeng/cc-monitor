// 生成的文件（tests/scripts/gen-kotlin-wire.mjs 读 ts-rs 导出的声明出的）。不要手改：改 Rust 那一份类型，再跑 `npm run gen:types`。
@file:Suppress("ktlint", "detekt:all")

package com.ccmonitor.mobile.core.wire.gen

import com.squareup.moshi.JsonAdapter

/** 每条命令（桌面 TypedCommands 那一份）：名字 · 入参类 · 回包类，按命令名取的 [ALL]；按类取编解码的 [BY_CLASS]。 */
object Commands {
    /** 金样逐字节对过的那几条（`cmd-wire/src/call.rs` 那张 `calls!` 表）⇒ 回包怎么解。 */
    val REPLIES: Map<String, JsonAdapter<*>> = mapOf(
        "files-ls" to com.ccmonitor.mobile.core.wire.gen.SyncPatch.adapter(com.ccmonitor.mobile.core.wire.gen.LsHead.ADAPTER, com.ccmonitor.mobile.core.wire.gen.LsEntry.ADAPTER),
        "files-stat" to com.ccmonitor.mobile.core.wire.gen.StatReply.ADAPTER,
        "files-home" to com.ccmonitor.mobile.core.wire.gen.FilesPathReply.ADAPTER,
        "files-size" to com.ccmonitor.mobile.core.wire.gen.SizeReply.ADAPTER,
        "files-read-text" to com.ccmonitor.mobile.core.wire.gen.ReadTextReply.ADAPTER,
        "files-read-chunk" to com.ccmonitor.mobile.core.wire.gen.ReadChunkReply.ADAPTER,
        "files-grep" to com.ccmonitor.mobile.core.wire.gen.GrepReply.ADAPTER,
        "files-find" to com.ccmonitor.mobile.core.wire.gen.FindReply.ADAPTER,
        "files-index-status" to com.ccmonitor.mobile.core.wire.gen.IndexStatusReply.ADAPTER,
        "files-index-rebuild" to com.ccmonitor.mobile.core.wire.gen.RebuildReply.ADAPTER,
        "files-browse" to com.ccmonitor.mobile.core.wire.gen.BrowseReply.ADAPTER,
        "files-create" to com.ccmonitor.mobile.core.wire.gen.FilesPathBytesReply.ADAPTER,
        "files-mkdir" to com.ccmonitor.mobile.core.wire.gen.FilesPathReply.ADAPTER,
        "files-rename" to com.ccmonitor.mobile.core.wire.gen.FilesPathReply.ADAPTER,
        "files-delete" to com.ccmonitor.mobile.core.wire.gen.BatchReply.ADAPTER,
        "files-chmod" to com.ccmonitor.mobile.core.wire.gen.BatchReply.ADAPTER,
        "files-copy" to com.ccmonitor.mobile.core.wire.gen.BatchReply.ADAPTER,
        "files-extract" to com.ccmonitor.mobile.core.wire.gen.FilesTreeReply.ADAPTER,
        "files-write-text" to com.ccmonitor.mobile.core.wire.gen.FilesWrittenReply.ADAPTER,
        "files-delete-session" to com.ccmonitor.mobile.core.wire.gen.FilesPathReply.ADAPTER,
        "files-stage-chunk" to com.ccmonitor.mobile.core.wire.gen.FilesStageChunkReply.ADAPTER,
        "files-commit-text" to com.ccmonitor.mobile.core.wire.gen.FilesWrittenReply.ADAPTER,
        "files-commit-upload" to com.ccmonitor.mobile.core.wire.gen.FilesPathBytesReply.ADAPTER,
        "files-batch-stop" to com.ccmonitor.mobile.core.wire.gen.FilesBatchStopReply.ADAPTER,
        "bookmarks-read" to com.ccmonitor.mobile.core.wire.gen.Bookmarks.ADAPTER,
        "bookmarks-edit" to com.ccmonitor.mobile.core.wire.gen.Bookmarks.ADAPTER,
        "deploy-install" to com.ccmonitor.mobile.core.wire.gen.Installed.ADAPTER,
        "deploy-uninstall" to com.ccmonitor.mobile.core.wire.gen.Uninstalled.ADAPTER,
        "plan-files" to com.ccmonitor.mobile.core.wire.gen.PlanFiles.ADAPTER,
        "terminal-ssh" to com.ccmonitor.mobile.core.wire.gen.SshCommand.ADAPTER,
        "terminal-unfollow" to com.ccmonitor.mobile.core.wire.gen.RawJson,
        "exit-policy-read" to com.ccmonitor.mobile.core.wire.gen.ExitPolicy.ADAPTER,
        "exit-policy-set" to com.ccmonitor.mobile.core.wire.gen.ExitPolicy.ADAPTER,
        "assets-sync" to com.ccmonitor.mobile.core.wire.gen.AssetsSynced.ADAPTER,
        "assets-catalog" to com.ccmonitor.mobile.core.wire.gen.CatalogReply.ADAPTER,
        "assets-catalog-merge" to com.ccmonitor.mobile.core.wire.gen.CatalogReply.ADAPTER,
        "machine-read" to com.ccmonitor.mobile.core.wire.gen.MachineRead.ADAPTER,
        "machine-wake" to com.ccmonitor.mobile.core.wire.gen.MachineState.ADAPTER,
        "machine-stop" to com.ccmonitor.mobile.core.wire.gen.MachineStopped.ADAPTER,
        "history-read" to com.ccmonitor.mobile.core.wire.gen.HistoryRead.ADAPTER,
        "history-tail" to com.ccmonitor.mobile.core.wire.gen.HistoryTail.ADAPTER,
        "history-lines" to com.ccmonitor.mobile.core.wire.gen.HistoryLines.ADAPTER,
        "transfer-upload" to com.ccmonitor.mobile.core.wire.gen.TransferOpened.ADAPTER,
        "transfer-download" to com.ccmonitor.mobile.core.wire.gen.TransferOpened.ADAPTER,
        "transfer-copy" to com.ccmonitor.mobile.core.wire.gen.BatchReply.ADAPTER,
        "transfer-start" to com.ccmonitor.mobile.core.wire.gen.RawJson,
        "transfer-stop" to com.ccmonitor.mobile.core.wire.gen.RawJson,
        "quota-read" to com.ccmonitor.mobile.core.wire.gen.QuotaRead.ADAPTER,
        "rotation-facts" to com.ccmonitor.mobile.core.wire.gen.RotationFactsReply.ADAPTER,
        "rotation-strategies" to com.ccmonitor.mobile.core.wire.gen.RotationStrategiesReply.ADAPTER,
        "rotation-view" to com.ccmonitor.mobile.core.wire.gen.RotationViewReply.ADAPTER,
        "rotation-settings" to com.ccmonitor.mobile.core.wire.gen.RotStrategyRev.ADAPTER,
        "rotation-use" to com.ccmonitor.mobile.core.wire.gen.RotDone.ADAPTER,
        "rotation-reload" to com.ccmonitor.mobile.core.wire.gen.RotDone.ADAPTER,
        "rotation-for-new" to com.ccmonitor.mobile.core.wire.gen.RotDone.ADAPTER,
        "rotation-strategy-add" to com.ccmonitor.mobile.core.wire.gen.RotStrategyId.ADAPTER,
        "rotation-strategy-copy" to com.ccmonitor.mobile.core.wire.gen.RotStrategyId.ADAPTER,
        "rotation-strategy-rename" to com.ccmonitor.mobile.core.wire.gen.RotStrategyId.ADAPTER,
        "rotation-strategy-tz" to com.ccmonitor.mobile.core.wire.gen.RotStrategyRev.ADAPTER,
        "rotation-strategy-delete" to com.ccmonitor.mobile.core.wire.gen.RotDone.ADAPTER,
        "rotation-temp" to com.ccmonitor.mobile.core.wire.gen.RotationTempReply.ADAPTER,
        "rotation-use-now" to com.ccmonitor.mobile.core.wire.gen.RotDone.ADAPTER,
        "rotation-try" to com.ccmonitor.mobile.core.wire.gen.RotationTryReply.ADAPTER,
        "terminals-list" to com.ccmonitor.mobile.core.wire.gen.TerminalsList.ADAPTER,
        "terminal-preview" to com.ccmonitor.mobile.core.wire.gen.Preview.ADAPTER,
        "terminal-input" to com.ccmonitor.mobile.core.wire.gen.Sent.ADAPTER,
        "history-search" to com.ccmonitor.mobile.core.wire.gen.HistorySearchReply.ADAPTER,
        "history-list" to com.ccmonitor.mobile.core.wire.gen.SyncPatch.adapter(com.ccmonitor.mobile.core.wire.gen.HistoryHead.ADAPTER, com.ccmonitor.mobile.core.wire.gen.HistoryRow.ADAPTER),
        "accounts-list" to com.ccmonitor.mobile.core.wire.gen.AccountsList.ADAPTER,
        "needs-answer" to com.ccmonitor.mobile.core.wire.gen.Answered.ADAPTER,
        "sessions-needs" to com.ccmonitor.mobile.core.wire.gen.NeedsList.ADAPTER,
        "session-commands" to com.ccmonitor.mobile.core.wire.gen.SessionCommands.ADAPTER,
        "session-model" to com.ccmonitor.mobile.core.wire.gen.ModelSwitched.ADAPTER,
        "session-permission" to com.ccmonitor.mobile.core.wire.gen.ModeSwitched.ADAPTER,
        "session-open" to com.ccmonitor.mobile.core.wire.gen.Opened.ADAPTER,
        "session-open-facts" to com.ccmonitor.mobile.core.wire.gen.OpenFacts.ADAPTER,
        "session-arrive" to com.ccmonitor.mobile.core.wire.gen.ArriveEnd.ADAPTER,
        "session-queue-put" to com.ccmonitor.mobile.core.wire.gen.QueuePut.ADAPTER,
        "session-queue-cancel" to com.ccmonitor.mobile.core.wire.gen.QueueCancelled.ADAPTER,
        "session-queue-read" to com.ccmonitor.mobile.core.wire.gen.QueueList.ADAPTER,
        "session-rewind-options" to com.ccmonitor.mobile.core.wire.gen.RewindOptions.ADAPTER,
        "session-attachment-check" to com.ccmonitor.mobile.core.wire.gen.SessionAttachmentCheckReply.ADAPTER,
        "session-attachment-commit" to com.ccmonitor.mobile.core.wire.gen.SessionAttachmentCommitReply.ADAPTER,
        "session-attachment-drop" to com.ccmonitor.mobile.core.wire.gen.SessionAttachmentDropReply.ADAPTER,
    )

    /** `accounts-add`。 */
    object AccountsAdd : Command<com.ccmonitor.mobile.core.wire.gen.AccountAddArgs, com.ccmonitor.mobile.core.wire.gen.AccountChange>("accounts-add", { com.ccmonitor.mobile.core.wire.gen.AccountAddArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.AccountChange.ADAPTER }) {
        const val NAME: String = "accounts-add"
    }

    /** `accounts-init`。 */
    object AccountsInit : Command<com.ccmonitor.mobile.core.wire.gen.AccountInitArgs, com.ccmonitor.mobile.core.wire.gen.AccountChange>("accounts-init", { com.ccmonitor.mobile.core.wire.gen.AccountInitArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.AccountChange.ADAPTER }) {
        const val NAME: String = "accounts-init"
    }

    /** `accounts-list`。 */
    object AccountsList : Command<com.ccmonitor.mobile.core.wire.gen.AccountsListReq, com.ccmonitor.mobile.core.wire.gen.AccountsList>("accounts-list", { com.ccmonitor.mobile.core.wire.gen.AccountsListReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.AccountsList.ADAPTER }) {
        const val NAME: String = "accounts-list"
    }

    /** `accounts-login-cmd`。 */
    object AccountsLoginCmd : Command<com.ccmonitor.mobile.core.wire.gen.AccountNameArgs, com.ccmonitor.mobile.core.wire.gen.AccountLoginCmd>("accounts-login-cmd", { com.ccmonitor.mobile.core.wire.gen.AccountNameArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.AccountLoginCmd.ADAPTER }) {
        const val NAME: String = "accounts-login-cmd"
    }

    /** `accounts-mcp-pick`。 */
    object AccountsMcpPick : Command<com.ccmonitor.mobile.core.wire.gen.AccountMcpPickArgs, com.ccmonitor.mobile.core.wire.gen.AccountMcpView>("accounts-mcp-pick", { com.ccmonitor.mobile.core.wire.gen.AccountMcpPickArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.AccountMcpView.ADAPTER }) {
        const val NAME: String = "accounts-mcp-pick"
    }

    /** `accounts-mcp-read`。 */
    object AccountsMcpRead : Command<com.ccmonitor.mobile.core.wire.gen.NoArgs, com.ccmonitor.mobile.core.wire.gen.AccountMcpView>("accounts-mcp-read", { com.ccmonitor.mobile.core.wire.gen.NoArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.AccountMcpView.ADAPTER }) {
        const val NAME: String = "accounts-mcp-read"
    }

    /** `accounts-mcp-remove`。 */
    object AccountsMcpRemove : Command<com.ccmonitor.mobile.core.wire.gen.AccountMcpNameArgs, com.ccmonitor.mobile.core.wire.gen.AccountMcpView>("accounts-mcp-remove", { com.ccmonitor.mobile.core.wire.gen.AccountMcpNameArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.AccountMcpView.ADAPTER }) {
        const val NAME: String = "accounts-mcp-remove"
    }

    /** `accounts-mcp-sync`。 */
    object AccountsMcpSync : Command<com.ccmonitor.mobile.core.wire.gen.AccountMcpSyncArgs, com.ccmonitor.mobile.core.wire.gen.AccountMcpView>("accounts-mcp-sync", { com.ccmonitor.mobile.core.wire.gen.AccountMcpSyncArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.AccountMcpView.ADAPTER }) {
        const val NAME: String = "accounts-mcp-sync"
    }

    /** `accounts-remove`。 */
    object AccountsRemove : Command<com.ccmonitor.mobile.core.wire.gen.AccountRemoveArgs, com.ccmonitor.mobile.core.wire.gen.AccountChange>("accounts-remove", { com.ccmonitor.mobile.core.wire.gen.AccountRemoveArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.AccountChange.ADAPTER }) {
        const val NAME: String = "accounts-remove"
    }

    /** `accounts-repair`。 */
    object AccountsRepair : Command<com.ccmonitor.mobile.core.wire.gen.AccountRepairArgs, com.ccmonitor.mobile.core.wire.gen.AccountChange>("accounts-repair", { com.ccmonitor.mobile.core.wire.gen.AccountRepairArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.AccountChange.ADAPTER }) {
        const val NAME: String = "accounts-repair"
    }

    /** `accounts-rollback`。 */
    object AccountsRollback : Command<com.ccmonitor.mobile.core.wire.gen.AccountRollbackArgs, com.ccmonitor.mobile.core.wire.gen.AccountChange>("accounts-rollback", { com.ccmonitor.mobile.core.wire.gen.AccountRollbackArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.AccountChange.ADAPTER }) {
        const val NAME: String = "accounts-rollback"
    }

    /** `accounts-set-default`。 */
    object AccountsSetDefault : Command<com.ccmonitor.mobile.core.wire.gen.AccountNameArgs, com.ccmonitor.mobile.core.wire.gen.AccountChange>("accounts-set-default", { com.ccmonitor.mobile.core.wire.gen.AccountNameArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.AccountChange.ADAPTER }) {
        const val NAME: String = "accounts-set-default"
    }

    /** `accounts-verify`。 */
    object AccountsVerify : Command<com.ccmonitor.mobile.core.wire.gen.NoArgs, com.ccmonitor.mobile.core.wire.gen.VerifyReport>("accounts-verify", { com.ccmonitor.mobile.core.wire.gen.NoArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.VerifyReport.ADAPTER }) {
        const val NAME: String = "accounts-verify"
    }

    /** `agent-home-check`。 */
    object AgentHomeCheck : Command<com.ccmonitor.mobile.core.wire.gen.AgentHomeCheckReq, com.ccmonitor.mobile.core.wire.gen.AgentHomeCheck>("agent-home-check", { com.ccmonitor.mobile.core.wire.gen.AgentHomeCheckReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.AgentHomeCheck.ADAPTER }) {
        const val NAME: String = "agent-home-check"
    }

    /** `aliases-block-install`。 */
    object AliasesBlockInstall : Command<com.ccmonitor.mobile.core.wire.gen.BlockReq, com.ccmonitor.mobile.core.wire.gen.BlockDone>("aliases-block-install", { com.ccmonitor.mobile.core.wire.gen.BlockReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.BlockDone.ADAPTER }) {
        const val NAME: String = "aliases-block-install"
    }

    /** `aliases-block-remove`。 */
    object AliasesBlockRemove : Command<com.ccmonitor.mobile.core.wire.gen.BlockReq, com.ccmonitor.mobile.core.wire.gen.BlockDone>("aliases-block-remove", { com.ccmonitor.mobile.core.wire.gen.BlockReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.BlockDone.ADAPTER }) {
        const val NAME: String = "aliases-block-remove"
    }

    /** `aliases-block-render`。 */
    object AliasesBlockRender : Command<com.ccmonitor.mobile.core.wire.gen.BlockReq, com.ccmonitor.mobile.core.wire.gen.BlockText>("aliases-block-render", { com.ccmonitor.mobile.core.wire.gen.BlockReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.BlockText.ADAPTER }) {
        const val NAME: String = "aliases-block-render"
    }

    /** `aliases-read`。 */
    object AliasesRead : Command<com.ccmonitor.mobile.core.wire.gen.AliasesReadReq, com.ccmonitor.mobile.core.wire.gen.AliasListing>("aliases-read", { com.ccmonitor.mobile.core.wire.gen.AliasesReadReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.AliasListing.ADAPTER }) {
        const val NAME: String = "aliases-read"
    }

    /** `apikey-key-set`。 */
    object ApikeyKeySet : Command<com.ccmonitor.mobile.core.wire.gen.KeySetReq, com.ccmonitor.mobile.core.wire.gen.KeySet>("apikey-key-set", { com.ccmonitor.mobile.core.wire.gen.KeySetReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.KeySet.ADAPTER }) {
        const val NAME: String = "apikey-key-set"
    }

    /** `assets-catalog`。 */
    object AssetsCatalog : Command<com.ccmonitor.mobile.core.wire.gen.AssetsCatalogReq, com.ccmonitor.mobile.core.wire.gen.CatalogReply>("assets-catalog", { com.ccmonitor.mobile.core.wire.gen.AssetsCatalogReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.CatalogReply.ADAPTER }) {
        const val NAME: String = "assets-catalog"
    }

    /** `assets-catalog-merge`。 */
    object AssetsCatalogMerge : Command<com.ccmonitor.mobile.core.wire.gen.AssetsCatalogMergeReq, com.ccmonitor.mobile.core.wire.gen.CatalogReply>("assets-catalog-merge", { com.ccmonitor.mobile.core.wire.gen.AssetsCatalogMergeReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.CatalogReply.ADAPTER }) {
        const val NAME: String = "assets-catalog-merge"
    }

    /** `assets-sync`。 */
    object AssetsSync : Command<com.ccmonitor.mobile.core.wire.gen.AssetsSyncReq, com.ccmonitor.mobile.core.wire.gen.AssetsSynced>("assets-sync", { com.ccmonitor.mobile.core.wire.gen.AssetsSyncReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.AssetsSynced.ADAPTER }) {
        const val NAME: String = "assets-sync"
    }

    /** `authorized-keys-add`。 */
    object AuthorizedKeysAdd : Command<com.ccmonitor.mobile.core.wire.gen.KeyAddReq, com.ccmonitor.mobile.core.wire.gen.KeyAdded>("authorized-keys-add", { com.ccmonitor.mobile.core.wire.gen.KeyAddReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.KeyAdded.ADAPTER }) {
        const val NAME: String = "authorized-keys-add"
    }

    /** `backend-log`。 */
    object BackendLog : Command<com.ccmonitor.mobile.core.wire.gen.BackendLogReq, com.ccmonitor.mobile.core.wire.gen.LogTail>("backend-log", { com.ccmonitor.mobile.core.wire.gen.BackendLogReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.LogTail.ADAPTER }) {
        const val NAME: String = "backend-log"
    }

    /** `bookmarks-edit`。 */
    object BookmarksEdit : Command<com.ccmonitor.mobile.core.wire.gen.BookmarksEditReq, com.ccmonitor.mobile.core.wire.gen.Bookmarks>("bookmarks-edit", { com.ccmonitor.mobile.core.wire.gen.BookmarksEditReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.Bookmarks.ADAPTER }) {
        const val NAME: String = "bookmarks-edit"
    }

    /** `bookmarks-read`。 */
    object BookmarksRead : Command<com.ccmonitor.mobile.core.wire.gen.BookmarksReadReq, com.ccmonitor.mobile.core.wire.gen.Bookmarks>("bookmarks-read", { com.ccmonitor.mobile.core.wire.gen.BookmarksReadReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.Bookmarks.ADAPTER }) {
        const val NAME: String = "bookmarks-read"
    }

    /** `bus-commit`。 */
    object BusCommit : Command<com.ccmonitor.mobile.core.wire.gen.BusCommitReq, com.ccmonitor.mobile.core.wire.gen.BusCommitted>("bus-commit", { com.ccmonitor.mobile.core.wire.gen.BusCommitReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.BusCommitted.ADAPTER }) {
        const val NAME: String = "bus-commit"
    }

    /** `bus-inbox`。 */
    object BusInbox : Command<com.ccmonitor.mobile.core.wire.gen.BusInboxReq, com.ccmonitor.mobile.core.wire.gen.BusInbox>("bus-inbox", { com.ccmonitor.mobile.core.wire.gen.BusInboxReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.BusInbox.ADAPTER }) {
        const val NAME: String = "bus-inbox"
    }

    /** `bus-list`。 */
    object BusList : Command<com.ccmonitor.mobile.core.wire.gen.NoArgs, com.ccmonitor.mobile.core.wire.gen.BusList>("bus-list", { com.ccmonitor.mobile.core.wire.gen.NoArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.BusList.ADAPTER }) {
        const val NAME: String = "bus-list"
    }

    /** `bus-peek`。 */
    object BusPeek : Command<com.ccmonitor.mobile.core.wire.gen.BusPeekReq, com.ccmonitor.mobile.core.wire.gen.BusPeeked>("bus-peek", { com.ccmonitor.mobile.core.wire.gen.BusPeekReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.BusPeeked.ADAPTER }) {
        const val NAME: String = "bus-peek"
    }

    /** `bus-send`。 */
    object BusSend : Command<com.ccmonitor.mobile.core.wire.gen.BusEnvelopeReq, com.ccmonitor.mobile.core.wire.gen.BusSent>("bus-send", { com.ccmonitor.mobile.core.wire.gen.BusEnvelopeReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.BusSent.ADAPTER }) {
        const val NAME: String = "bus-send"
    }

    /** `bus-unread`。 */
    object BusUnread : Command<com.ccmonitor.mobile.core.wire.gen.BusUnreadReq, com.ccmonitor.mobile.core.wire.gen.BusUnread>("bus-unread", { com.ccmonitor.mobile.core.wire.gen.BusUnreadReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.BusUnread.ADAPTER }) {
        const val NAME: String = "bus-unread"
    }

    /** `cancel`。 */
    object Cancel : Command<com.ccmonitor.mobile.core.wire.gen.CancelReq, com.ccmonitor.mobile.core.wire.gen.Raw>("cancel", { com.ccmonitor.mobile.core.wire.gen.CancelReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RawJson }) {
        const val NAME: String = "cancel"
    }

    /** `cc-bus-install`。 */
    object CcBusInstall : Command<com.ccmonitor.mobile.core.wire.gen.NoReq, com.ccmonitor.mobile.core.wire.gen.CcBusInstalled>("cc-bus-install", { com.ccmonitor.mobile.core.wire.gen.NoReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.CcBusInstalled.ADAPTER }) {
        const val NAME: String = "cc-bus-install"
    }

    /** `cc-bus-install-state`。 */
    object CcBusInstallState : Command<com.ccmonitor.mobile.core.wire.gen.NoReq, com.ccmonitor.mobile.core.wire.gen.CcBusState>("cc-bus-install-state", { com.ccmonitor.mobile.core.wire.gen.NoReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.CcBusState.ADAPTER }) {
        const val NAME: String = "cc-bus-install-state"
    }

    /** `ccm-print`。 */
    object CcmPrint : Command<com.ccmonitor.mobile.core.wire.gen.CcmPrintReq, com.ccmonitor.mobile.core.wire.gen.CcmLine>("ccm-print", { com.ccmonitor.mobile.core.wire.gen.CcmPrintReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.CcmLine.ADAPTER }) {
        const val NAME: String = "ccm-print"
    }

    /** `ccm-probe`。 */
    object CcmProbe : Command<com.ccmonitor.mobile.core.wire.gen.NoArgs, com.ccmonitor.mobile.core.wire.gen.CcmCard>("ccm-probe", { com.ccmonitor.mobile.core.wire.gen.NoArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.CcmCard.ADAPTER }) {
        const val NAME: String = "ccm-probe"
    }

    /** `ccm-user-path`。 */
    object CcmUserPath : Command<com.ccmonitor.mobile.core.wire.gen.UserPathReq, com.ccmonitor.mobile.core.wire.gen.UserPath>("ccm-user-path", { com.ccmonitor.mobile.core.wire.gen.UserPathReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.UserPath.ADAPTER }) {
        const val NAME: String = "ccm-user-path"
    }

    /** `chores-mark`。 */
    object ChoresMark : Command<com.ccmonitor.mobile.core.wire.gen.ChoresMarkReq, com.ccmonitor.mobile.core.wire.gen.ChoresMarks>("chores-mark", { com.ccmonitor.mobile.core.wire.gen.ChoresMarkReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.ChoresMarks.ADAPTER }) {
        const val NAME: String = "chores-mark"
    }

    /** `chores-merge`。 */
    object ChoresMerge : Command<com.ccmonitor.mobile.core.wire.gen.ChoresMergeReq, com.ccmonitor.mobile.core.wire.gen.ChoresMerged>("chores-merge", { com.ccmonitor.mobile.core.wire.gen.ChoresMergeReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.ChoresMerged.ADAPTER }) {
        const val NAME: String = "chores-merge"
    }

    /** `data-report`。 */
    object DataReport : Command<com.ccmonitor.mobile.core.wire.gen.FootprintReq, com.ccmonitor.mobile.core.wire.gen.DataReport>("data-report", { com.ccmonitor.mobile.core.wire.gen.FootprintReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.DataReport.ADAPTER }) {
        const val NAME: String = "data-report"
    }

    /** `deploy-install`。 */
    object DeployInstall : Command<com.ccmonitor.mobile.core.wire.gen.DeployInstallReq, com.ccmonitor.mobile.core.wire.gen.Installed>("deploy-install", { com.ccmonitor.mobile.core.wire.gen.DeployInstallReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.Installed.ADAPTER }) {
        const val NAME: String = "deploy-install"
    }

    /** `deploy-uninstall`。 */
    object DeployUninstall : Command<com.ccmonitor.mobile.core.wire.gen.DeployUninstallReq, com.ccmonitor.mobile.core.wire.gen.Uninstalled>("deploy-uninstall", { com.ccmonitor.mobile.core.wire.gen.DeployUninstallReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.Uninstalled.ADAPTER }) {
        const val NAME: String = "deploy-uninstall"
    }

    /** `drift-merge`。 */
    object DriftMerge : Command<com.ccmonitor.mobile.core.wire.gen.NoArgs, com.ccmonitor.mobile.core.wire.gen.DriftMerge>("drift-merge", { com.ccmonitor.mobile.core.wire.gen.NoArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.DriftMerge.ADAPTER }) {
        const val NAME: String = "drift-merge"
    }

    /** `drift-report`。 */
    object DriftReport : Command<com.ccmonitor.mobile.core.wire.gen.NoArgs, com.ccmonitor.mobile.core.wire.gen.DriftReport>("drift-report", { com.ccmonitor.mobile.core.wire.gen.NoArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.DriftReport.ADAPTER }) {
        const val NAME: String = "drift-report"
    }

    /** `exit-policy-read`。 */
    object ExitPolicyRead : Command<com.ccmonitor.mobile.core.wire.gen.ExitPolicyReadReq, com.ccmonitor.mobile.core.wire.gen.ExitPolicy>("exit-policy-read", { com.ccmonitor.mobile.core.wire.gen.ExitPolicyReadReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.ExitPolicy.ADAPTER }) {
        const val NAME: String = "exit-policy-read"
    }

    /** `exit-policy-set`。 */
    object ExitPolicySet : Command<com.ccmonitor.mobile.core.wire.gen.ExitPolicySetReq, com.ccmonitor.mobile.core.wire.gen.ExitPolicy>("exit-policy-set", { com.ccmonitor.mobile.core.wire.gen.ExitPolicySetReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.ExitPolicy.ADAPTER }) {
        const val NAME: String = "exit-policy-set"
    }

    /** `ext-hub-apply`。 */
    object ExtHubApply : Command<com.ccmonitor.mobile.core.wire.gen.HubApplyReq, com.ccmonitor.mobile.core.wire.gen.HubApplied>("ext-hub-apply", { com.ccmonitor.mobile.core.wire.gen.HubApplyReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.HubApplied.ADAPTER }) {
        const val NAME: String = "ext-hub-apply"
    }

    /** `ext-hub-preview`。 */
    object ExtHubPreview : Command<com.ccmonitor.mobile.core.wire.gen.HubReq, com.ccmonitor.mobile.core.wire.gen.HubPreview>("ext-hub-preview", { com.ccmonitor.mobile.core.wire.gen.HubReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.HubPreview.ADAPTER }) {
        const val NAME: String = "ext-hub-preview"
    }

    /** `ext-list`。 */
    object ExtList : Command<com.ccmonitor.mobile.core.wire.gen.ExtListReq, com.ccmonitor.mobile.core.wire.gen.ExtList>("ext-list", { com.ccmonitor.mobile.core.wire.gen.ExtListReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.ExtList.ADAPTER }) {
        const val NAME: String = "ext-list"
    }

    /** `ext-list-here`。 */
    object ExtListHere : Command<com.ccmonitor.mobile.core.wire.gen.ExtListReq, com.ccmonitor.mobile.core.wire.gen.ExtList>("ext-list-here", { com.ccmonitor.mobile.core.wire.gen.ExtListReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.ExtList.ADAPTER }) {
        const val NAME: String = "ext-list-here"
    }

    /** `ext-note-set`。 */
    object ExtNoteSet : Command<com.ccmonitor.mobile.core.wire.gen.ExtNoteReq, com.ccmonitor.mobile.core.wire.gen.ExtNote>("ext-note-set", { com.ccmonitor.mobile.core.wire.gen.ExtNoteReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.ExtNote.ADAPTER }) {
        const val NAME: String = "ext-note-set"
    }

    /** `ext-uninstall-apply`。 */
    object ExtUninstallApply : Command<com.ccmonitor.mobile.core.wire.gen.ExtUninstallApplyReq, com.ccmonitor.mobile.core.wire.gen.ExtDone>("ext-uninstall-apply", { com.ccmonitor.mobile.core.wire.gen.ExtUninstallApplyReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.ExtDone.ADAPTER }) {
        const val NAME: String = "ext-uninstall-apply"
    }

    /** `ext-uninstall-preview`。 */
    object ExtUninstallPreview : Command<com.ccmonitor.mobile.core.wire.gen.ExtUninstallReq, com.ccmonitor.mobile.core.wire.gen.ExtUninstallCard>("ext-uninstall-preview", { com.ccmonitor.mobile.core.wire.gen.ExtUninstallReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.ExtUninstallCard.ADAPTER }) {
        const val NAME: String = "ext-uninstall-preview"
    }

    /** `files-batch-stop`。 */
    object FilesBatchStop : Command<com.ccmonitor.mobile.core.wire.gen.FilesBatchStopReq, com.ccmonitor.mobile.core.wire.gen.FilesBatchStopReply>("files-batch-stop", { com.ccmonitor.mobile.core.wire.gen.FilesBatchStopReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.FilesBatchStopReply.ADAPTER }) {
        const val NAME: String = "files-batch-stop"
    }

    /** `files-browse`。 */
    object FilesBrowse : Command<com.ccmonitor.mobile.core.wire.gen.BrowseReq, com.ccmonitor.mobile.core.wire.gen.BrowseReply>("files-browse", { com.ccmonitor.mobile.core.wire.gen.BrowseReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.BrowseReply.ADAPTER }) {
        const val NAME: String = "files-browse"
    }

    /** `files-chmod`。 */
    object FilesChmod : Command<com.ccmonitor.mobile.core.wire.gen.FilesChmodReq, com.ccmonitor.mobile.core.wire.gen.BatchReply>("files-chmod", { com.ccmonitor.mobile.core.wire.gen.FilesChmodReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.BatchReply.ADAPTER }) {
        const val NAME: String = "files-chmod"
    }

    /** `files-commit-text`。 */
    object FilesCommitText : Command<com.ccmonitor.mobile.core.wire.gen.FilesCommitTextReq, com.ccmonitor.mobile.core.wire.gen.FilesWrittenReply>("files-commit-text", { com.ccmonitor.mobile.core.wire.gen.FilesCommitTextReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.FilesWrittenReply.ADAPTER }) {
        const val NAME: String = "files-commit-text"
    }

    /** `files-commit-upload`。 */
    object FilesCommitUpload : Command<com.ccmonitor.mobile.core.wire.gen.FilesCommitUploadReq, com.ccmonitor.mobile.core.wire.gen.FilesPathBytesReply>("files-commit-upload", { com.ccmonitor.mobile.core.wire.gen.FilesCommitUploadReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.FilesPathBytesReply.ADAPTER }) {
        const val NAME: String = "files-commit-upload"
    }

    /** `files-copy`。 */
    object FilesCopy : Command<com.ccmonitor.mobile.core.wire.gen.FilesCopyReq, com.ccmonitor.mobile.core.wire.gen.BatchReply>("files-copy", { com.ccmonitor.mobile.core.wire.gen.FilesCopyReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.BatchReply.ADAPTER }) {
        const val NAME: String = "files-copy"
    }

    /** `files-create`。 */
    object FilesCreate : Command<com.ccmonitor.mobile.core.wire.gen.FilesCreateReq, com.ccmonitor.mobile.core.wire.gen.FilesPathBytesReply>("files-create", { com.ccmonitor.mobile.core.wire.gen.FilesCreateReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.FilesPathBytesReply.ADAPTER }) {
        const val NAME: String = "files-create"
    }

    /** `files-delete`。 */
    object FilesDelete : Command<com.ccmonitor.mobile.core.wire.gen.FilesDeleteReq, com.ccmonitor.mobile.core.wire.gen.BatchReply>("files-delete", { com.ccmonitor.mobile.core.wire.gen.FilesDeleteReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.BatchReply.ADAPTER }) {
        const val NAME: String = "files-delete"
    }

    /** `files-delete-session`。 */
    object FilesDeleteSession : Command<com.ccmonitor.mobile.core.wire.gen.FilesDeleteSessionReq, com.ccmonitor.mobile.core.wire.gen.FilesPathReply>("files-delete-session", { com.ccmonitor.mobile.core.wire.gen.FilesDeleteSessionReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.FilesPathReply.ADAPTER }) {
        const val NAME: String = "files-delete-session"
    }

    /** `files-extract`。 */
    object FilesExtract : Command<com.ccmonitor.mobile.core.wire.gen.FilesExtractReq, com.ccmonitor.mobile.core.wire.gen.FilesTreeReply>("files-extract", { com.ccmonitor.mobile.core.wire.gen.FilesExtractReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.FilesTreeReply.ADAPTER }) {
        const val NAME: String = "files-extract"
    }

    /** `files-find`。 */
    object FilesFind : Command<com.ccmonitor.mobile.core.wire.gen.FindReq, com.ccmonitor.mobile.core.wire.gen.FindReply>("files-find", { com.ccmonitor.mobile.core.wire.gen.FindReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.FindReply.ADAPTER }) {
        const val NAME: String = "files-find"
    }

    /** `files-grep`。 */
    object FilesGrep : Command<com.ccmonitor.mobile.core.wire.gen.GrepReq, com.ccmonitor.mobile.core.wire.gen.GrepReply>("files-grep", { com.ccmonitor.mobile.core.wire.gen.GrepReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.GrepReply.ADAPTER }) {
        const val NAME: String = "files-grep"
    }

    /** `files-home`。 */
    object FilesHome : Command<com.ccmonitor.mobile.core.wire.gen.HomeReq, com.ccmonitor.mobile.core.wire.gen.FilesPathReply>("files-home", { com.ccmonitor.mobile.core.wire.gen.HomeReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.FilesPathReply.ADAPTER }) {
        const val NAME: String = "files-home"
    }

    /** `files-index-rebuild`。 */
    object FilesIndexRebuild : Command<com.ccmonitor.mobile.core.wire.gen.RebuildReq, com.ccmonitor.mobile.core.wire.gen.RebuildReply>("files-index-rebuild", { com.ccmonitor.mobile.core.wire.gen.RebuildReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RebuildReply.ADAPTER }) {
        const val NAME: String = "files-index-rebuild"
    }

    /** `files-index-status`。 */
    object FilesIndexStatus : Command<com.ccmonitor.mobile.core.wire.gen.IndexStatusReq, com.ccmonitor.mobile.core.wire.gen.IndexStatusReply>("files-index-status", { com.ccmonitor.mobile.core.wire.gen.IndexStatusReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.IndexStatusReply.ADAPTER }) {
        const val NAME: String = "files-index-status"
    }

    /** `files-ls`。 */
    object FilesLs : Command<com.ccmonitor.mobile.core.wire.gen.LsReq, com.ccmonitor.mobile.core.wire.gen.SyncPatch<com.ccmonitor.mobile.core.wire.gen.LsHead, com.ccmonitor.mobile.core.wire.gen.LsEntry>>("files-ls", { com.ccmonitor.mobile.core.wire.gen.LsReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.SyncPatch.adapter(com.ccmonitor.mobile.core.wire.gen.LsHead.ADAPTER, com.ccmonitor.mobile.core.wire.gen.LsEntry.ADAPTER) }) {
        const val NAME: String = "files-ls"
    }

    /** `files-mkdir`。 */
    object FilesMkdir : Command<com.ccmonitor.mobile.core.wire.gen.FilesMkdirReq, com.ccmonitor.mobile.core.wire.gen.FilesPathReply>("files-mkdir", { com.ccmonitor.mobile.core.wire.gen.FilesMkdirReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.FilesPathReply.ADAPTER }) {
        const val NAME: String = "files-mkdir"
    }

    /** `files-read-chunk`。 */
    object FilesReadChunk : Command<com.ccmonitor.mobile.core.wire.gen.ReadChunkReq, com.ccmonitor.mobile.core.wire.gen.ReadChunkReply>("files-read-chunk", { com.ccmonitor.mobile.core.wire.gen.ReadChunkReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.ReadChunkReply.ADAPTER }) {
        const val NAME: String = "files-read-chunk"
    }

    /** `files-read-text`。 */
    object FilesReadText : Command<com.ccmonitor.mobile.core.wire.gen.ReadTextReq, com.ccmonitor.mobile.core.wire.gen.ReadTextReply>("files-read-text", { com.ccmonitor.mobile.core.wire.gen.ReadTextReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.ReadTextReply.ADAPTER }) {
        const val NAME: String = "files-read-text"
    }

    /** `files-rename`。 */
    object FilesRename : Command<com.ccmonitor.mobile.core.wire.gen.FilesRenameReq, com.ccmonitor.mobile.core.wire.gen.FilesPathReply>("files-rename", { com.ccmonitor.mobile.core.wire.gen.FilesRenameReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.FilesPathReply.ADAPTER }) {
        const val NAME: String = "files-rename"
    }

    /** `files-size`。 */
    object FilesSize : Command<com.ccmonitor.mobile.core.wire.gen.SizeReq, com.ccmonitor.mobile.core.wire.gen.SizeReply>("files-size", { com.ccmonitor.mobile.core.wire.gen.SizeReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.SizeReply.ADAPTER }) {
        const val NAME: String = "files-size"
    }

    /** `files-stage-chunk`。 */
    object FilesStageChunk : Command<com.ccmonitor.mobile.core.wire.gen.FilesStageChunkReq, com.ccmonitor.mobile.core.wire.gen.FilesStageChunkReply>("files-stage-chunk", { com.ccmonitor.mobile.core.wire.gen.FilesStageChunkReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.FilesStageChunkReply.ADAPTER }) {
        const val NAME: String = "files-stage-chunk"
    }

    /** `files-stat`。 */
    object FilesStat : Command<com.ccmonitor.mobile.core.wire.gen.StatReq, com.ccmonitor.mobile.core.wire.gen.StatReply>("files-stat", { com.ccmonitor.mobile.core.wire.gen.StatReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.StatReply.ADAPTER }) {
        const val NAME: String = "files-stat"
    }

    /** `files-write-text`。 */
    object FilesWriteText : Command<com.ccmonitor.mobile.core.wire.gen.FilesWriteTextReq, com.ccmonitor.mobile.core.wire.gen.FilesWrittenReply>("files-write-text", { com.ccmonitor.mobile.core.wire.gen.FilesWriteTextReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.FilesWrittenReply.ADAPTER }) {
        const val NAME: String = "files-write-text"
    }

    /** `first-run`。 */
    object FirstRun : Command<com.ccmonitor.mobile.core.wire.gen.FirstRunReq, com.ccmonitor.mobile.core.wire.gen.FirstRun>("first-run", { com.ccmonitor.mobile.core.wire.gen.FirstRunReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.FirstRun.ADAPTER }) {
        const val NAME: String = "first-run"
    }

    /** `footprint-report`。 */
    object FootprintReport : Command<com.ccmonitor.mobile.core.wire.gen.FootprintReq, com.ccmonitor.mobile.core.wire.gen.ConfigSurfaceReport>("footprint-report", { com.ccmonitor.mobile.core.wire.gen.FootprintReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.ConfigSurfaceReport.ADAPTER }) {
        const val NAME: String = "footprint-report"
    }

    /** `forward-list`。 */
    object ForwardList : Command<com.ccmonitor.mobile.core.wire.gen.NoArgs, com.ccmonitor.mobile.core.wire.gen.Forwards>("forward-list", { com.ccmonitor.mobile.core.wire.gen.NoArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.Forwards.ADAPTER }) {
        const val NAME: String = "forward-list"
    }

    /** `forward-start`。 */
    object ForwardStart : Command<com.ccmonitor.mobile.core.wire.gen.ForwardStartReq, com.ccmonitor.mobile.core.wire.gen.ForwardStarted>("forward-start", { com.ccmonitor.mobile.core.wire.gen.ForwardStartReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.ForwardStarted.ADAPTER }) {
        const val NAME: String = "forward-start"
    }

    /** `forward-stop`。 */
    object ForwardStop : Command<com.ccmonitor.mobile.core.wire.gen.ForwardId, com.ccmonitor.mobile.core.wire.gen.ForwardId>("forward-stop", { com.ccmonitor.mobile.core.wire.gen.ForwardId.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.ForwardId.ADAPTER }) {
        const val NAME: String = "forward-stop"
    }

    /** `groups-here`。 */
    object GroupsHere : Command<com.ccmonitor.mobile.core.wire.gen.NoArgs, com.ccmonitor.mobile.core.wire.gen.GroupsHere>("groups-here", { com.ccmonitor.mobile.core.wire.gen.NoArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.GroupsHere.ADAPTER }) {
        const val NAME: String = "groups-here"
    }

    /** `groups-read`。 */
    object GroupsRead : Command<com.ccmonitor.mobile.core.wire.gen.GroupsReadReq, com.ccmonitor.mobile.core.wire.gen.GroupsRead>("groups-read", { com.ccmonitor.mobile.core.wire.gen.GroupsReadReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.GroupsRead.ADAPTER }) {
        const val NAME: String = "groups-read"
    }

    /** `groups-write`。 */
    object GroupsWrite : Command<com.ccmonitor.mobile.core.wire.gen.GroupsWriteReq, com.ccmonitor.mobile.core.wire.gen.GroupsWritten>("groups-write", { com.ccmonitor.mobile.core.wire.gen.GroupsWriteReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.GroupsWritten.ADAPTER }) {
        const val NAME: String = "groups-write"
    }

    /** `history-annotate`。 */
    object HistoryAnnotate : Command<com.ccmonitor.mobile.core.wire.gen.AnnotateReq, com.ccmonitor.mobile.core.wire.gen.Annotated>("history-annotate", { com.ccmonitor.mobile.core.wire.gen.AnnotateReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.Annotated.ADAPTER }) {
        const val NAME: String = "history-annotate"
    }

    /** `history-branch`。 */
    object HistoryBranch : Command<com.ccmonitor.mobile.core.wire.gen.BranchReq, com.ccmonitor.mobile.core.wire.gen.Branch>("history-branch", { com.ccmonitor.mobile.core.wire.gen.BranchReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.Branch.ADAPTER }) {
        const val NAME: String = "history-branch"
    }

    /** `history-browse`。 */
    object HistoryBrowse : Command<com.ccmonitor.mobile.core.wire.gen.HistoryBrowseReq, com.ccmonitor.mobile.core.wire.gen.SyncPatch<com.ccmonitor.mobile.core.wire.gen.HistoryBrowseHead, com.ccmonitor.mobile.core.wire.gen.HistoryLine>>("history-browse", { com.ccmonitor.mobile.core.wire.gen.HistoryBrowseReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.SyncPatch.adapter(com.ccmonitor.mobile.core.wire.gen.HistoryBrowseHead.ADAPTER, com.ccmonitor.mobile.core.wire.gen.HistoryLine.ADAPTER) }) {
        const val NAME: String = "history-browse"
    }

    /** `history-facts`。 */
    object HistoryFacts : Command<com.ccmonitor.mobile.core.wire.gen.HistoryFactsReq, com.ccmonitor.mobile.core.wire.gen.FactsReply>("history-facts", { com.ccmonitor.mobile.core.wire.gen.HistoryFactsReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.FactsReply.ADAPTER }) {
        const val NAME: String = "history-facts"
    }

    /** `history-find`。 */
    object HistoryFind : Command<com.ccmonitor.mobile.core.wire.gen.HistoryFindReq, com.ccmonitor.mobile.core.wire.gen.HistoryFind>("history-find", { com.ccmonitor.mobile.core.wire.gen.HistoryFindReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.HistoryFind.ADAPTER }) {
        const val NAME: String = "history-find"
    }

    /** `history-forget`。 */
    object HistoryForget : Command<com.ccmonitor.mobile.core.wire.gen.ForgetReq, com.ccmonitor.mobile.core.wire.gen.Forgotten>("history-forget", { com.ccmonitor.mobile.core.wire.gen.ForgetReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.Forgotten.ADAPTER }) {
        const val NAME: String = "history-forget"
    }

    /** `history-image`。 */
    object HistoryImage : Command<com.ccmonitor.mobile.core.wire.gen.HistoryImageReq, com.ccmonitor.mobile.core.wire.gen.HistoryImage>("history-image", { com.ccmonitor.mobile.core.wire.gen.HistoryImageReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.HistoryImage.ADAPTER }) {
        const val NAME: String = "history-image"
    }

    /** `history-index`。 */
    object HistoryIndex : Command<com.ccmonitor.mobile.core.wire.gen.HistoryIndexReq, com.ccmonitor.mobile.core.wire.gen.HistoryIndex>("history-index", { com.ccmonitor.mobile.core.wire.gen.HistoryIndexReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.HistoryIndex.ADAPTER }) {
        const val NAME: String = "history-index"
    }

    /** `history-lines`。 */
    object HistoryLines : Command<com.ccmonitor.mobile.core.wire.gen.HistoryLinesReq, com.ccmonitor.mobile.core.wire.gen.HistoryLines>("history-lines", { com.ccmonitor.mobile.core.wire.gen.HistoryLinesReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.HistoryLines.ADAPTER }) {
        const val NAME: String = "history-lines"
    }

    /** `history-list`。 */
    object HistoryList : Command<com.ccmonitor.mobile.core.wire.gen.HistoryListReq, com.ccmonitor.mobile.core.wire.gen.SyncPatch<com.ccmonitor.mobile.core.wire.gen.HistoryHead, com.ccmonitor.mobile.core.wire.gen.HistoryRow>>("history-list", { com.ccmonitor.mobile.core.wire.gen.HistoryListReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.SyncPatch.adapter(com.ccmonitor.mobile.core.wire.gen.HistoryHead.ADAPTER, com.ccmonitor.mobile.core.wire.gen.HistoryRow.ADAPTER) }) {
        const val NAME: String = "history-list"
    }

    /** `history-listing`。 */
    object HistoryListing : Command<com.ccmonitor.mobile.core.wire.gen.HistoryListingReq, com.ccmonitor.mobile.core.wire.gen.SyncPatch<com.ccmonitor.mobile.core.wire.gen.HistoryListingHead, com.ccmonitor.mobile.core.wire.gen.HistoryRawRow>>("history-listing", { com.ccmonitor.mobile.core.wire.gen.HistoryListingReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.SyncPatch.adapter(com.ccmonitor.mobile.core.wire.gen.HistoryListingHead.ADAPTER, com.ccmonitor.mobile.core.wire.gen.HistoryRawRow.ADAPTER) }) {
        const val NAME: String = "history-listing"
    }

    /** `history-page`。 */
    object HistoryPage : Command<com.ccmonitor.mobile.core.wire.gen.HistoryPageReq, com.ccmonitor.mobile.core.wire.gen.HistoryPage>("history-page", { com.ccmonitor.mobile.core.wire.gen.HistoryPageReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.HistoryPage.ADAPTER }) {
        const val NAME: String = "history-page"
    }

    /** `history-read`。 */
    object HistoryRead : Command<com.ccmonitor.mobile.core.wire.gen.HistoryReadReq, com.ccmonitor.mobile.core.wire.gen.HistoryRead>("history-read", { com.ccmonitor.mobile.core.wire.gen.HistoryReadReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.HistoryRead.ADAPTER }) {
        const val NAME: String = "history-read"
    }

    /** `history-record`。 */
    object HistoryRecord : Command<com.ccmonitor.mobile.core.wire.gen.HistoryRecordReq, com.ccmonitor.mobile.core.wire.gen.RecordProbe>("history-record", { com.ccmonitor.mobile.core.wire.gen.HistoryRecordReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RecordProbe.ADAPTER }) {
        const val NAME: String = "history-record"
    }

    /** `history-run`。 */
    object HistoryRun : Command<com.ccmonitor.mobile.core.wire.gen.HistoryRunReq, com.ccmonitor.mobile.core.wire.gen.HistoryRun>("history-run", { com.ccmonitor.mobile.core.wire.gen.HistoryRunReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.HistoryRun.ADAPTER }) {
        const val NAME: String = "history-run"
    }

    /** `history-search`。 */
    object HistorySearch : Command<com.ccmonitor.mobile.core.wire.gen.HistorySearchReq, com.ccmonitor.mobile.core.wire.gen.HistorySearchReply>("history-search", { com.ccmonitor.mobile.core.wire.gen.HistorySearchReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.HistorySearchReply.ADAPTER }) {
        const val NAME: String = "history-search"
    }

    /** `history-search-merge`。 */
    object HistorySearchMerge : Command<com.ccmonitor.mobile.core.wire.gen.HistorySearchMergeReq, com.ccmonitor.mobile.core.wire.gen.Merged>("history-search-merge", { com.ccmonitor.mobile.core.wire.gen.HistorySearchMergeReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.Merged.ADAPTER }) {
        const val NAME: String = "history-search-merge"
    }

    /** `history-tail`。 */
    object HistoryTail : Command<com.ccmonitor.mobile.core.wire.gen.HistoryTailReq, com.ccmonitor.mobile.core.wire.gen.HistoryTail>("history-tail", { com.ccmonitor.mobile.core.wire.gen.HistoryTailReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.HistoryTail.ADAPTER }) {
        const val NAME: String = "history-tail"
    }

    /** `history-turns`。 */
    object HistoryTurns : Command<com.ccmonitor.mobile.core.wire.gen.HistoryTurnsReq, com.ccmonitor.mobile.core.wire.gen.HistoryTurns>("history-turns", { com.ccmonitor.mobile.core.wire.gen.HistoryTurnsReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.HistoryTurns.ADAPTER }) {
        const val NAME: String = "history-turns"
    }

    /** `history-user-inputs`。 */
    object HistoryUserInputs : Command<com.ccmonitor.mobile.core.wire.gen.UserInputsReq, com.ccmonitor.mobile.core.wire.gen.UserInputs>("history-user-inputs", { com.ccmonitor.mobile.core.wire.gen.UserInputsReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.UserInputs.ADAPTER }) {
        const val NAME: String = "history-user-inputs"
    }

    /** `hooks-diag`。 */
    object HooksDiag : Command<com.ccmonitor.mobile.core.wire.gen.NoArgs, com.ccmonitor.mobile.core.wire.gen.HooksReport>("hooks-diag", { com.ccmonitor.mobile.core.wire.gen.NoArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.HooksReport.ADAPTER }) {
        const val NAME: String = "hooks-diag"
    }

    /** `host-keys-forget`。 */
    object HostKeysForget : Command<com.ccmonitor.mobile.core.wire.gen.HostKeysForgetReq, com.ccmonitor.mobile.core.wire.gen.MachinesReply>("host-keys-forget", { com.ccmonitor.mobile.core.wire.gen.HostKeysForgetReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.MachinesReply.ADAPTER }) {
        const val NAME: String = "host-keys-forget"
    }

    /** `last-seen-read`。 */
    object LastSeenRead : Command<com.ccmonitor.mobile.core.wire.gen.LastSeenReadReq, com.ccmonitor.mobile.core.wire.gen.LastSeen>("last-seen-read", { com.ccmonitor.mobile.core.wire.gen.LastSeenReadReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.LastSeen.ADAPTER }) {
        const val NAME: String = "last-seen-read"
    }

    /** `last-seen-write`。 */
    object LastSeenWrite : Command<com.ccmonitor.mobile.core.wire.gen.LastSeenWriteReq, com.ccmonitor.mobile.core.wire.gen.LastSeenWritten>("last-seen-write", { com.ccmonitor.mobile.core.wire.gen.LastSeenWriteReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.LastSeenWritten.ADAPTER }) {
        const val NAME: String = "last-seen-write"
    }

    /** `login-cancel`。 */
    object LoginCancel : Command<com.ccmonitor.mobile.core.wire.gen.LoginCancelReq, com.ccmonitor.mobile.core.wire.gen.LoginCancelled>("login-cancel", { com.ccmonitor.mobile.core.wire.gen.LoginCancelReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.LoginCancelled.ADAPTER }) {
        const val NAME: String = "login-cancel"
    }

    /** `login-finish`。 */
    object LoginFinish : Command<com.ccmonitor.mobile.core.wire.gen.LoginFinishReq, com.ccmonitor.mobile.core.wire.gen.LoginDone>("login-finish", { com.ccmonitor.mobile.core.wire.gen.LoginFinishReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.LoginDone.ADAPTER }) {
        const val NAME: String = "login-finish"
    }

    /** `login-start`。 */
    object LoginStart : Command<com.ccmonitor.mobile.core.wire.gen.LoginStartArgs, com.ccmonitor.mobile.core.wire.gen.LoginStarted>("login-start", { com.ccmonitor.mobile.core.wire.gen.LoginStartArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.LoginStarted.ADAPTER }) {
        const val NAME: String = "login-start"
    }

    /** `machine-find`。 */
    object MachineFind : Command<com.ccmonitor.mobile.core.wire.gen.MachineFindReq, com.ccmonitor.mobile.core.wire.gen.MachineFind>("machine-find", { com.ccmonitor.mobile.core.wire.gen.MachineFindReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.MachineFind.ADAPTER }) {
        const val NAME: String = "machine-find"
    }

    /** `machine-interrupts`。 */
    object MachineInterrupts : Command<com.ccmonitor.mobile.core.wire.gen.MachineInterruptsReq, com.ccmonitor.mobile.core.wire.gen.MachineInterruptsReply>("machine-interrupts", { com.ccmonitor.mobile.core.wire.gen.MachineInterruptsReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.MachineInterruptsReply.ADAPTER }) {
        const val NAME: String = "machine-interrupts"
    }

    /** `machine-menu`。 */
    object MachineMenu : Command<com.ccmonitor.mobile.core.wire.gen.MachineMenuReq, com.ccmonitor.mobile.core.wire.gen.MachineMenu>("machine-menu", { com.ccmonitor.mobile.core.wire.gen.MachineMenuReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.MachineMenu.ADAPTER }) {
        const val NAME: String = "machine-menu"
    }

    /** `machine-read`。 */
    object MachineRead : Command<com.ccmonitor.mobile.core.wire.gen.MachineReadReq, com.ccmonitor.mobile.core.wire.gen.MachineRead>("machine-read", { com.ccmonitor.mobile.core.wire.gen.MachineReadReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.MachineRead.ADAPTER }) {
        const val NAME: String = "machine-read"
    }

    /** `machine-stop`。 */
    object MachineStop : Command<com.ccmonitor.mobile.core.wire.gen.MachineStopReq, com.ccmonitor.mobile.core.wire.gen.MachineStopped>("machine-stop", { com.ccmonitor.mobile.core.wire.gen.MachineStopReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.MachineStopped.ADAPTER }) {
        const val NAME: String = "machine-stop"
    }

    /** `machine-wake`。 */
    object MachineWake : Command<com.ccmonitor.mobile.core.wire.gen.MachineWakeReq, com.ccmonitor.mobile.core.wire.gen.MachineState>("machine-wake", { com.ccmonitor.mobile.core.wire.gen.MachineWakeReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.MachineState.ADAPTER }) {
        const val NAME: String = "machine-wake"
    }

    /** `machines-read`。 */
    object MachinesRead : Command<com.ccmonitor.mobile.core.wire.gen.NoArgs, com.ccmonitor.mobile.core.wire.gen.MachinesReply>("machines-read", { com.ccmonitor.mobile.core.wire.gen.NoArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.MachinesReply.ADAPTER }) {
        const val NAME: String = "machines-read"
    }

    /** `machines-set`。 */
    object MachinesSet : Command<com.ccmonitor.mobile.core.wire.gen.MachinesEdits, com.ccmonitor.mobile.core.wire.gen.MachinesSet>("machines-set", { com.ccmonitor.mobile.core.wire.gen.MachinesEdits.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.MachinesSet.ADAPTER }) {
        const val NAME: String = "machines-set"
    }

    /** `machines-try`。 */
    object MachinesTry : Command<com.ccmonitor.mobile.core.wire.gen.MachinesEdits, com.ccmonitor.mobile.core.wire.gen.MachinesTried>("machines-try", { com.ccmonitor.mobile.core.wire.gen.MachinesEdits.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.MachinesTried.ADAPTER }) {
        const val NAME: String = "machines-try"
    }

    /** `mcp-read`。 */
    object McpRead : Command<com.ccmonitor.mobile.core.wire.gen.McpReadReq, com.ccmonitor.mobile.core.wire.gen.McpReadReply>("mcp-read", { com.ccmonitor.mobile.core.wire.gen.McpReadReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.McpReadReply.ADAPTER }) {
        const val NAME: String = "mcp-read"
    }

    /** `mcp-sync-apply`。 */
    object McpSyncApply : Command<com.ccmonitor.mobile.core.wire.gen.McpApplyReq, com.ccmonitor.mobile.core.wire.gen.McpApplied>("mcp-sync-apply", { com.ccmonitor.mobile.core.wire.gen.McpApplyReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.McpApplied.ADAPTER }) {
        const val NAME: String = "mcp-sync-apply"
    }

    /** `mcp-sync-preview`。 */
    object McpSyncPreview : Command<com.ccmonitor.mobile.core.wire.gen.McpPreviewReq, com.ccmonitor.mobile.core.wire.gen.McpPreview>("mcp-sync-preview", { com.ccmonitor.mobile.core.wire.gen.McpPreviewReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.McpPreview.ADAPTER }) {
        const val NAME: String = "mcp-sync-preview"
    }

    /** `mcp-sync-source`。 */
    object McpSyncSource : Command<com.ccmonitor.mobile.core.wire.gen.McpSourceReq, com.ccmonitor.mobile.core.wire.gen.McpSource>("mcp-sync-source", { com.ccmonitor.mobile.core.wire.gen.McpSourceReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.McpSource.ADAPTER }) {
        const val NAME: String = "mcp-sync-source"
    }

    /** `monitor-board`。 */
    object MonitorBoard : Command<com.ccmonitor.mobile.core.wire.gen.MonitorBoardReq, com.ccmonitor.mobile.core.wire.gen.MonitorBoard>("monitor-board", { com.ccmonitor.mobile.core.wire.gen.MonitorBoardReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.MonitorBoard.ADAPTER }) {
        const val NAME: String = "monitor-board"
    }

    /** `monitor-board-here`。 */
    object MonitorBoardHere : Command<com.ccmonitor.mobile.core.wire.gen.MonitorHereReq, com.ccmonitor.mobile.core.wire.gen.MonitorHere>("monitor-board-here", { com.ccmonitor.mobile.core.wire.gen.MonitorHereReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.MonitorHere.ADAPTER }) {
        const val NAME: String = "monitor-board-here"
    }

    /** `needs-answer`。 */
    object NeedsAnswer : Command<com.ccmonitor.mobile.core.wire.gen.AnswerReq, com.ccmonitor.mobile.core.wire.gen.Answered>("needs-answer", { com.ccmonitor.mobile.core.wire.gen.AnswerReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.Answered.ADAPTER }) {
        const val NAME: String = "needs-answer"
    }

    /** `needs-ask`。 */
    object NeedsAsk : Command<com.ccmonitor.mobile.core.wire.gen.AskReq, com.ccmonitor.mobile.core.wire.gen.AskReply>("needs-ask", { com.ccmonitor.mobile.core.wire.gen.AskReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.AskReply.ADAPTER }) {
        const val NAME: String = "needs-ask"
    }

    /** `ping`。 */
    object Ping : Command<com.ccmonitor.mobile.core.wire.gen.NoArgs, com.ccmonitor.mobile.core.wire.gen.Raw>("ping", { com.ccmonitor.mobile.core.wire.gen.NoArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RawJson }) {
        const val NAME: String = "ping"
    }

    /** `place-verdict`。 */
    object PlaceVerdict : Command<com.ccmonitor.mobile.core.wire.gen.PlaceReq, com.ccmonitor.mobile.core.wire.gen.PlaceAnswer>("place-verdict", { com.ccmonitor.mobile.core.wire.gen.PlaceReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.PlaceAnswer.ADAPTER }) {
        const val NAME: String = "place-verdict"
    }

    /** `plan-ack`。 */
    object PlanAck : Command<com.ccmonitor.mobile.core.wire.gen.PlanAckReq, com.ccmonitor.mobile.core.wire.gen.PlanAckReply>("plan-ack", { com.ccmonitor.mobile.core.wire.gen.PlanAckReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.PlanAckReply.ADAPTER }) {
        const val NAME: String = "plan-ack"
    }

    /** `plan-ask`。 */
    object PlanAsk : Command<com.ccmonitor.mobile.core.wire.gen.PlanAskReq, com.ccmonitor.mobile.core.wire.gen.PlanAsk>("plan-ask", { com.ccmonitor.mobile.core.wire.gen.PlanAskReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.PlanAsk.ADAPTER }) {
        const val NAME: String = "plan-ask"
    }

    /** `plan-cell-view`。 */
    object PlanCellView : Command<com.ccmonitor.mobile.core.wire.gen.PlanCellViewReq, com.ccmonitor.mobile.core.wire.gen.PlanCellView>("plan-cell-view", { com.ccmonitor.mobile.core.wire.gen.PlanCellViewReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.PlanCellView.ADAPTER }) {
        const val NAME: String = "plan-cell-view"
    }

    /** `plan-command`。 */
    object PlanCommand : Command<com.ccmonitor.mobile.core.wire.gen.PlanCommandReq, com.ccmonitor.mobile.core.wire.gen.PlanCmdReply>("plan-command", { com.ccmonitor.mobile.core.wire.gen.PlanCommandReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.PlanCmdReply.ADAPTER }) {
        const val NAME: String = "plan-command"
    }

    /** `plan-files`。 */
    object PlanFiles : Command<com.ccmonitor.mobile.core.wire.gen.PlanFilesReq, com.ccmonitor.mobile.core.wire.gen.PlanFiles>("plan-files", { com.ccmonitor.mobile.core.wire.gen.PlanFilesReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.PlanFiles.ADAPTER }) {
        const val NAME: String = "plan-files"
    }

    /** `plan-list`。 */
    object PlanList : Command<com.ccmonitor.mobile.core.wire.gen.PlanListReq, com.ccmonitor.mobile.core.wire.gen.PlanList>("plan-list", { com.ccmonitor.mobile.core.wire.gen.PlanListReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.PlanList.ADAPTER }) {
        const val NAME: String = "plan-list"
    }

    /** `plan-new`。 */
    object PlanNew : Command<com.ccmonitor.mobile.core.wire.gen.PlanNewReq, com.ccmonitor.mobile.core.wire.gen.PlanNewForm>("plan-new", { com.ccmonitor.mobile.core.wire.gen.PlanNewReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.PlanNewForm.ADAPTER }) {
        const val NAME: String = "plan-new"
    }

    /** `plan-pb-check`。 */
    object PlanPbCheck : Command<com.ccmonitor.mobile.core.wire.gen.PlanPbCheckReq, com.ccmonitor.mobile.core.wire.gen.PlanPbCheck>("plan-pb-check", { com.ccmonitor.mobile.core.wire.gen.PlanPbCheckReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.PlanPbCheck.ADAPTER }) {
        const val NAME: String = "plan-pb-check"
    }

    /** `plan-read`。 */
    object PlanRead : Command<com.ccmonitor.mobile.core.wire.gen.PlanReadReq, com.ccmonitor.mobile.core.wire.gen.PlanRead>("plan-read", { com.ccmonitor.mobile.core.wire.gen.PlanReadReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.PlanRead.ADAPTER }) {
        const val NAME: String = "plan-read"
    }

    /** `plan-refs`。 */
    object PlanRefs : Command<com.ccmonitor.mobile.core.wire.gen.PlanRefsReq, com.ccmonitor.mobile.core.wire.gen.PlanMsgRefs>("plan-refs", { com.ccmonitor.mobile.core.wire.gen.PlanRefsReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.PlanMsgRefs.ADAPTER }) {
        const val NAME: String = "plan-refs"
    }

    /** `plan-return`。 */
    object PlanReturn : Command<com.ccmonitor.mobile.core.wire.gen.PlanReturnReq, com.ccmonitor.mobile.core.wire.gen.PlanReturnReply>("plan-return", { com.ccmonitor.mobile.core.wire.gen.PlanReturnReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.PlanReturnReply.ADAPTER }) {
        const val NAME: String = "plan-return"
    }

    /** `plan-unack`。 */
    object PlanUnack : Command<com.ccmonitor.mobile.core.wire.gen.PlanAckReq, com.ccmonitor.mobile.core.wire.gen.PlanAckReply>("plan-unack", { com.ccmonitor.mobile.core.wire.gen.PlanAckReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.PlanAckReply.ADAPTER }) {
        const val NAME: String = "plan-unack"
    }

    /** `powershell-policy-set`。 */
    object PowershellPolicySet : Command<com.ccmonitor.mobile.core.wire.gen.PolicySetReq, com.ccmonitor.mobile.core.wire.gen.PolicySet>("powershell-policy-set", { com.ccmonitor.mobile.core.wire.gen.PolicySetReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.PolicySet.ADAPTER }) {
        const val NAME: String = "powershell-policy-set"
    }

    /** `prefs-read`。 */
    object PrefsRead : Command<com.ccmonitor.mobile.core.wire.gen.NoArgs, com.ccmonitor.mobile.core.wire.gen.PrefsReply>("prefs-read", { com.ccmonitor.mobile.core.wire.gen.NoArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.PrefsReply.ADAPTER }) {
        const val NAME: String = "prefs-read"
    }

    /** `prefs-set`。 */
    object PrefsSet : Command<com.ccmonitor.mobile.core.wire.gen.PrefsSet, com.ccmonitor.mobile.core.wire.gen.PrefsReply>("prefs-set", { com.ccmonitor.mobile.core.wire.gen.PrefsSet.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.PrefsReply.ADAPTER }) {
        const val NAME: String = "prefs-set"
    }

    /** `profiles-bases`。 */
    object ProfilesBases : Command<com.ccmonitor.mobile.core.wire.gen.ProfilesBasesReq, com.ccmonitor.mobile.core.wire.gen.ProfilesBases>("profiles-bases", { com.ccmonitor.mobile.core.wire.gen.ProfilesBasesReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.ProfilesBases.ADAPTER }) {
        const val NAME: String = "profiles-bases"
    }

    /** `profiles-from-line`。 */
    object ProfilesFromLine : Command<com.ccmonitor.mobile.core.wire.gen.ProfilesFromLineReq, com.ccmonitor.mobile.core.wire.gen.ProfileForm>("profiles-from-line", { com.ccmonitor.mobile.core.wire.gen.ProfilesFromLineReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.ProfileForm.ADAPTER }) {
        const val NAME: String = "profiles-from-line"
    }

    /** `profiles-impact`。 */
    object ProfilesImpact : Command<com.ccmonitor.mobile.core.wire.gen.ProfilesImpactReq, com.ccmonitor.mobile.core.wire.gen.ProfilesImpact>("profiles-impact", { com.ccmonitor.mobile.core.wire.gen.ProfilesImpactReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.ProfilesImpact.ADAPTER }) {
        const val NAME: String = "profiles-impact"
    }

    /** `profiles-read`。 */
    object ProfilesRead : Command<com.ccmonitor.mobile.core.wire.gen.NoArgs, com.ccmonitor.mobile.core.wire.gen.ProfilesBook>("profiles-read", { com.ccmonitor.mobile.core.wire.gen.NoArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.ProfilesBook.ADAPTER }) {
        const val NAME: String = "profiles-read"
    }

    /** `profiles-resolve`。 */
    object ProfilesResolve : Command<com.ccmonitor.mobile.core.wire.gen.ProfilesResolveReq, com.ccmonitor.mobile.core.wire.gen.ProfileResolved>("profiles-resolve", { com.ccmonitor.mobile.core.wire.gen.ProfilesResolveReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.ProfileResolved.ADAPTER }) {
        const val NAME: String = "profiles-resolve"
    }

    /** `profiles-write`。 */
    object ProfilesWrite : Command<com.ccmonitor.mobile.core.wire.gen.ProfilesWriteReq, com.ccmonitor.mobile.core.wire.gen.ProfilesWriteDone>("profiles-write", { com.ccmonitor.mobile.core.wire.gen.ProfilesWriteReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.ProfilesWriteDone.ADAPTER }) {
        const val NAME: String = "profiles-write"
    }

    /** `prose-hl`。 */
    object ProseHl : Command<com.ccmonitor.mobile.core.wire.gen.ProseHlReq, com.ccmonitor.mobile.core.wire.gen.SegHl>("prose-hl", { com.ccmonitor.mobile.core.wire.gen.ProseHlReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.SegHl.ADAPTER }) {
        const val NAME: String = "prose-hl"
    }

    /** `pubkey-push`。 */
    object PubkeyPush : Command<com.ccmonitor.mobile.core.wire.gen.PubkeyPushReq, com.ccmonitor.mobile.core.wire.gen.PubkeyPushed>("pubkey-push", { com.ccmonitor.mobile.core.wire.gen.PubkeyPushReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.PubkeyPushed.ADAPTER }) {
        const val NAME: String = "pubkey-push"
    }

    /** `quota-probe`。 */
    object QuotaProbe : Command<com.ccmonitor.mobile.core.wire.gen.QuotaProbeReq, com.ccmonitor.mobile.core.wire.gen.Probed>("quota-probe", { com.ccmonitor.mobile.core.wire.gen.QuotaProbeReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.Probed.ADAPTER }) {
        const val NAME: String = "quota-probe"
    }

    /** `quota-read`。 */
    object QuotaRead : Command<com.ccmonitor.mobile.core.wire.gen.QuotaReadReq, com.ccmonitor.mobile.core.wire.gen.QuotaRead>("quota-read", { com.ccmonitor.mobile.core.wire.gen.QuotaReadReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.QuotaRead.ADAPTER }) {
        const val NAME: String = "quota-read"
    }

    /** `quota-warm`。 */
    object QuotaWarm : Command<com.ccmonitor.mobile.core.wire.gen.QuotaWarmReq, com.ccmonitor.mobile.core.wire.gen.Warmed>("quota-warm", { com.ccmonitor.mobile.core.wire.gen.QuotaWarmReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.Warmed.ADAPTER }) {
        const val NAME: String = "quota-warm"
    }

    /** `remote-probe`。 */
    object RemoteProbe : Command<com.ccmonitor.mobile.core.wire.gen.ProbeReq, com.ccmonitor.mobile.core.wire.gen.Raw>("remote-probe", { com.ccmonitor.mobile.core.wire.gen.ProbeReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RawJson }) {
        const val NAME: String = "remote-probe"
    }

    /** `resync`。 */
    object Resync : Command<com.ccmonitor.mobile.core.wire.gen.ResyncReq, com.ccmonitor.mobile.core.wire.gen.Resynced>("resync", { com.ccmonitor.mobile.core.wire.gen.ResyncReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.Resynced.ADAPTER }) {
        const val NAME: String = "resync"
    }

    /** `rotation-facts`。 */
    object RotationFacts : Command<com.ccmonitor.mobile.core.wire.gen.RotationFactsReq, com.ccmonitor.mobile.core.wire.gen.RotationFactsReply>("rotation-facts", { com.ccmonitor.mobile.core.wire.gen.RotationFactsReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RotationFactsReply.ADAPTER }) {
        const val NAME: String = "rotation-facts"
    }

    /** `rotation-for-new`。 */
    object RotationForNew : Command<com.ccmonitor.mobile.core.wire.gen.RotationForNewReq, com.ccmonitor.mobile.core.wire.gen.RotDone>("rotation-for-new", { com.ccmonitor.mobile.core.wire.gen.RotationForNewReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RotDone.ADAPTER }) {
        const val NAME: String = "rotation-for-new"
    }

    /** `rotation-reload`。 */
    object RotationReload : Command<com.ccmonitor.mobile.core.wire.gen.RotationReloadReq, com.ccmonitor.mobile.core.wire.gen.RotDone>("rotation-reload", { com.ccmonitor.mobile.core.wire.gen.RotationReloadReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RotDone.ADAPTER }) {
        const val NAME: String = "rotation-reload"
    }

    /** `rotation-settings`。 */
    object RotationSettings : Command<com.ccmonitor.mobile.core.wire.gen.RotationSettingsReq, com.ccmonitor.mobile.core.wire.gen.RotStrategyRev>("rotation-settings", { com.ccmonitor.mobile.core.wire.gen.RotationSettingsReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RotStrategyRev.ADAPTER }) {
        const val NAME: String = "rotation-settings"
    }

    /** `rotation-strategies`。 */
    object RotationStrategies : Command<com.ccmonitor.mobile.core.wire.gen.RotationStrategiesReq, com.ccmonitor.mobile.core.wire.gen.RotationStrategiesReply>("rotation-strategies", { com.ccmonitor.mobile.core.wire.gen.RotationStrategiesReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RotationStrategiesReply.ADAPTER }) {
        const val NAME: String = "rotation-strategies"
    }

    /** `rotation-strategy-add`。 */
    object RotationStrategyAdd : Command<com.ccmonitor.mobile.core.wire.gen.RotationStrategyAddReq, com.ccmonitor.mobile.core.wire.gen.RotStrategyId>("rotation-strategy-add", { com.ccmonitor.mobile.core.wire.gen.RotationStrategyAddReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RotStrategyId.ADAPTER }) {
        const val NAME: String = "rotation-strategy-add"
    }

    /** `rotation-strategy-copy`。 */
    object RotationStrategyCopy : Command<com.ccmonitor.mobile.core.wire.gen.RotationStrategyCopyReq, com.ccmonitor.mobile.core.wire.gen.RotStrategyId>("rotation-strategy-copy", { com.ccmonitor.mobile.core.wire.gen.RotationStrategyCopyReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RotStrategyId.ADAPTER }) {
        const val NAME: String = "rotation-strategy-copy"
    }

    /** `rotation-strategy-delete`。 */
    object RotationStrategyDelete : Command<com.ccmonitor.mobile.core.wire.gen.RotationStrategyDeleteReq, com.ccmonitor.mobile.core.wire.gen.RotDone>("rotation-strategy-delete", { com.ccmonitor.mobile.core.wire.gen.RotationStrategyDeleteReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RotDone.ADAPTER }) {
        const val NAME: String = "rotation-strategy-delete"
    }

    /** `rotation-strategy-rename`。 */
    object RotationStrategyRename : Command<com.ccmonitor.mobile.core.wire.gen.RotationStrategyRenameReq, com.ccmonitor.mobile.core.wire.gen.RotStrategyId>("rotation-strategy-rename", { com.ccmonitor.mobile.core.wire.gen.RotationStrategyRenameReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RotStrategyId.ADAPTER }) {
        const val NAME: String = "rotation-strategy-rename"
    }

    /** `rotation-strategy-tz`。 */
    object RotationStrategyTz : Command<com.ccmonitor.mobile.core.wire.gen.RotationStrategyTzReq, com.ccmonitor.mobile.core.wire.gen.RotStrategyRev>("rotation-strategy-tz", { com.ccmonitor.mobile.core.wire.gen.RotationStrategyTzReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RotStrategyRev.ADAPTER }) {
        const val NAME: String = "rotation-strategy-tz"
    }

    /** `rotation-temp`。 */
    object RotationTemp : Command<com.ccmonitor.mobile.core.wire.gen.RotationTempReq, com.ccmonitor.mobile.core.wire.gen.RotationTempReply>("rotation-temp", { com.ccmonitor.mobile.core.wire.gen.RotationTempReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RotationTempReply.ADAPTER }) {
        const val NAME: String = "rotation-temp"
    }

    /** `rotation-try`。 */
    object RotationTry : Command<com.ccmonitor.mobile.core.wire.gen.RotationTryReq, com.ccmonitor.mobile.core.wire.gen.RotationTryReply>("rotation-try", { com.ccmonitor.mobile.core.wire.gen.RotationTryReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RotationTryReply.ADAPTER }) {
        const val NAME: String = "rotation-try"
    }

    /** `rotation-use`。 */
    object RotationUse : Command<com.ccmonitor.mobile.core.wire.gen.RotationUseReq, com.ccmonitor.mobile.core.wire.gen.RotDone>("rotation-use", { com.ccmonitor.mobile.core.wire.gen.RotationUseReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RotDone.ADAPTER }) {
        const val NAME: String = "rotation-use"
    }

    /** `rotation-use-now`。 */
    object RotationUseNow : Command<com.ccmonitor.mobile.core.wire.gen.RotationUseNowReq, com.ccmonitor.mobile.core.wire.gen.RotDone>("rotation-use-now", { com.ccmonitor.mobile.core.wire.gen.RotationUseNowReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RotDone.ADAPTER }) {
        const val NAME: String = "rotation-use-now"
    }

    /** `rotation-view`。 */
    object RotationView : Command<com.ccmonitor.mobile.core.wire.gen.RotationViewReq, com.ccmonitor.mobile.core.wire.gen.RotationViewReply>("rotation-view", { com.ccmonitor.mobile.core.wire.gen.RotationViewReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RotationViewReply.ADAPTER }) {
        const val NAME: String = "rotation-view"
    }

    /** `session-account`。 */
    object SessionAccount : Command<com.ccmonitor.mobile.core.wire.gen.SessionAccountReq, com.ccmonitor.mobile.core.wire.gen.SessionAccountReply>("session-account", { com.ccmonitor.mobile.core.wire.gen.SessionAccountReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.SessionAccountReply.ADAPTER }) {
        const val NAME: String = "session-account"
    }

    /** `session-arrive`。 */
    object SessionArrive : Command<com.ccmonitor.mobile.core.wire.gen.ArriveAsk, com.ccmonitor.mobile.core.wire.gen.ArriveEnd>("session-arrive", { com.ccmonitor.mobile.core.wire.gen.ArriveAsk.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.ArriveEnd.ADAPTER }) {
        const val NAME: String = "session-arrive"
    }

    /** `session-attachment-check`。 */
    object SessionAttachmentCheck : Command<com.ccmonitor.mobile.core.wire.gen.SessionAttachmentCheckReq, com.ccmonitor.mobile.core.wire.gen.SessionAttachmentCheckReply>("session-attachment-check", { com.ccmonitor.mobile.core.wire.gen.SessionAttachmentCheckReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.SessionAttachmentCheckReply.ADAPTER }) {
        const val NAME: String = "session-attachment-check"
    }

    /** `session-attachment-commit`。 */
    object SessionAttachmentCommit : Command<com.ccmonitor.mobile.core.wire.gen.SessionAttachmentCommitReq, com.ccmonitor.mobile.core.wire.gen.SessionAttachmentCommitReply>("session-attachment-commit", { com.ccmonitor.mobile.core.wire.gen.SessionAttachmentCommitReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.SessionAttachmentCommitReply.ADAPTER }) {
        const val NAME: String = "session-attachment-commit"
    }

    /** `session-attachment-drop`。 */
    object SessionAttachmentDrop : Command<com.ccmonitor.mobile.core.wire.gen.SessionAttachmentDropReq, com.ccmonitor.mobile.core.wire.gen.SessionAttachmentDropReply>("session-attachment-drop", { com.ccmonitor.mobile.core.wire.gen.SessionAttachmentDropReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.SessionAttachmentDropReply.ADAPTER }) {
        const val NAME: String = "session-attachment-drop"
    }

    /** `session-commands`。 */
    object SessionCommands : Command<com.ccmonitor.mobile.core.wire.gen.SessionCommandsReq, com.ccmonitor.mobile.core.wire.gen.SessionCommands>("session-commands", { com.ccmonitor.mobile.core.wire.gen.SessionCommandsReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.SessionCommands.ADAPTER }) {
        const val NAME: String = "session-commands"
    }

    /** `session-fork`。 */
    object SessionFork : Command<com.ccmonitor.mobile.core.wire.gen.ForkReq, com.ccmonitor.mobile.core.wire.gen.Forked>("session-fork", { com.ccmonitor.mobile.core.wire.gen.ForkReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.Forked.ADAPTER }) {
        const val NAME: String = "session-fork"
    }

    /** `session-interrupt`。 */
    object SessionInterrupt : Command<com.ccmonitor.mobile.core.wire.gen.SessionInterruptReq, com.ccmonitor.mobile.core.wire.gen.Sent>("session-interrupt", { com.ccmonitor.mobile.core.wire.gen.SessionInterruptReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.Sent.ADAPTER }) {
        const val NAME: String = "session-interrupt"
    }

    /** `session-model`。 */
    object SessionModel : Command<com.ccmonitor.mobile.core.wire.gen.SessionModelReq, com.ccmonitor.mobile.core.wire.gen.ModelSwitched>("session-model", { com.ccmonitor.mobile.core.wire.gen.SessionModelReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.ModelSwitched.ADAPTER }) {
        const val NAME: String = "session-model"
    }

    /** `session-open`。 */
    object SessionOpen : Command<com.ccmonitor.mobile.core.wire.gen.SessionOpenReq, com.ccmonitor.mobile.core.wire.gen.Opened>("session-open", { com.ccmonitor.mobile.core.wire.gen.SessionOpenReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.Opened.ADAPTER }) {
        const val NAME: String = "session-open"
    }

    /** `session-open-dir`。 */
    object SessionOpenDir : Command<com.ccmonitor.mobile.core.wire.gen.OpenDirReq, com.ccmonitor.mobile.core.wire.gen.DirChecked>("session-open-dir", { com.ccmonitor.mobile.core.wire.gen.OpenDirReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.DirChecked.ADAPTER }) {
        const val NAME: String = "session-open-dir"
    }

    /** `session-open-facts`。 */
    object SessionOpenFacts : Command<com.ccmonitor.mobile.core.wire.gen.OpenFactsReq, com.ccmonitor.mobile.core.wire.gen.OpenFacts>("session-open-facts", { com.ccmonitor.mobile.core.wire.gen.OpenFactsReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.OpenFacts.ADAPTER }) {
        const val NAME: String = "session-open-facts"
    }

    /** `session-permission`。 */
    object SessionPermission : Command<com.ccmonitor.mobile.core.wire.gen.SessionPermissionReq, com.ccmonitor.mobile.core.wire.gen.ModeSwitched>("session-permission", { com.ccmonitor.mobile.core.wire.gen.SessionPermissionReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.ModeSwitched.ADAPTER }) {
        const val NAME: String = "session-permission"
    }

    /** `session-queue-cancel`。 */
    object SessionQueueCancel : Command<com.ccmonitor.mobile.core.wire.gen.SessionQueueCancelReq, com.ccmonitor.mobile.core.wire.gen.QueueCancelled>("session-queue-cancel", { com.ccmonitor.mobile.core.wire.gen.SessionQueueCancelReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.QueueCancelled.ADAPTER }) {
        const val NAME: String = "session-queue-cancel"
    }

    /** `session-queue-put`。 */
    object SessionQueuePut : Command<com.ccmonitor.mobile.core.wire.gen.SessionQueuePutReq, com.ccmonitor.mobile.core.wire.gen.QueuePut>("session-queue-put", { com.ccmonitor.mobile.core.wire.gen.SessionQueuePutReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.QueuePut.ADAPTER }) {
        const val NAME: String = "session-queue-put"
    }

    /** `session-queue-read`。 */
    object SessionQueueRead : Command<com.ccmonitor.mobile.core.wire.gen.SessionQueueReadReq, com.ccmonitor.mobile.core.wire.gen.QueueList>("session-queue-read", { com.ccmonitor.mobile.core.wire.gen.SessionQueueReadReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.QueueList.ADAPTER }) {
        const val NAME: String = "session-queue-read"
    }

    /** `session-restart`。 */
    object SessionRestart : Command<com.ccmonitor.mobile.core.wire.gen.RestartReq, com.ccmonitor.mobile.core.wire.gen.Restarted>("session-restart", { com.ccmonitor.mobile.core.wire.gen.RestartReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.Restarted.ADAPTER }) {
        const val NAME: String = "session-restart"
    }

    /** `session-rewind-options`。 */
    object SessionRewindOptions : Command<com.ccmonitor.mobile.core.wire.gen.SessionRewindOptionsReq, com.ccmonitor.mobile.core.wire.gen.RewindOptions>("session-rewind-options", { com.ccmonitor.mobile.core.wire.gen.SessionRewindOptionsReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RewindOptions.ADAPTER }) {
        const val NAME: String = "session-rewind-options"
    }

    /** `session-self`。 */
    object SessionSelf : Command<com.ccmonitor.mobile.core.wire.gen.NoArgs, com.ccmonitor.mobile.core.wire.gen.SessionSelf>("session-self", { com.ccmonitor.mobile.core.wire.gen.NoArgs.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.SessionSelf.ADAPTER }) {
        const val NAME: String = "session-self"
    }

    /** `session-terminals`。 */
    object SessionTerminals : Command<com.ccmonitor.mobile.core.wire.gen.SidReq, com.ccmonitor.mobile.core.wire.gen.Showing>("session-terminals", { com.ccmonitor.mobile.core.wire.gen.SidReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.Showing.ADAPTER }) {
        const val NAME: String = "session-terminals"
    }

    /** `session-window`。 */
    object SessionWindow : Command<com.ccmonitor.mobile.core.wire.gen.SessionWindowReq, com.ccmonitor.mobile.core.wire.gen.SessionWindow>("session-window", { com.ccmonitor.mobile.core.wire.gen.SessionWindowReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.SessionWindow.ADAPTER }) {
        const val NAME: String = "session-window"
    }

    /** `sessions-batch`。 */
    object SessionsBatch : Command<com.ccmonitor.mobile.core.wire.gen.SessionsBatchReq, com.ccmonitor.mobile.core.wire.gen.SessionsBatchReply>("sessions-batch", { com.ccmonitor.mobile.core.wire.gen.SessionsBatchReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.SessionsBatchReply.ADAPTER }) {
        const val NAME: String = "sessions-batch"
    }

    /** `sessions-needs`。 */
    object SessionsNeeds : Command<com.ccmonitor.mobile.core.wire.gen.SessionsNeedsReq, com.ccmonitor.mobile.core.wire.gen.NeedsList>("sessions-needs", { com.ccmonitor.mobile.core.wire.gen.SessionsNeedsReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.NeedsList.ADAPTER }) {
        const val NAME: String = "sessions-needs"
    }

    /** `sessions-needs-merge`。 */
    object SessionsNeedsMerge : Command<com.ccmonitor.mobile.core.wire.gen.NeedsMergeReq, com.ccmonitor.mobile.core.wire.gen.NeedsMerged>("sessions-needs-merge", { com.ccmonitor.mobile.core.wire.gen.NeedsMergeReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.NeedsMerged.ADAPTER }) {
        const val NAME: String = "sessions-needs-merge"
    }

    /** `sessions-stop`。 */
    object SessionsStop : Command<com.ccmonitor.mobile.core.wire.gen.SidsReq, com.ccmonitor.mobile.core.wire.gen.Batched>("sessions-stop", { com.ccmonitor.mobile.core.wire.gen.SidsReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.Batched.ADAPTER }) {
        const val NAME: String = "sessions-stop"
    }

    /** `sessions-where`。 */
    object SessionsWhere : Command<com.ccmonitor.mobile.core.wire.gen.SidsReq, com.ccmonitor.mobile.core.wire.gen.Whereabouts>("sessions-where", { com.ccmonitor.mobile.core.wire.gen.SidsReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.Whereabouts.ADAPTER }) {
        const val NAME: String = "sessions-where"
    }

    /** `set-launch`。 */
    object SetLaunch : Command<com.ccmonitor.mobile.core.wire.gen.SetLaunchReq, com.ccmonitor.mobile.core.wire.gen.SetLaunched>("set-launch", { com.ccmonitor.mobile.core.wire.gen.SetLaunchReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.SetLaunched.ADAPTER }) {
        const val NAME: String = "set-launch"
    }

    /** `set-plan`。 */
    object SetPlan : Command<com.ccmonitor.mobile.core.wire.gen.SetPlanReq, com.ccmonitor.mobile.core.wire.gen.SetPlan>("set-plan", { com.ccmonitor.mobile.core.wire.gen.SetPlanReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.SetPlan.ADAPTER }) {
        const val NAME: String = "set-plan"
    }

    /** `skill-install-apply`。 */
    object SkillInstallApply : Command<com.ccmonitor.mobile.core.wire.gen.SkillApplyReq, com.ccmonitor.mobile.core.wire.gen.SkillApplied>("skill-install-apply", { com.ccmonitor.mobile.core.wire.gen.SkillApplyReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.SkillApplied.ADAPTER }) {
        const val NAME: String = "skill-install-apply"
    }

    /** `skill-install-plan`。 */
    object SkillInstallPlan : Command<com.ccmonitor.mobile.core.wire.gen.SkillPlanReq, com.ccmonitor.mobile.core.wire.gen.SkillPlan>("skill-install-plan", { com.ccmonitor.mobile.core.wire.gen.SkillPlanReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.SkillPlan.ADAPTER }) {
        const val NAME: String = "skill-install-plan"
    }

    /** `skill-read`。 */
    object SkillRead : Command<com.ccmonitor.mobile.core.wire.gen.SkillReadReq, com.ccmonitor.mobile.core.wire.gen.SkillRead>("skill-read", { com.ccmonitor.mobile.core.wire.gen.SkillReadReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.SkillRead.ADAPTER }) {
        const val NAME: String = "skill-read"
    }

    /** `ssh-config-import`。 */
    object SshConfigImport : Command<com.ccmonitor.mobile.core.wire.gen.SshImportReq, com.ccmonitor.mobile.core.wire.gen.SshImported>("ssh-config-import", { com.ccmonitor.mobile.core.wire.gen.SshImportReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.SshImported.ADAPTER }) {
        const val NAME: String = "ssh-config-import"
    }

    /** `stream-watch`。 */
    object StreamWatch : Command<com.ccmonitor.mobile.core.wire.gen.WatchAsk, com.ccmonitor.mobile.core.wire.gen.WatchReply>("stream-watch", { com.ccmonitor.mobile.core.wire.gen.WatchAsk.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.WatchReply.ADAPTER }) {
        const val NAME: String = "stream-watch"
    }

    /** `tasks-list`。 */
    object TasksList : Command<com.ccmonitor.mobile.core.wire.gen.TasksListReq, com.ccmonitor.mobile.core.wire.gen.TasksList>("tasks-list", { com.ccmonitor.mobile.core.wire.gen.TasksListReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.TasksList.ADAPTER }) {
        const val NAME: String = "tasks-list"
    }

    /** `terminal-end`。 */
    object TerminalEnd : Command<com.ccmonitor.mobile.core.wire.gen.TerminalEndReq, com.ccmonitor.mobile.core.wire.gen.TerminalEnded>("terminal-end", { com.ccmonitor.mobile.core.wire.gen.TerminalEndReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.TerminalEnded.ADAPTER }) {
        const val NAME: String = "terminal-end"
    }

    /** `terminal-follow`。 */
    object TerminalFollow : Command<com.ccmonitor.mobile.core.wire.gen.FollowReq, com.ccmonitor.mobile.core.wire.gen.Raw>("terminal-follow", { com.ccmonitor.mobile.core.wire.gen.FollowReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RawJson }) {
        const val NAME: String = "terminal-follow"
    }

    /** `terminal-follow-ack`。 */
    object TerminalFollowAck : Command<com.ccmonitor.mobile.core.wire.gen.AckReq, com.ccmonitor.mobile.core.wire.gen.Raw>("terminal-follow-ack", { com.ccmonitor.mobile.core.wire.gen.AckReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RawJson }) {
        const val NAME: String = "terminal-follow-ack"
    }

    /** `terminal-input`。 */
    object TerminalInput : Command<com.ccmonitor.mobile.core.wire.gen.TerminalInputReq, com.ccmonitor.mobile.core.wire.gen.Sent>("terminal-input", { com.ccmonitor.mobile.core.wire.gen.TerminalInputReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.Sent.ADAPTER }) {
        const val NAME: String = "terminal-input"
    }

    /** `terminal-preview`。 */
    object TerminalPreview : Command<com.ccmonitor.mobile.core.wire.gen.TerminalPreviewReq, com.ccmonitor.mobile.core.wire.gen.Preview>("terminal-preview", { com.ccmonitor.mobile.core.wire.gen.TerminalPreviewReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.Preview.ADAPTER }) {
        const val NAME: String = "terminal-preview"
    }

    /** `terminal-release-input`。 */
    object TerminalReleaseInput : Command<com.ccmonitor.mobile.core.wire.gen.TerminalHoldReq, com.ccmonitor.mobile.core.wire.gen.InputNow>("terminal-release-input", { com.ccmonitor.mobile.core.wire.gen.TerminalHoldReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.InputNow.ADAPTER }) {
        const val NAME: String = "terminal-release-input"
    }

    /** `terminal-ssh`。 */
    object TerminalSsh : Command<com.ccmonitor.mobile.core.wire.gen.TerminalSshReq, com.ccmonitor.mobile.core.wire.gen.SshCommand>("terminal-ssh", { com.ccmonitor.mobile.core.wire.gen.TerminalSshReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.SshCommand.ADAPTER }) {
        const val NAME: String = "terminal-ssh"
    }

    /** `terminal-take-input`。 */
    object TerminalTakeInput : Command<com.ccmonitor.mobile.core.wire.gen.TerminalHoldReq, com.ccmonitor.mobile.core.wire.gen.InputNow>("terminal-take-input", { com.ccmonitor.mobile.core.wire.gen.TerminalHoldReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.InputNow.ADAPTER }) {
        const val NAME: String = "terminal-take-input"
    }

    /** `terminal-unfollow`。 */
    object TerminalUnfollow : Command<com.ccmonitor.mobile.core.wire.gen.UnfollowReq, com.ccmonitor.mobile.core.wire.gen.Raw>("terminal-unfollow", { com.ccmonitor.mobile.core.wire.gen.UnfollowReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RawJson }) {
        const val NAME: String = "terminal-unfollow"
    }

    /** `terminals-board`。 */
    object TerminalsBoard : Command<com.ccmonitor.mobile.core.wire.gen.BoardReq, com.ccmonitor.mobile.core.wire.gen.TerminalsBoard>("terminals-board", { com.ccmonitor.mobile.core.wire.gen.BoardReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.TerminalsBoard.ADAPTER }) {
        const val NAME: String = "terminals-board"
    }

    /** `terminals-board-here`。 */
    object TerminalsBoardHere : Command<com.ccmonitor.mobile.core.wire.gen.BoardHereReq, com.ccmonitor.mobile.core.wire.gen.BoardHere>("terminals-board-here", { com.ccmonitor.mobile.core.wire.gen.BoardHereReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.BoardHere.ADAPTER }) {
        const val NAME: String = "terminals-board-here"
    }

    /** `terminals-end`。 */
    object TerminalsEnd : Command<com.ccmonitor.mobile.core.wire.gen.TerminalsEndReq, com.ccmonitor.mobile.core.wire.gen.TerminalsEndReply>("terminals-end", { com.ccmonitor.mobile.core.wire.gen.TerminalsEndReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.TerminalsEndReply.ADAPTER }) {
        const val NAME: String = "terminals-end"
    }

    /** `terminals-list`。 */
    object TerminalsList : Command<com.ccmonitor.mobile.core.wire.gen.TerminalsListReq, com.ccmonitor.mobile.core.wire.gen.TerminalsList>("terminals-list", { com.ccmonitor.mobile.core.wire.gen.TerminalsListReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.TerminalsList.ADAPTER }) {
        const val NAME: String = "terminals-list"
    }

    /** `transfer-copy`。 */
    object TransferCopy : Command<com.ccmonitor.mobile.core.wire.gen.TransferCopyReq, com.ccmonitor.mobile.core.wire.gen.BatchReply>("transfer-copy", { com.ccmonitor.mobile.core.wire.gen.TransferCopyReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.BatchReply.ADAPTER }) {
        const val NAME: String = "transfer-copy"
    }

    /** `transfer-download`。 */
    object TransferDownload : Command<com.ccmonitor.mobile.core.wire.gen.TransferDownloadReq, com.ccmonitor.mobile.core.wire.gen.TransferOpened>("transfer-download", { com.ccmonitor.mobile.core.wire.gen.TransferDownloadReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.TransferOpened.ADAPTER }) {
        const val NAME: String = "transfer-download"
    }

    /** `transfer-start`。 */
    object TransferStart : Command<com.ccmonitor.mobile.core.wire.gen.TransferStartReq, com.ccmonitor.mobile.core.wire.gen.Raw>("transfer-start", { com.ccmonitor.mobile.core.wire.gen.TransferStartReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RawJson }) {
        const val NAME: String = "transfer-start"
    }

    /** `transfer-stop`。 */
    object TransferStop : Command<com.ccmonitor.mobile.core.wire.gen.TransferStopReq, com.ccmonitor.mobile.core.wire.gen.Raw>("transfer-stop", { com.ccmonitor.mobile.core.wire.gen.TransferStopReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.RawJson }) {
        const val NAME: String = "transfer-stop"
    }

    /** `transfer-upload`。 */
    object TransferUpload : Command<com.ccmonitor.mobile.core.wire.gen.TransferUploadReq, com.ccmonitor.mobile.core.wire.gen.TransferOpened>("transfer-upload", { com.ccmonitor.mobile.core.wire.gen.TransferUploadReq.ADAPTER }, { com.ccmonitor.mobile.core.wire.gen.TransferOpened.ADAPTER }) {
        const val NAME: String = "transfer-upload"
    }

    /** 命令名 ⇒ 那一条。 */
    val ALL: Map<String, Command<*, *>> by lazy {
        listOf<Command<*, *>>(
            AccountsAdd,
            AccountsInit,
            AccountsList,
            AccountsLoginCmd,
            AccountsMcpPick,
            AccountsMcpRead,
            AccountsMcpRemove,
            AccountsMcpSync,
            AccountsRemove,
            AccountsRepair,
            AccountsRollback,
            AccountsSetDefault,
            AccountsVerify,
            AgentHomeCheck,
            AliasesBlockInstall,
            AliasesBlockRemove,
            AliasesBlockRender,
            AliasesRead,
            ApikeyKeySet,
            AssetsCatalog,
            AssetsCatalogMerge,
            AssetsSync,
            AuthorizedKeysAdd,
            BackendLog,
            BookmarksEdit,
            BookmarksRead,
            BusCommit,
            BusInbox,
            BusList,
            BusPeek,
            BusSend,
            BusUnread,
            Cancel,
            CcBusInstall,
            CcBusInstallState,
            CcmPrint,
            CcmProbe,
            CcmUserPath,
            ChoresMark,
            ChoresMerge,
            DataReport,
            DeployInstall,
            DeployUninstall,
            DriftMerge,
            DriftReport,
            ExitPolicyRead,
            ExitPolicySet,
            ExtHubApply,
            ExtHubPreview,
            ExtList,
            ExtListHere,
            ExtNoteSet,
            ExtUninstallApply,
            ExtUninstallPreview,
            FilesBatchStop,
            FilesBrowse,
            FilesChmod,
            FilesCommitText,
            FilesCommitUpload,
            FilesCopy,
            FilesCreate,
            FilesDelete,
            FilesDeleteSession,
            FilesExtract,
            FilesFind,
            FilesGrep,
            FilesHome,
            FilesIndexRebuild,
            FilesIndexStatus,
            FilesLs,
            FilesMkdir,
            FilesReadChunk,
            FilesReadText,
            FilesRename,
            FilesSize,
            FilesStageChunk,
            FilesStat,
            FilesWriteText,
            FirstRun,
            FootprintReport,
            ForwardList,
            ForwardStart,
            ForwardStop,
            GroupsHere,
            GroupsRead,
            GroupsWrite,
            HistoryAnnotate,
            HistoryBranch,
            HistoryBrowse,
            HistoryFacts,
            HistoryFind,
            HistoryForget,
            HistoryImage,
            HistoryIndex,
            HistoryLines,
            HistoryList,
            HistoryListing,
            HistoryPage,
            HistoryRead,
            HistoryRecord,
            HistoryRun,
            HistorySearch,
            HistorySearchMerge,
            HistoryTail,
            HistoryTurns,
            HistoryUserInputs,
            HooksDiag,
            HostKeysForget,
            LastSeenRead,
            LastSeenWrite,
            LoginCancel,
            LoginFinish,
            LoginStart,
            MachineFind,
            MachineInterrupts,
            MachineMenu,
            MachineRead,
            MachineStop,
            MachineWake,
            MachinesRead,
            MachinesSet,
            MachinesTry,
            McpRead,
            McpSyncApply,
            McpSyncPreview,
            McpSyncSource,
            MonitorBoard,
            MonitorBoardHere,
            NeedsAnswer,
            NeedsAsk,
            Ping,
            PlaceVerdict,
            PlanAck,
            PlanAsk,
            PlanCellView,
            PlanCommand,
            PlanFiles,
            PlanList,
            PlanNew,
            PlanPbCheck,
            PlanRead,
            PlanRefs,
            PlanReturn,
            PlanUnack,
            PowershellPolicySet,
            PrefsRead,
            PrefsSet,
            ProfilesBases,
            ProfilesFromLine,
            ProfilesImpact,
            ProfilesRead,
            ProfilesResolve,
            ProfilesWrite,
            ProseHl,
            PubkeyPush,
            QuotaProbe,
            QuotaRead,
            QuotaWarm,
            RemoteProbe,
            Resync,
            RotationFacts,
            RotationForNew,
            RotationReload,
            RotationSettings,
            RotationStrategies,
            RotationStrategyAdd,
            RotationStrategyCopy,
            RotationStrategyDelete,
            RotationStrategyRename,
            RotationStrategyTz,
            RotationTemp,
            RotationTry,
            RotationUse,
            RotationUseNow,
            RotationView,
            SessionAccount,
            SessionArrive,
            SessionAttachmentCheck,
            SessionAttachmentCommit,
            SessionAttachmentDrop,
            SessionCommands,
            SessionFork,
            SessionInterrupt,
            SessionModel,
            SessionOpen,
            SessionOpenDir,
            SessionOpenFacts,
            SessionPermission,
            SessionQueueCancel,
            SessionQueuePut,
            SessionQueueRead,
            SessionRestart,
            SessionRewindOptions,
            SessionSelf,
            SessionTerminals,
            SessionWindow,
            SessionsBatch,
            SessionsNeeds,
            SessionsNeedsMerge,
            SessionsStop,
            SessionsWhere,
            SetLaunch,
            SetPlan,
            SkillInstallApply,
            SkillInstallPlan,
            SkillRead,
            SshConfigImport,
            StreamWatch,
            TasksList,
            TerminalEnd,
            TerminalFollow,
            TerminalFollowAck,
            TerminalInput,
            TerminalPreview,
            TerminalReleaseInput,
            TerminalSsh,
            TerminalTakeInput,
            TerminalUnfollow,
            TerminalsBoard,
            TerminalsBoardHere,
            TerminalsEnd,
            TerminalsList,
            TransferCopy,
            TransferDownload,
            TransferStart,
            TransferStop,
            TransferUpload,
        ).associateBy { it.name }
    }

    /** 每个不带类型参数的类型 ⇒ 它的编解码（按类取，不经反射）。 */
    val BY_CLASS: Map<Class<*>, JsonAdapter<*>> by lazy {
        mapOf(
            com.ccmonitor.mobile.core.wire.gen.AccountAddArgs::class.java to com.ccmonitor.mobile.core.wire.gen.AccountAddArgs.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AccountAsk::class.java to com.ccmonitor.mobile.core.wire.gen.AccountAsk.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AccountChange::class.java to com.ccmonitor.mobile.core.wire.gen.AccountChange.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AccountInitArgs::class.java to com.ccmonitor.mobile.core.wire.gen.AccountInitArgs.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AccountKind::class.java to com.ccmonitor.mobile.core.wire.gen.AccountKind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AccountLoginCmd::class.java to com.ccmonitor.mobile.core.wire.gen.AccountLoginCmd.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AccountMcpChoice::class.java to com.ccmonitor.mobile.core.wire.gen.AccountMcpChoice.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AccountMcpConflict::class.java to com.ccmonitor.mobile.core.wire.gen.AccountMcpConflict.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AccountMcpNameArgs::class.java to com.ccmonitor.mobile.core.wire.gen.AccountMcpNameArgs.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AccountMcpPickArgs::class.java to com.ccmonitor.mobile.core.wire.gen.AccountMcpPickArgs.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AccountMcpSyncArgs::class.java to com.ccmonitor.mobile.core.wire.gen.AccountMcpSyncArgs.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AccountMcpView::class.java to com.ccmonitor.mobile.core.wire.gen.AccountMcpView.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AccountNameArgs::class.java to com.ccmonitor.mobile.core.wire.gen.AccountNameArgs.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AccountRef::class.java to com.ccmonitor.mobile.core.wire.gen.AccountRef.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AccountRemoveArgs::class.java to com.ccmonitor.mobile.core.wire.gen.AccountRemoveArgs.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AccountRepairArgs::class.java to com.ccmonitor.mobile.core.wire.gen.AccountRepairArgs.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AccountRollbackArgs::class.java to com.ccmonitor.mobile.core.wire.gen.AccountRollbackArgs.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AccountRow::class.java to com.ccmonitor.mobile.core.wire.gen.AccountRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AccountUnavailable::class.java to com.ccmonitor.mobile.core.wire.gen.AccountUnavailable.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AccountsList::class.java to com.ccmonitor.mobile.core.wire.gen.AccountsList.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AccountsListReq::class.java to com.ccmonitor.mobile.core.wire.gen.AccountsListReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AckReq::class.java to com.ccmonitor.mobile.core.wire.gen.AckReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ActionTag::class.java to com.ccmonitor.mobile.core.wire.gen.ActionTag.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AgentFacts::class.java to com.ccmonitor.mobile.core.wire.gen.AgentFacts.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AgentHomeCheck::class.java to com.ccmonitor.mobile.core.wire.gen.AgentHomeCheck.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AgentHomeCheckReq::class.java to com.ccmonitor.mobile.core.wire.gen.AgentHomeCheckReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AgentHomeState::class.java to com.ccmonitor.mobile.core.wire.gen.AgentHomeState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AgentPick::class.java to com.ccmonitor.mobile.core.wire.gen.AgentPick.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AlertKind::class.java to com.ccmonitor.mobile.core.wire.gen.AlertKind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AliasBlockState::class.java to com.ccmonitor.mobile.core.wire.gen.AliasBlockState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AliasChange::class.java to com.ccmonitor.mobile.core.wire.gen.AliasChange.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AliasListing::class.java to com.ccmonitor.mobile.core.wire.gen.AliasListing.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AliasNameClash::class.java to com.ccmonitor.mobile.core.wire.gen.AliasNameClash.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AliasStartupFile::class.java to com.ccmonitor.mobile.core.wire.gen.AliasStartupFile.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AliasWins::class.java to com.ccmonitor.mobile.core.wire.gen.AliasWins.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AliasesReadReq::class.java to com.ccmonitor.mobile.core.wire.gen.AliasesReadReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Allow::class.java to com.ccmonitor.mobile.core.wire.gen.Allow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AnnotateReq::class.java to com.ccmonitor.mobile.core.wire.gen.AnnotateReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Annotated::class.java to com.ccmonitor.mobile.core.wire.gen.Annotated.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Answer::class.java to com.ccmonitor.mobile.core.wire.gen.Answer.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AnswerReq::class.java to com.ccmonitor.mobile.core.wire.gen.AnswerReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AnswerResult::class.java to com.ccmonitor.mobile.core.wire.gen.AnswerResult.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Answered::class.java to com.ccmonitor.mobile.core.wire.gen.Answered.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AnsweredRow::class.java to com.ccmonitor.mobile.core.wire.gen.AnsweredRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ApiReason::class.java to com.ccmonitor.mobile.core.wire.gen.ApiReason.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Arrival::class.java to com.ccmonitor.mobile.core.wire.gen.Arrival.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ArriveAsk::class.java to com.ccmonitor.mobile.core.wire.gen.ArriveAsk.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ArriveBy::class.java to com.ccmonitor.mobile.core.wire.gen.ArriveBy.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ArriveEnd::class.java to com.ccmonitor.mobile.core.wire.gen.ArriveEnd.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AskKind::class.java to com.ccmonitor.mobile.core.wire.gen.AskKind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AskOption::class.java to com.ccmonitor.mobile.core.wire.gen.AskOption.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AskPick::class.java to com.ccmonitor.mobile.core.wire.gen.AskPick.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AskQuestion::class.java to com.ccmonitor.mobile.core.wire.gen.AskQuestion.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AskReply::class.java to com.ccmonitor.mobile.core.wire.gen.AskReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AskReq::class.java to com.ccmonitor.mobile.core.wire.gen.AskReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AssetsCatalogMergeReq::class.java to com.ccmonitor.mobile.core.wire.gen.AssetsCatalogMergeReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AssetsCatalogReq::class.java to com.ccmonitor.mobile.core.wire.gen.AssetsCatalogReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AssetsSyncReq::class.java to com.ccmonitor.mobile.core.wire.gen.AssetsSyncReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AssetsSyncRow::class.java to com.ccmonitor.mobile.core.wire.gen.AssetsSyncRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AssetsSynced::class.java to com.ccmonitor.mobile.core.wire.gen.AssetsSynced.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AttachAct::class.java to com.ccmonitor.mobile.core.wire.gen.AttachAct.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AttachCardPhase::class.java to com.ccmonitor.mobile.core.wire.gen.AttachCardPhase.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AttachFace::class.java to com.ccmonitor.mobile.core.wire.gen.AttachFace.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AttachFaceCard::class.java to com.ccmonitor.mobile.core.wire.gen.AttachFaceCard.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AttachFaceNote::class.java to com.ccmonitor.mobile.core.wire.gen.AttachFaceNote.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AttachItem::class.java to com.ccmonitor.mobile.core.wire.gen.AttachItem.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AttachLocal::class.java to com.ccmonitor.mobile.core.wire.gen.AttachLocal.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AttachRefused::class.java to com.ccmonitor.mobile.core.wire.gen.AttachRefused.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AttachVerdict::class.java to com.ccmonitor.mobile.core.wire.gen.AttachVerdict.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AttachWant::class.java to com.ccmonitor.mobile.core.wire.gen.AttachWant.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AttachWantCard::class.java to com.ccmonitor.mobile.core.wire.gen.AttachWantCard.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Attached::class.java to com.ccmonitor.mobile.core.wire.gen.Attached.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AttachedKind::class.java to com.ccmonitor.mobile.core.wire.gen.AttachedKind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Attachment::class.java to com.ccmonitor.mobile.core.wire.gen.Attachment.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AutoAnswer::class.java to com.ccmonitor.mobile.core.wire.gen.AutoAnswer.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AutoChoice::class.java to com.ccmonitor.mobile.core.wire.gen.AutoChoice.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.AutoLaunchConfig::class.java to com.ccmonitor.mobile.core.wire.gen.AutoLaunchConfig.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BackendLogReq::class.java to com.ccmonitor.mobile.core.wire.gen.BackendLogReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BackendStatus::class.java to com.ccmonitor.mobile.core.wire.gen.BackendStatus.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Badge::class.java to com.ccmonitor.mobile.core.wire.gen.Badge.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BatchAnswer::class.java to com.ccmonitor.mobile.core.wire.gen.BatchAnswer.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BatchEdge::class.java to com.ccmonitor.mobile.core.wire.gen.BatchEdge.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BatchKeep::class.java to com.ccmonitor.mobile.core.wire.gen.BatchKeep.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BatchLine::class.java to com.ccmonitor.mobile.core.wire.gen.BatchLine.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BatchOutcome::class.java to com.ccmonitor.mobile.core.wire.gen.BatchOutcome.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BatchReply::class.java to com.ccmonitor.mobile.core.wire.gen.BatchReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Batched::class.java to com.ccmonitor.mobile.core.wire.gen.Batched.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BgTask::class.java to com.ccmonitor.mobile.core.wire.gen.BgTask.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Block::class.java to com.ccmonitor.mobile.core.wire.gen.Block.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BlockDone::class.java to com.ccmonitor.mobile.core.wire.gen.BlockDone.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BlockKind::class.java to com.ccmonitor.mobile.core.wire.gen.BlockKind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BlockReq::class.java to com.ccmonitor.mobile.core.wire.gen.BlockReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BlockText::class.java to com.ccmonitor.mobile.core.wire.gen.BlockText.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BoardDown::class.java to com.ccmonitor.mobile.core.wire.gen.BoardDown.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BoardFilter::class.java to com.ccmonitor.mobile.core.wire.gen.BoardFilter.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BoardGroup::class.java to com.ccmonitor.mobile.core.wire.gen.BoardGroup.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BoardHere::class.java to com.ccmonitor.mobile.core.wire.gen.BoardHere.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BoardHereReq::class.java to com.ccmonitor.mobile.core.wire.gen.BoardHereReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BoardReq::class.java to com.ccmonitor.mobile.core.wire.gen.BoardReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BoardRow::class.java to com.ccmonitor.mobile.core.wire.gen.BoardRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BoardRowCanEnd::class.java to com.ccmonitor.mobile.core.wire.gen.BoardRowCanEnd.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BoardRowCanEndObjValue::class.java to com.ccmonitor.mobile.core.wire.gen.BoardRowCanEndObjValue.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BoardTag::class.java to com.ccmonitor.mobile.core.wire.gen.BoardTag.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Bookmarks::class.java to com.ccmonitor.mobile.core.wire.gen.Bookmarks.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BookmarksEditReq::class.java to com.ccmonitor.mobile.core.wire.gen.BookmarksEditReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BookmarksEditReqOp::class.java to com.ccmonitor.mobile.core.wire.gen.BookmarksEditReqOp.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BookmarksReadReq::class.java to com.ccmonitor.mobile.core.wire.gen.BookmarksReadReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Branch::class.java to com.ccmonitor.mobile.core.wire.gen.Branch.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BranchHeld::class.java to com.ccmonitor.mobile.core.wire.gen.BranchHeld.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BranchReq::class.java to com.ccmonitor.mobile.core.wire.gen.BranchReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BrowseReply::class.java to com.ccmonitor.mobile.core.wire.gen.BrowseReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BrowseReq::class.java to com.ccmonitor.mobile.core.wire.gen.BrowseReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BusAct::class.java to com.ccmonitor.mobile.core.wire.gen.BusAct.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BusAgent::class.java to com.ccmonitor.mobile.core.wire.gen.BusAgent.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BusCommitReq::class.java to com.ccmonitor.mobile.core.wire.gen.BusCommitReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BusCommitted::class.java to com.ccmonitor.mobile.core.wire.gen.BusCommitted.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BusEnvelope::class.java to com.ccmonitor.mobile.core.wire.gen.BusEnvelope.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BusEnvelopeReq::class.java to com.ccmonitor.mobile.core.wire.gen.BusEnvelopeReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BusInbox::class.java to com.ccmonitor.mobile.core.wire.gen.BusInbox.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BusInboxItem::class.java to com.ccmonitor.mobile.core.wire.gen.BusInboxItem.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BusInboxReq::class.java to com.ccmonitor.mobile.core.wire.gen.BusInboxReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BusList::class.java to com.ccmonitor.mobile.core.wire.gen.BusList.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BusPeekReq::class.java to com.ccmonitor.mobile.core.wire.gen.BusPeekReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BusPeeked::class.java to com.ccmonitor.mobile.core.wire.gen.BusPeeked.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BusSent::class.java to com.ccmonitor.mobile.core.wire.gen.BusSent.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BusUnread::class.java to com.ccmonitor.mobile.core.wire.gen.BusUnread.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.BusUnreadReq::class.java to com.ccmonitor.mobile.core.wire.gen.BusUnreadReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.CallEnd::class.java to com.ccmonitor.mobile.core.wire.gen.CallEnd.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Can::class.java to com.ccmonitor.mobile.core.wire.gen.Can.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.CanClose::class.java to com.ccmonitor.mobile.core.wire.gen.CanClose.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.CanDelete::class.java to com.ccmonitor.mobile.core.wire.gen.CanDelete.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.CanResume::class.java to com.ccmonitor.mobile.core.wire.gen.CanResume.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.CanSay::class.java to com.ccmonitor.mobile.core.wire.gen.CanSay.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.CanTerm::class.java to com.ccmonitor.mobile.core.wire.gen.CanTerm.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.CancelReq::class.java to com.ccmonitor.mobile.core.wire.gen.CancelReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.CaptureOpts::class.java to com.ccmonitor.mobile.core.wire.gen.CaptureOpts.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Captured::class.java to com.ccmonitor.mobile.core.wire.gen.Captured.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.CatalogAsset::class.java to com.ccmonitor.mobile.core.wire.gen.CatalogAsset.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.CatalogFrom::class.java to com.ccmonitor.mobile.core.wire.gen.CatalogFrom.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.CatalogIn::class.java to com.ccmonitor.mobile.core.wire.gen.CatalogIn.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.CatalogMachine::class.java to com.ccmonitor.mobile.core.wire.gen.CatalogMachine.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.CatalogNote::class.java to com.ccmonitor.mobile.core.wire.gen.CatalogNote.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.CatalogReply::class.java to com.ccmonitor.mobile.core.wire.gen.CatalogReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.CatalogRow::class.java to com.ccmonitor.mobile.core.wire.gen.CatalogRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.CcBusInstalled::class.java to com.ccmonitor.mobile.core.wire.gen.CcBusInstalled.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.CcBusState::class.java to com.ccmonitor.mobile.core.wire.gen.CcBusState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.CcmCard::class.java to com.ccmonitor.mobile.core.wire.gen.CcmCard.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.CcmLine::class.java to com.ccmonitor.mobile.core.wire.gen.CcmLine.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.CcmPrintReq::class.java to com.ccmonitor.mobile.core.wire.gen.CcmPrintReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Changed::class.java to com.ccmonitor.mobile.core.wire.gen.Changed.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ChangedFile::class.java to com.ccmonitor.mobile.core.wire.gen.ChangedFile.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ChatsColumn::class.java to com.ccmonitor.mobile.core.wire.gen.ChatsColumn.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ChatsColumnAsk::class.java to com.ccmonitor.mobile.core.wire.gen.ChatsColumnAsk.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.CheckLevel::class.java to com.ccmonitor.mobile.core.wire.gen.CheckLevel.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ChildRunTag::class.java to com.ccmonitor.mobile.core.wire.gen.ChildRunTag.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ChmodItem::class.java to com.ccmonitor.mobile.core.wire.gen.ChmodItem.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Chore::class.java to com.ccmonitor.mobile.core.wire.gen.Chore.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ChoreAction::class.java to com.ccmonitor.mobile.core.wire.gen.ChoreAction.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ChoreCounts::class.java to com.ccmonitor.mobile.core.wire.gen.ChoreCounts.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ChoreKind::class.java to com.ccmonitor.mobile.core.wire.gen.ChoreKind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ChoreState::class.java to com.ccmonitor.mobile.core.wire.gen.ChoreState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ChoresAt::class.java to com.ccmonitor.mobile.core.wire.gen.ChoresAt.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ChoresMarkReq::class.java to com.ccmonitor.mobile.core.wire.gen.ChoresMarkReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ChoresMarks::class.java to com.ccmonitor.mobile.core.wire.gen.ChoresMarks.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ChoresMergeReq::class.java to com.ccmonitor.mobile.core.wire.gen.ChoresMergeReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ChoresMerged::class.java to com.ccmonitor.mobile.core.wire.gen.ChoresMerged.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ChunkBytes::class.java to com.ccmonitor.mobile.core.wire.gen.ChunkBytes.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ChunkBytesObjValue::class.java to com.ccmonitor.mobile.core.wire.gen.ChunkBytesObjValue.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ClashChoice::class.java to com.ccmonitor.mobile.core.wire.gen.ClashChoice.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ClashRow::class.java to com.ccmonitor.mobile.core.wire.gen.ClashRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ColumnAt::class.java to com.ccmonitor.mobile.core.wire.gen.ColumnAt.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ColumnFork::class.java to com.ccmonitor.mobile.core.wire.gen.ColumnFork.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ColumnHost::class.java to com.ccmonitor.mobile.core.wire.gen.ColumnHost.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ColumnLine::class.java to com.ccmonitor.mobile.core.wire.gen.ColumnLine.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ColumnRow::class.java to com.ccmonitor.mobile.core.wire.gen.ColumnRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ColumnSection::class.java to com.ccmonitor.mobile.core.wire.gen.ColumnSection.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.CommandScope::class.java to com.ccmonitor.mobile.core.wire.gen.CommandScope.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Compact::class.java to com.ccmonitor.mobile.core.wire.gen.Compact.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ConfigEdit::class.java to com.ccmonitor.mobile.core.wire.gen.ConfigEdit.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ConfigHost::class.java to com.ccmonitor.mobile.core.wire.gen.ConfigHost.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ConfigSurfaceReport::class.java to com.ccmonitor.mobile.core.wire.gen.ConfigSurfaceReport.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Conflict::class.java to com.ccmonitor.mobile.core.wire.gen.Conflict.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Conflicts::class.java to com.ccmonitor.mobile.core.wire.gen.Conflicts.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ConnTestResult::class.java to com.ccmonitor.mobile.core.wire.gen.ConnTestResult.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.CopyFrom::class.java to com.ccmonitor.mobile.core.wire.gen.CopyFrom.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.CopyItem::class.java to com.ccmonitor.mobile.core.wire.gen.CopyItem.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Crumb::class.java to com.ccmonitor.mobile.core.wire.gen.Crumb.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Cursor::class.java to com.ccmonitor.mobile.core.wire.gen.Cursor.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.DataClass::class.java to com.ccmonitor.mobile.core.wire.gen.DataClass.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.DataPathInfo::class.java to com.ccmonitor.mobile.core.wire.gen.DataPathInfo.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.DataPathsResponse::class.java to com.ccmonitor.mobile.core.wire.gen.DataPathsResponse.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.DataReport::class.java to com.ccmonitor.mobile.core.wire.gen.DataReport.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Decide::class.java to com.ccmonitor.mobile.core.wire.gen.Decide.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.DeleteItem::class.java to com.ccmonitor.mobile.core.wire.gen.DeleteItem.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.DeployAct::class.java to com.ccmonitor.mobile.core.wire.gen.DeployAct.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.DeployEnd::class.java to com.ccmonitor.mobile.core.wire.gen.DeployEnd.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.DeployInstallReq::class.java to com.ccmonitor.mobile.core.wire.gen.DeployInstallReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.DeployUninstallReq::class.java to com.ccmonitor.mobile.core.wire.gen.DeployUninstallReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.DiagnosticsConfig::class.java to com.ccmonitor.mobile.core.wire.gen.DiagnosticsConfig.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.DiagnosticsReport::class.java to com.ccmonitor.mobile.core.wire.gen.DiagnosticsReport.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.DialAck::class.java to com.ccmonitor.mobile.core.wire.gen.DialAck.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.DialPlan::class.java to com.ccmonitor.mobile.core.wire.gen.DialPlan.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.DialRequest::class.java to com.ccmonitor.mobile.core.wire.gen.DialRequest.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.DialUse::class.java to com.ccmonitor.mobile.core.wire.gen.DialUse.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.DialWhy::class.java to com.ccmonitor.mobile.core.wire.gen.DialWhy.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.DirChecked::class.java to com.ccmonitor.mobile.core.wire.gen.DirChecked.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Doc::class.java to com.ccmonitor.mobile.core.wire.gen.Doc.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.DownloadItem::class.java to com.ccmonitor.mobile.core.wire.gen.DownloadItem.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Draw::class.java to com.ccmonitor.mobile.core.wire.gen.Draw.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.DriftEntry::class.java to com.ccmonitor.mobile.core.wire.gen.DriftEntry.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.DriftFaceReport::class.java to com.ccmonitor.mobile.core.wire.gen.DriftFaceReport.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.DriftMerge::class.java to com.ccmonitor.mobile.core.wire.gen.DriftMerge.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.DriftOn::class.java to com.ccmonitor.mobile.core.wire.gen.DriftOn.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.DriftReport::class.java to com.ccmonitor.mobile.core.wire.gen.DriftReport.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ElemKey::class.java to com.ccmonitor.mobile.core.wire.gen.ElemKey.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Endpoint::class.java to com.ccmonitor.mobile.core.wire.gen.Endpoint.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Entry::class.java to com.ccmonitor.mobile.core.wire.gen.Entry.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.EnvTier::class.java to com.ccmonitor.mobile.core.wire.gen.EnvTier.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExitPolicy::class.java to com.ccmonitor.mobile.core.wire.gen.ExitPolicy.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExitPolicyReadReq::class.java to com.ccmonitor.mobile.core.wire.gen.ExitPolicyReadReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExitPolicySetReq::class.java to com.ccmonitor.mobile.core.wire.gen.ExitPolicySetReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExitPolicyState::class.java to com.ccmonitor.mobile.core.wire.gen.ExitPolicyState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExtBring::class.java to com.ccmonitor.mobile.core.wire.gen.ExtBring.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExtCard::class.java to com.ccmonitor.mobile.core.wire.gen.ExtCard.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExtCell::class.java to com.ccmonitor.mobile.core.wire.gen.ExtCell.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExtDetail::class.java to com.ccmonitor.mobile.core.wire.gen.ExtDetail.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExtDone::class.java to com.ccmonitor.mobile.core.wire.gen.ExtDone.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExtKind::class.java to com.ccmonitor.mobile.core.wire.gen.ExtKind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExtKnown::class.java to com.ccmonitor.mobile.core.wire.gen.ExtKnown.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExtLegend::class.java to com.ccmonitor.mobile.core.wire.gen.ExtLegend.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExtList::class.java to com.ccmonitor.mobile.core.wire.gen.ExtList.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExtListReq::class.java to com.ccmonitor.mobile.core.wire.gen.ExtListReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExtLoc::class.java to com.ccmonitor.mobile.core.wire.gen.ExtLoc.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExtMachine::class.java to com.ccmonitor.mobile.core.wire.gen.ExtMachine.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExtNote::class.java to com.ccmonitor.mobile.core.wire.gen.ExtNote.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExtNoteReq::class.java to com.ccmonitor.mobile.core.wire.gen.ExtNoteReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExtPlace::class.java to com.ccmonitor.mobile.core.wire.gen.ExtPlace.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExtRow::class.java to com.ccmonitor.mobile.core.wire.gen.ExtRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExtScope::class.java to com.ccmonitor.mobile.core.wire.gen.ExtScope.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExtSlot::class.java to com.ccmonitor.mobile.core.wire.gen.ExtSlot.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExtState::class.java to com.ccmonitor.mobile.core.wire.gen.ExtState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExtTarget::class.java to com.ccmonitor.mobile.core.wire.gen.ExtTarget.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExtTokens::class.java to com.ccmonitor.mobile.core.wire.gen.ExtTokens.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExtUninstallApplyReq::class.java to com.ccmonitor.mobile.core.wire.gen.ExtUninstallApplyReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExtUninstallCard::class.java to com.ccmonitor.mobile.core.wire.gen.ExtUninstallCard.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ExtUninstallReq::class.java to com.ccmonitor.mobile.core.wire.gen.ExtUninstallReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Fact::class.java to com.ccmonitor.mobile.core.wire.gen.Fact.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FactBlocked::class.java to com.ccmonitor.mobile.core.wire.gen.FactBlocked.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FactWhy::class.java to com.ccmonitor.mobile.core.wire.gen.FactWhy.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FactsBackground::class.java to com.ccmonitor.mobile.core.wire.gen.FactsBackground.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FactsByPath::class.java to com.ccmonitor.mobile.core.wire.gen.FactsByPath.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FactsBySid::class.java to com.ccmonitor.mobile.core.wire.gen.FactsBySid.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FactsClock::class.java to com.ccmonitor.mobile.core.wire.gen.FactsClock.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FactsCost::class.java to com.ccmonitor.mobile.core.wire.gen.FactsCost.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FactsLine::class.java to com.ccmonitor.mobile.core.wire.gen.FactsLine.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FactsReply::class.java to com.ccmonitor.mobile.core.wire.gen.FactsReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Failed::class.java to com.ccmonitor.mobile.core.wire.gen.Failed.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FileClass::class.java to com.ccmonitor.mobile.core.wire.gen.FileClass.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FileKind::class.java to com.ccmonitor.mobile.core.wire.gen.FileKind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FileState::class.java to com.ccmonitor.mobile.core.wire.gen.FileState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FilesBatchStopReply::class.java to com.ccmonitor.mobile.core.wire.gen.FilesBatchStopReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FilesBatchStopReq::class.java to com.ccmonitor.mobile.core.wire.gen.FilesBatchStopReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FilesChmodReq::class.java to com.ccmonitor.mobile.core.wire.gen.FilesChmodReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FilesCommitTextReq::class.java to com.ccmonitor.mobile.core.wire.gen.FilesCommitTextReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FilesCommitUploadReq::class.java to com.ccmonitor.mobile.core.wire.gen.FilesCommitUploadReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FilesCopyReq::class.java to com.ccmonitor.mobile.core.wire.gen.FilesCopyReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FilesCreateReq::class.java to com.ccmonitor.mobile.core.wire.gen.FilesCreateReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FilesDeleteReq::class.java to com.ccmonitor.mobile.core.wire.gen.FilesDeleteReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FilesDeleteSessionReq::class.java to com.ccmonitor.mobile.core.wire.gen.FilesDeleteSessionReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FilesExtractReq::class.java to com.ccmonitor.mobile.core.wire.gen.FilesExtractReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FilesFindHit::class.java to com.ccmonitor.mobile.core.wire.gen.FilesFindHit.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FilesFindHitMarks::class.java to com.ccmonitor.mobile.core.wire.gen.FilesFindHitMarks.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FilesMkdirReq::class.java to com.ccmonitor.mobile.core.wire.gen.FilesMkdirReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FilesPathBytesReply::class.java to com.ccmonitor.mobile.core.wire.gen.FilesPathBytesReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FilesPathReply::class.java to com.ccmonitor.mobile.core.wire.gen.FilesPathReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FilesRenameReq::class.java to com.ccmonitor.mobile.core.wire.gen.FilesRenameReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FilesStageChunkReply::class.java to com.ccmonitor.mobile.core.wire.gen.FilesStageChunkReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FilesStageChunkReq::class.java to com.ccmonitor.mobile.core.wire.gen.FilesStageChunkReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FilesTreeReply::class.java to com.ccmonitor.mobile.core.wire.gen.FilesTreeReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FilesWriteTextReq::class.java to com.ccmonitor.mobile.core.wire.gen.FilesWriteTextReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FilesWrittenReply::class.java to com.ccmonitor.mobile.core.wire.gen.FilesWrittenReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FilewinTotals::class.java to com.ccmonitor.mobile.core.wire.gen.FilewinTotals.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FindHit::class.java to com.ccmonitor.mobile.core.wire.gen.FindHit.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FindReply::class.java to com.ccmonitor.mobile.core.wire.gen.FindReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FindReq::class.java to com.ccmonitor.mobile.core.wire.gen.FindReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FindScope::class.java to com.ccmonitor.mobile.core.wire.gen.FindScope.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FindSort::class.java to com.ccmonitor.mobile.core.wire.gen.FindSort.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FirstRun::class.java to com.ccmonitor.mobile.core.wire.gen.FirstRun.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FirstRunReq::class.java to com.ccmonitor.mobile.core.wire.gen.FirstRunReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FirstRunStep::class.java to com.ccmonitor.mobile.core.wire.gen.FirstRunStep.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FirstRunStepId::class.java to com.ccmonitor.mobile.core.wire.gen.FirstRunStepId.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FleetLines::class.java to com.ccmonitor.mobile.core.wire.gen.FleetLines.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FoldFail::class.java to com.ccmonitor.mobile.core.wire.gen.FoldFail.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FoldSegment::class.java to com.ccmonitor.mobile.core.wire.gen.FoldSegment.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FollowEnd::class.java to com.ccmonitor.mobile.core.wire.gen.FollowEnd.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FollowLive::class.java to com.ccmonitor.mobile.core.wire.gen.FollowLive.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FollowRefused::class.java to com.ccmonitor.mobile.core.wire.gen.FollowRefused.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FollowReq::class.java to com.ccmonitor.mobile.core.wire.gen.FollowReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FootprintClient::class.java to com.ccmonitor.mobile.core.wire.gen.FootprintClient.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FootprintReq::class.java to com.ccmonitor.mobile.core.wire.gen.FootprintReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ForgetReq::class.java to com.ccmonitor.mobile.core.wire.gen.ForgetReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Forgotten::class.java to com.ccmonitor.mobile.core.wire.gen.Forgotten.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ForkFacts::class.java to com.ccmonitor.mobile.core.wire.gen.ForkFacts.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ForkFrom::class.java to com.ccmonitor.mobile.core.wire.gen.ForkFrom.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ForkLaunch::class.java to com.ccmonitor.mobile.core.wire.gen.ForkLaunch.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ForkReq::class.java to com.ccmonitor.mobile.core.wire.gen.ForkReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ForkTerminal::class.java to com.ccmonitor.mobile.core.wire.gen.ForkTerminal.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ForkWhence::class.java to com.ccmonitor.mobile.core.wire.gen.ForkWhence.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ForkWhy::class.java to com.ccmonitor.mobile.core.wire.gen.ForkWhy.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Forked::class.java to com.ccmonitor.mobile.core.wire.gen.Forked.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ForkedFrom::class.java to com.ccmonitor.mobile.core.wire.gen.ForkedFrom.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ForwardId::class.java to com.ccmonitor.mobile.core.wire.gen.ForwardId.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ForwardRow::class.java to com.ccmonitor.mobile.core.wire.gen.ForwardRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ForwardRowState::class.java to com.ccmonitor.mobile.core.wire.gen.ForwardRowState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ForwardSpec::class.java to com.ccmonitor.mobile.core.wire.gen.ForwardSpec.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ForwardStartReq::class.java to com.ccmonitor.mobile.core.wire.gen.ForwardStartReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ForwardStarted::class.java to com.ccmonitor.mobile.core.wire.gen.ForwardStarted.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Forwards::class.java to com.ccmonitor.mobile.core.wire.gen.Forwards.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FrontAct::class.java to com.ccmonitor.mobile.core.wire.gen.FrontAct.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FrontFace::class.java to com.ccmonitor.mobile.core.wire.gen.FrontFace.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FrontTold::class.java to com.ccmonitor.mobile.core.wire.gen.FrontTold.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.FrontendReadyPayload::class.java to com.ccmonitor.mobile.core.wire.gen.FrontendReadyPayload.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.GapCount::class.java to com.ccmonitor.mobile.core.wire.gen.GapCount.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Gate::class.java to com.ccmonitor.mobile.core.wire.gen.Gate.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Gates::class.java to com.ccmonitor.mobile.core.wire.gen.Gates.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Gone::class.java to com.ccmonitor.mobile.core.wire.gen.Gone.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.GrepHit::class.java to com.ccmonitor.mobile.core.wire.gen.GrepHit.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.GrepLine::class.java to com.ccmonitor.mobile.core.wire.gen.GrepLine.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.GrepLineMarks::class.java to com.ccmonitor.mobile.core.wire.gen.GrepLineMarks.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.GrepReply::class.java to com.ccmonitor.mobile.core.wire.gen.GrepReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.GrepReq::class.java to com.ccmonitor.mobile.core.wire.gen.GrepReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.GrepStopped::class.java to com.ccmonitor.mobile.core.wire.gen.GrepStopped.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.GroupCount::class.java to com.ccmonitor.mobile.core.wire.gen.GroupCount.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.GroupKnown::class.java to com.ccmonitor.mobile.core.wire.gen.GroupKnown.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.GroupMember::class.java to com.ccmonitor.mobile.core.wire.gen.GroupMember.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.GroupMissed::class.java to com.ccmonitor.mobile.core.wire.gen.GroupMissed.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.GroupRead::class.java to com.ccmonitor.mobile.core.wire.gen.GroupRead.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.GroupRef::class.java to com.ccmonitor.mobile.core.wire.gen.GroupRef.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.GroupsHere::class.java to com.ccmonitor.mobile.core.wire.gen.GroupsHere.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.GroupsMachine::class.java to com.ccmonitor.mobile.core.wire.gen.GroupsMachine.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.GroupsRead::class.java to com.ccmonitor.mobile.core.wire.gen.GroupsRead.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.GroupsReadReq::class.java to com.ccmonitor.mobile.core.wire.gen.GroupsReadReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.GroupsWriteReq::class.java to com.ccmonitor.mobile.core.wire.gen.GroupsWriteReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.GroupsWritten::class.java to com.ccmonitor.mobile.core.wire.gen.GroupsWritten.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HealthFace::class.java to com.ccmonitor.mobile.core.wire.gen.HealthFace.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HealthState::class.java to com.ccmonitor.mobile.core.wire.gen.HealthState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Held::class.java to com.ccmonitor.mobile.core.wire.gen.Held.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HereGroup::class.java to com.ccmonitor.mobile.core.wire.gen.HereGroup.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HereMember::class.java to com.ccmonitor.mobile.core.wire.gen.HereMember.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HexBytes::class.java to com.ccmonitor.mobile.core.wire.gen.HexBytes.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Hidden::class.java to com.ccmonitor.mobile.core.wire.gen.Hidden.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HiddenBy::class.java to com.ccmonitor.mobile.core.wire.gen.HiddenBy.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HiddenNow::class.java to com.ccmonitor.mobile.core.wire.gen.HiddenNow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Hide::class.java to com.ccmonitor.mobile.core.wire.gen.Hide.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HideAsk::class.java to com.ccmonitor.mobile.core.wire.gen.HideAsk.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryBrowseHead::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryBrowseHead.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryBrowseReq::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryBrowseReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryBy::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryBy.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryFactsReq::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryFactsReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryFailedDir::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryFailedDir.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryFind::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryFind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryFindReq::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryFindReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryFold::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryFold.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryGroup::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryGroup.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryGroupId::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryGroupId.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryHead::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryHead.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryImage::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryImage.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryImageReq::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryImageReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryIndex::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryIndex.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryIndexReq::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryIndexReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryLine::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryLine.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryLines::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryLines.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryLinesReq::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryLinesReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryListReq::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryListReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryListingHead::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryListingHead.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryListingReq::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryListingReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryMachine::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryMachine.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryMachineRead::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryMachineRead.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryPage::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryPage.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryPageReq::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryPageReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryRawRow::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryRawRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryRead::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryRead.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryReadReq::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryReadReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryRecordReq::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryRecordReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryRow::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryRowId::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryRowId.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryRun::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryRun.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryRunReq::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryRunReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistorySearchMergeReq::class.java to com.ccmonitor.mobile.core.wire.gen.HistorySearchMergeReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistorySearchReply::class.java to com.ccmonitor.mobile.core.wire.gen.HistorySearchReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistorySearchReq::class.java to com.ccmonitor.mobile.core.wire.gen.HistorySearchReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistorySection::class.java to com.ccmonitor.mobile.core.wire.gen.HistorySection.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistorySort::class.java to com.ccmonitor.mobile.core.wire.gen.HistorySort.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryTail::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryTail.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryTailReq::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryTailReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryTurns::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryTurns.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HistoryTurnsReq::class.java to com.ccmonitor.mobile.core.wire.gen.HistoryTurnsReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HlCode::class.java to com.ccmonitor.mobile.core.wire.gen.HlCode.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HolderWho::class.java to com.ccmonitor.mobile.core.wire.gen.HolderWho.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HomeReq::class.java to com.ccmonitor.mobile.core.wire.gen.HomeReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HookState::class.java to com.ccmonitor.mobile.core.wire.gen.HookState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HooksDiagnosis::class.java to com.ccmonitor.mobile.core.wire.gen.HooksDiagnosis.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HooksReport::class.java to com.ccmonitor.mobile.core.wire.gen.HooksReport.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Host::class.java to com.ccmonitor.mobile.core.wire.gen.Host.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HostFacts::class.java to com.ccmonitor.mobile.core.wire.gen.HostFacts.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HostKeysForgetReq::class.java to com.ccmonitor.mobile.core.wire.gen.HostKeysForgetReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HubApplied::class.java to com.ccmonitor.mobile.core.wire.gen.HubApplied.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HubApplyReq::class.java to com.ccmonitor.mobile.core.wire.gen.HubApplyReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HubMachineCard::class.java to com.ccmonitor.mobile.core.wire.gen.HubMachineCard.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HubMachineDone::class.java to com.ccmonitor.mobile.core.wire.gen.HubMachineDone.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HubPreview::class.java to com.ccmonitor.mobile.core.wire.gen.HubPreview.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HubReq::class.java to com.ccmonitor.mobile.core.wire.gen.HubReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.HubSlot::class.java to com.ccmonitor.mobile.core.wire.gen.HubSlot.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.IndexBody::class.java to com.ccmonitor.mobile.core.wire.gen.IndexBody.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.IndexInputs::class.java to com.ccmonitor.mobile.core.wire.gen.IndexInputs.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.IndexStatusReply::class.java to com.ccmonitor.mobile.core.wire.gen.IndexStatusReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.IndexStatusReq::class.java to com.ccmonitor.mobile.core.wire.gen.IndexStatusReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Inl::class.java to com.ccmonitor.mobile.core.wire.gen.Inl.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.InlNode::class.java to com.ccmonitor.mobile.core.wire.gen.InlNode.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.InputHolder::class.java to com.ccmonitor.mobile.core.wire.gen.InputHolder.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.InputNow::class.java to com.ccmonitor.mobile.core.wire.gen.InputNow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Installed::class.java to com.ccmonitor.mobile.core.wire.gen.Installed.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Interrupts::class.java to com.ccmonitor.mobile.core.wire.gen.Interrupts.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ItemState::class.java to com.ccmonitor.mobile.core.wire.gen.ItemState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.JumpHop::class.java to com.ccmonitor.mobile.core.wire.gen.JumpHop.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.KeyAddReq::class.java to com.ccmonitor.mobile.core.wire.gen.KeyAddReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.KeyAdded::class.java to com.ccmonitor.mobile.core.wire.gen.KeyAdded.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.KeyPair::class.java to com.ccmonitor.mobile.core.wire.gen.KeyPair.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.KeySet::class.java to com.ccmonitor.mobile.core.wire.gen.KeySet.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.KeySetReq::class.java to com.ccmonitor.mobile.core.wire.gen.KeySetReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Kin::class.java to com.ccmonitor.mobile.core.wire.gen.Kin.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.KnownHost::class.java to com.ccmonitor.mobile.core.wire.gen.KnownHost.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Landed::class.java to com.ccmonitor.mobile.core.wire.gen.Landed.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LastSay::class.java to com.ccmonitor.mobile.core.wire.gen.LastSay.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LastSeen::class.java to com.ccmonitor.mobile.core.wire.gen.LastSeen.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LastSeenReadReq::class.java to com.ccmonitor.mobile.core.wire.gen.LastSeenReadReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LastSeenWriteReq::class.java to com.ccmonitor.mobile.core.wire.gen.LastSeenWriteReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LastSeenWritten::class.java to com.ccmonitor.mobile.core.wire.gen.LastSeenWritten.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Launch::class.java to com.ccmonitor.mobile.core.wire.gen.Launch.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LaunchTab::class.java to com.ccmonitor.mobile.core.wire.gen.LaunchTab.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LaunchedAccount::class.java to com.ccmonitor.mobile.core.wire.gen.LaunchedAccount.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LedgerFile::class.java to com.ccmonitor.mobile.core.wire.gen.LedgerFile.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Life::class.java to com.ccmonitor.mobile.core.wire.gen.Life.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LimitFrom::class.java to com.ccmonitor.mobile.core.wire.gen.LimitFrom.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LineDiff::class.java to com.ccmonitor.mobile.core.wire.gen.LineDiff.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LineDiffKind::class.java to com.ccmonitor.mobile.core.wire.gen.LineDiffKind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LineDiffRow::class.java to com.ccmonitor.mobile.core.wire.gen.LineDiffRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LineRecord::class.java to com.ccmonitor.mobile.core.wire.gen.LineRecord.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LineRecordBody::class.java to com.ccmonitor.mobile.core.wire.gen.LineRecordBody.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LineRecordBodyUnreadTone::class.java to com.ccmonitor.mobile.core.wire.gen.LineRecordBodyUnreadTone.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LineRecordUnreadTone::class.java to com.ccmonitor.mobile.core.wire.gen.LineRecordUnreadTone.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Link::class.java to com.ccmonitor.mobile.core.wire.gen.Link.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LinkLine::class.java to com.ccmonitor.mobile.core.wire.gen.LinkLine.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LinkTo::class.java to com.ccmonitor.mobile.core.wire.gen.LinkTo.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ListMeta::class.java to com.ccmonitor.mobile.core.wire.gen.ListMeta.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LiveBlock::class.java to com.ccmonitor.mobile.core.wire.gen.LiveBlock.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LiveCard::class.java to com.ccmonitor.mobile.core.wire.gen.LiveCard.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LiveCardNow::class.java to com.ccmonitor.mobile.core.wire.gen.LiveCardNow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LiveCards::class.java to com.ccmonitor.mobile.core.wire.gen.LiveCards.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LiveOp::class.java to com.ccmonitor.mobile.core.wire.gen.LiveOp.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LivePhase::class.java to com.ccmonitor.mobile.core.wire.gen.LivePhase.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LiveWhy::class.java to com.ccmonitor.mobile.core.wire.gen.LiveWhy.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LocalAsk::class.java to com.ccmonitor.mobile.core.wire.gen.LocalAsk.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LocalBackend::class.java to com.ccmonitor.mobile.core.wire.gen.LocalBackend.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LocalOpened::class.java to com.ccmonitor.mobile.core.wire.gen.LocalOpened.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LogFileEntry::class.java to com.ccmonitor.mobile.core.wire.gen.LogFileEntry.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LogFileInfo::class.java to com.ccmonitor.mobile.core.wire.gen.LogFileInfo.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LogTail::class.java to com.ccmonitor.mobile.core.wire.gen.LogTail.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LoginAgain::class.java to com.ccmonitor.mobile.core.wire.gen.LoginAgain.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LoginAgainHow::class.java to com.ccmonitor.mobile.core.wire.gen.LoginAgainHow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LoginCancelReq::class.java to com.ccmonitor.mobile.core.wire.gen.LoginCancelReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LoginCancelled::class.java to com.ccmonitor.mobile.core.wire.gen.LoginCancelled.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LoginDone::class.java to com.ccmonitor.mobile.core.wire.gen.LoginDone.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LoginFinishReq::class.java to com.ccmonitor.mobile.core.wire.gen.LoginFinishReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LoginStartArgs::class.java to com.ccmonitor.mobile.core.wire.gen.LoginStartArgs.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LoginStartReq::class.java to com.ccmonitor.mobile.core.wire.gen.LoginStartReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LoginStarted::class.java to com.ccmonitor.mobile.core.wire.gen.LoginStarted.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LoginState::class.java to com.ccmonitor.mobile.core.wire.gen.LoginState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Looked::class.java to com.ccmonitor.mobile.core.wire.gen.Looked.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LsEntry::class.java to com.ccmonitor.mobile.core.wire.gen.LsEntry.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LsHead::class.java to com.ccmonitor.mobile.core.wire.gen.LsHead.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LsReq::class.java to com.ccmonitor.mobile.core.wire.gen.LsReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LsSort::class.java to com.ccmonitor.mobile.core.wire.gen.LsSort.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.LsSortBy::class.java to com.ccmonitor.mobile.core.wire.gen.LsSortBy.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineBlocker::class.java to com.ccmonitor.mobile.core.wire.gen.MachineBlocker.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineCheck::class.java to com.ccmonitor.mobile.core.wire.gen.MachineCheck.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineEdit::class.java to com.ccmonitor.mobile.core.wire.gen.MachineEdit.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineFault::class.java to com.ccmonitor.mobile.core.wire.gen.MachineFault.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineFaultCode::class.java to com.ccmonitor.mobile.core.wire.gen.MachineFaultCode.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineFind::class.java to com.ccmonitor.mobile.core.wire.gen.MachineFind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineFindReq::class.java to com.ccmonitor.mobile.core.wire.gen.MachineFindReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineFix::class.java to com.ccmonitor.mobile.core.wire.gen.MachineFix.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineFixButton::class.java to com.ccmonitor.mobile.core.wire.gen.MachineFixButton.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineFrom::class.java to com.ccmonitor.mobile.core.wire.gen.MachineFrom.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineHit::class.java to com.ccmonitor.mobile.core.wire.gen.MachineHit.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineInterruptsReply::class.java to com.ccmonitor.mobile.core.wire.gen.MachineInterruptsReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineInterruptsReq::class.java to com.ccmonitor.mobile.core.wire.gen.MachineInterruptsReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineMenu::class.java to com.ccmonitor.mobile.core.wire.gen.MachineMenu.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineMenuReq::class.java to com.ccmonitor.mobile.core.wire.gen.MachineMenuReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineRead::class.java to com.ccmonitor.mobile.core.wire.gen.MachineRead.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineReadReq::class.java to com.ccmonitor.mobile.core.wire.gen.MachineReadReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineRound::class.java to com.ccmonitor.mobile.core.wire.gen.MachineRound.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineRoundProblem::class.java to com.ccmonitor.mobile.core.wire.gen.MachineRoundProblem.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineRow::class.java to com.ccmonitor.mobile.core.wire.gen.MachineRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineStage::class.java to com.ccmonitor.mobile.core.wire.gen.MachineStage.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineState::class.java to com.ccmonitor.mobile.core.wire.gen.MachineState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineStateKind::class.java to com.ccmonitor.mobile.core.wire.gen.MachineStateKind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineStopReq::class.java to com.ccmonitor.mobile.core.wire.gen.MachineStopReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineStopped::class.java to com.ccmonitor.mobile.core.wire.gen.MachineStopped.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineStripLine::class.java to com.ccmonitor.mobile.core.wire.gen.MachineStripLine.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachineWakeReq::class.java to com.ccmonitor.mobile.core.wire.gen.MachineWakeReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachinesEdits::class.java to com.ccmonitor.mobile.core.wire.gen.MachinesEdits.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachinesReply::class.java to com.ccmonitor.mobile.core.wire.gen.MachinesReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachinesSet::class.java to com.ccmonitor.mobile.core.wire.gen.MachinesSet.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MachinesTried::class.java to com.ccmonitor.mobile.core.wire.gen.MachinesTried.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ManualFix::class.java to com.ccmonitor.mobile.core.wire.gen.ManualFix.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ManualItem::class.java to com.ccmonitor.mobile.core.wire.gen.ManualItem.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.McpApplied::class.java to com.ccmonitor.mobile.core.wire.gen.McpApplied.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.McpApplyReq::class.java to com.ccmonitor.mobile.core.wire.gen.McpApplyReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.McpFailedMark::class.java to com.ccmonitor.mobile.core.wire.gen.McpFailedMark.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.McpLoginFix::class.java to com.ccmonitor.mobile.core.wire.gen.McpLoginFix.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.McpLoginMark::class.java to com.ccmonitor.mobile.core.wire.gen.McpLoginMark.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.McpMark::class.java to com.ccmonitor.mobile.core.wire.gen.McpMark.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.McpPreview::class.java to com.ccmonitor.mobile.core.wire.gen.McpPreview.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.McpPreviewReq::class.java to com.ccmonitor.mobile.core.wire.gen.McpPreviewReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.McpReadEntry::class.java to com.ccmonitor.mobile.core.wire.gen.McpReadEntry.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.McpReadEntryScope::class.java to com.ccmonitor.mobile.core.wire.gen.McpReadEntryScope.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.McpReadEntryStatus::class.java to com.ccmonitor.mobile.core.wire.gen.McpReadEntryStatus.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.McpReadReply::class.java to com.ccmonitor.mobile.core.wire.gen.McpReadReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.McpReadReq::class.java to com.ccmonitor.mobile.core.wire.gen.McpReadReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.McpSource::class.java to com.ccmonitor.mobile.core.wire.gen.McpSource.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.McpSourceReq::class.java to com.ccmonitor.mobile.core.wire.gen.McpSourceReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.McpSourceSlot::class.java to com.ccmonitor.mobile.core.wire.gen.McpSourceSlot.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.McpStatus::class.java to com.ccmonitor.mobile.core.wire.gen.McpStatus.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.McpSuspect::class.java to com.ccmonitor.mobile.core.wire.gen.McpSuspect.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.McpTrouble::class.java to com.ccmonitor.mobile.core.wire.gen.McpTrouble.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MdAlign::class.java to com.ccmonitor.mobile.core.wire.gen.MdAlign.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MdBlock::class.java to com.ccmonitor.mobile.core.wire.gen.MdBlock.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MdItem::class.java to com.ccmonitor.mobile.core.wire.gen.MdItem.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MdNote::class.java to com.ccmonitor.mobile.core.wire.gen.MdNote.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MemberFacts::class.java to com.ccmonitor.mobile.core.wire.gen.MemberFacts.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MenuAct::class.java to com.ccmonitor.mobile.core.wire.gen.MenuAct.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MenuAction::class.java to com.ccmonitor.mobile.core.wire.gen.MenuAction.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MenuEntry::class.java to com.ccmonitor.mobile.core.wire.gen.MenuEntry.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Merged::class.java to com.ccmonitor.mobile.core.wire.gen.Merged.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MergedRow::class.java to com.ccmonitor.mobile.core.wire.gen.MergedRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ModeSwitchResult::class.java to com.ccmonitor.mobile.core.wire.gen.ModeSwitchResult.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ModeSwitched::class.java to com.ccmonitor.mobile.core.wire.gen.ModeSwitched.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ModelSwitched::class.java to com.ccmonitor.mobile.core.wire.gen.ModelSwitched.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MonitorAsk::class.java to com.ccmonitor.mobile.core.wire.gen.MonitorAsk.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MonitorBoard::class.java to com.ccmonitor.mobile.core.wire.gen.MonitorBoard.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MonitorBoardReq::class.java to com.ccmonitor.mobile.core.wire.gen.MonitorBoardReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MonitorGroup::class.java to com.ccmonitor.mobile.core.wire.gen.MonitorGroup.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MonitorHere::class.java to com.ccmonitor.mobile.core.wire.gen.MonitorHere.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MonitorHereReq::class.java to com.ccmonitor.mobile.core.wire.gen.MonitorHereReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.MonitorHereRow::class.java to com.ccmonitor.mobile.core.wire.gen.MonitorHereRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Moved::class.java to com.ccmonitor.mobile.core.wire.gen.Moved.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Needs::class.java to com.ccmonitor.mobile.core.wire.gen.Needs.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.NeedsAsk::class.java to com.ccmonitor.mobile.core.wire.gen.NeedsAsk.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.NeedsAskRun::class.java to com.ccmonitor.mobile.core.wire.gen.NeedsAskRun.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.NeedsAt::class.java to com.ccmonitor.mobile.core.wire.gen.NeedsAt.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.NeedsCell::class.java to com.ccmonitor.mobile.core.wire.gen.NeedsCell.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.NeedsChoice::class.java to com.ccmonitor.mobile.core.wire.gen.NeedsChoice.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.NeedsChoiceRow::class.java to com.ccmonitor.mobile.core.wire.gen.NeedsChoiceRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.NeedsClock::class.java to com.ccmonitor.mobile.core.wire.gen.NeedsClock.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.NeedsDown::class.java to com.ccmonitor.mobile.core.wire.gen.NeedsDown.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.NeedsItem::class.java to com.ccmonitor.mobile.core.wire.gen.NeedsItem.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.NeedsKind::class.java to com.ccmonitor.mobile.core.wire.gen.NeedsKind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.NeedsList::class.java to com.ccmonitor.mobile.core.wire.gen.NeedsList.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.NeedsMergeReq::class.java to com.ccmonitor.mobile.core.wire.gen.NeedsMergeReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.NeedsMerged::class.java to com.ccmonitor.mobile.core.wire.gen.NeedsMerged.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.NeedsNote::class.java to com.ccmonitor.mobile.core.wire.gen.NeedsNote.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.NeedsQuestion::class.java to com.ccmonitor.mobile.core.wire.gen.NeedsQuestion.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.NeedsQuestionOption::class.java to com.ccmonitor.mobile.core.wire.gen.NeedsQuestionOption.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.NeedsRow::class.java to com.ccmonitor.mobile.core.wire.gen.NeedsRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.NeedsUnreadable::class.java to com.ccmonitor.mobile.core.wire.gen.NeedsUnreadable.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.NewOpen::class.java to com.ccmonitor.mobile.core.wire.gen.NewOpen.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.NoArgs::class.java to com.ccmonitor.mobile.core.wire.gen.NoArgs.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.NoReq::class.java to com.ccmonitor.mobile.core.wire.gen.NoReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.NoticeRun::class.java to com.ccmonitor.mobile.core.wire.gen.NoticeRun.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.OnExists::class.java to com.ccmonitor.mobile.core.wire.gen.OnExists.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.OnPath::class.java to com.ccmonitor.mobile.core.wire.gen.OnPath.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.OnPathVerdict::class.java to com.ccmonitor.mobile.core.wire.gen.OnPathVerdict.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.OpenDirReq::class.java to com.ccmonitor.mobile.core.wire.gen.OpenDirReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.OpenFacts::class.java to com.ccmonitor.mobile.core.wire.gen.OpenFacts.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.OpenFactsReq::class.java to com.ccmonitor.mobile.core.wire.gen.OpenFactsReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.OpenField::class.java to com.ccmonitor.mobile.core.wire.gen.OpenField.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.OpenRefusal::class.java to com.ccmonitor.mobile.core.wire.gen.OpenRefusal.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.OpenState::class.java to com.ccmonitor.mobile.core.wire.gen.OpenState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.OpenWhat::class.java to com.ccmonitor.mobile.core.wire.gen.OpenWhat.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.OpenWindow::class.java to com.ccmonitor.mobile.core.wire.gen.OpenWindow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Opened::class.java to com.ccmonitor.mobile.core.wire.gen.Opened.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.OpenedItem::class.java to com.ccmonitor.mobile.core.wire.gen.OpenedItem.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.OriginSessionsListedPayload::class.java to com.ccmonitor.mobile.core.wire.gen.OriginSessionsListedPayload.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.OwnClass::class.java to com.ccmonitor.mobile.core.wire.gen.OwnClass.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.OwnItem::class.java to com.ccmonitor.mobile.core.wire.gen.OwnItem.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PasteKind::class.java to com.ccmonitor.mobile.core.wire.gen.PasteKind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Pasted::class.java to com.ccmonitor.mobile.core.wire.gen.Pasted.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Patch::class.java to com.ccmonitor.mobile.core.wire.gen.Patch.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PendingCall::class.java to com.ccmonitor.mobile.core.wire.gen.PendingCall.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PhoneMenu::class.java to com.ccmonitor.mobile.core.wire.gen.PhoneMenu.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlaceAnswer::class.java to com.ccmonitor.mobile.core.wire.gen.PlaceAnswer.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlaceAnswerAction::class.java to com.ccmonitor.mobile.core.wire.gen.PlaceAnswerAction.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlaceReq::class.java to com.ccmonitor.mobile.core.wire.gen.PlaceReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanAckReply::class.java to com.ccmonitor.mobile.core.wire.gen.PlanAckReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanAckReq::class.java to com.ccmonitor.mobile.core.wire.gen.PlanAckReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanAdopted::class.java to com.ccmonitor.mobile.core.wire.gen.PlanAdopted.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanArchived::class.java to com.ccmonitor.mobile.core.wire.gen.PlanArchived.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanAsk::class.java to com.ccmonitor.mobile.core.wire.gen.PlanAsk.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanAskReq::class.java to com.ccmonitor.mobile.core.wire.gen.PlanAskReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanBlock::class.java to com.ccmonitor.mobile.core.wire.gen.PlanBlock.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanBlockGroups::class.java to com.ccmonitor.mobile.core.wire.gen.PlanBlockGroups.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanBlockSaid::class.java to com.ccmonitor.mobile.core.wire.gen.PlanBlockSaid.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanCell::class.java to com.ccmonitor.mobile.core.wire.gen.PlanCell.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanCellSaid::class.java to com.ccmonitor.mobile.core.wire.gen.PlanCellSaid.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanCellStatusCode::class.java to com.ccmonitor.mobile.core.wire.gen.PlanCellStatusCode.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanCellView::class.java to com.ccmonitor.mobile.core.wire.gen.PlanCellView.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanCellViewReq::class.java to com.ccmonitor.mobile.core.wire.gen.PlanCellViewReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanCheck::class.java to com.ccmonitor.mobile.core.wire.gen.PlanCheck.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanCmdReply::class.java to com.ccmonitor.mobile.core.wire.gen.PlanCmdReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanCommandReq::class.java to com.ccmonitor.mobile.core.wire.gen.PlanCommandReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanCommandReqCmd::class.java to com.ccmonitor.mobile.core.wire.gen.PlanCommandReqCmd.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanDomain::class.java to com.ccmonitor.mobile.core.wire.gen.PlanDomain.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanEdgeWords::class.java to com.ccmonitor.mobile.core.wire.gen.PlanEdgeWords.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanEdges::class.java to com.ccmonitor.mobile.core.wire.gen.PlanEdges.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanFile::class.java to com.ccmonitor.mobile.core.wire.gen.PlanFile.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanFileDup::class.java to com.ccmonitor.mobile.core.wire.gen.PlanFileDup.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanFileOwner::class.java to com.ccmonitor.mobile.core.wire.gen.PlanFileOwner.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanFileOwnerFileState::class.java to com.ccmonitor.mobile.core.wire.gen.PlanFileOwnerFileState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanFileOwnerStatusCode::class.java to com.ccmonitor.mobile.core.wire.gen.PlanFileOwnerStatusCode.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanFileStateCode::class.java to com.ccmonitor.mobile.core.wire.gen.PlanFileStateCode.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanFiles::class.java to com.ccmonitor.mobile.core.wire.gen.PlanFiles.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanFilesReq::class.java to com.ccmonitor.mobile.core.wire.gen.PlanFilesReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanGroup::class.java to com.ccmonitor.mobile.core.wire.gen.PlanGroup.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanKids::class.java to com.ccmonitor.mobile.core.wire.gen.PlanKids.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanKind::class.java to com.ccmonitor.mobile.core.wire.gen.PlanKind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanList::class.java to com.ccmonitor.mobile.core.wire.gen.PlanList.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanListReq::class.java to com.ccmonitor.mobile.core.wire.gen.PlanListReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanListSlice::class.java to com.ccmonitor.mobile.core.wire.gen.PlanListSlice.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanListWorkspace::class.java to com.ccmonitor.mobile.core.wire.gen.PlanListWorkspace.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanLook::class.java to com.ccmonitor.mobile.core.wire.gen.PlanLook.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanMsgRef::class.java to com.ccmonitor.mobile.core.wire.gen.PlanMsgRef.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanMsgRefs::class.java to com.ccmonitor.mobile.core.wire.gen.PlanMsgRefs.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanNeed::class.java to com.ccmonitor.mobile.core.wire.gen.PlanNeed.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanNeedKind::class.java to com.ccmonitor.mobile.core.wire.gen.PlanNeedKind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanNeeds::class.java to com.ccmonitor.mobile.core.wire.gen.PlanNeeds.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanNewExisting::class.java to com.ccmonitor.mobile.core.wire.gen.PlanNewExisting.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanNewForm::class.java to com.ccmonitor.mobile.core.wire.gen.PlanNewForm.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanNewReq::class.java to com.ccmonitor.mobile.core.wire.gen.PlanNewReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanPb::class.java to com.ccmonitor.mobile.core.wire.gen.PlanPb.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanPbCheck::class.java to com.ccmonitor.mobile.core.wire.gen.PlanPbCheck.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanPbCheckReq::class.java to com.ccmonitor.mobile.core.wire.gen.PlanPbCheckReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanPbCheckState::class.java to com.ccmonitor.mobile.core.wire.gen.PlanPbCheckState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanPbState::class.java to com.ccmonitor.mobile.core.wire.gen.PlanPbState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanPendingSet::class.java to com.ccmonitor.mobile.core.wire.gen.PlanPendingSet.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanPhase::class.java to com.ccmonitor.mobile.core.wire.gen.PlanPhase.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanProgress::class.java to com.ccmonitor.mobile.core.wire.gen.PlanProgress.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanRead::class.java to com.ccmonitor.mobile.core.wire.gen.PlanRead.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanReadError::class.java to com.ccmonitor.mobile.core.wire.gen.PlanReadError.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanReadReq::class.java to com.ccmonitor.mobile.core.wire.gen.PlanReadReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanRed::class.java to com.ccmonitor.mobile.core.wire.gen.PlanRed.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanRef::class.java to com.ccmonitor.mobile.core.wire.gen.PlanRef.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanRefs::class.java to com.ccmonitor.mobile.core.wire.gen.PlanRefs.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanRefsReq::class.java to com.ccmonitor.mobile.core.wire.gen.PlanRefsReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanReturnReply::class.java to com.ccmonitor.mobile.core.wire.gen.PlanReturnReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanReturnReplyResult::class.java to com.ccmonitor.mobile.core.wire.gen.PlanReturnReplyResult.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanReturnReq::class.java to com.ccmonitor.mobile.core.wire.gen.PlanReturnReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanReturnReqTo::class.java to com.ccmonitor.mobile.core.wire.gen.PlanReturnReqTo.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanReturned::class.java to com.ccmonitor.mobile.core.wire.gen.PlanReturned.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanReturnedBy::class.java to com.ccmonitor.mobile.core.wire.gen.PlanReturnedBy.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanReturnedChild::class.java to com.ccmonitor.mobile.core.wire.gen.PlanReturnedChild.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanReturnedState::class.java to com.ccmonitor.mobile.core.wire.gen.PlanReturnedState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanSessionBlock::class.java to com.ccmonitor.mobile.core.wire.gen.PlanSessionBlock.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanSessionBlockVia::class.java to com.ccmonitor.mobile.core.wire.gen.PlanSessionBlockVia.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanSessionSaid::class.java to com.ccmonitor.mobile.core.wire.gen.PlanSessionSaid.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanSetsTo::class.java to com.ccmonitor.mobile.core.wire.gen.PlanSetsTo.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanSlice::class.java to com.ccmonitor.mobile.core.wire.gen.PlanSlice.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanSliceSaid::class.java to com.ccmonitor.mobile.core.wire.gen.PlanSliceSaid.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanStale::class.java to com.ccmonitor.mobile.core.wire.gen.PlanStale.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanState::class.java to com.ccmonitor.mobile.core.wire.gen.PlanState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanTakeover::class.java to com.ccmonitor.mobile.core.wire.gen.PlanTakeover.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanTitle::class.java to com.ccmonitor.mobile.core.wire.gen.PlanTitle.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanUnbuilt::class.java to com.ccmonitor.mobile.core.wire.gen.PlanUnbuilt.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanUndecidable::class.java to com.ccmonitor.mobile.core.wire.gen.PlanUndecidable.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanUnowned::class.java to com.ccmonitor.mobile.core.wire.gen.PlanUnowned.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanWho::class.java to com.ccmonitor.mobile.core.wire.gen.PlanWho.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanWhoKind::class.java to com.ccmonitor.mobile.core.wire.gen.PlanWhoKind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanWhyCode::class.java to com.ccmonitor.mobile.core.wire.gen.PlanWhyCode.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PlanWhyShort::class.java to com.ccmonitor.mobile.core.wire.gen.PlanWhyShort.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PolicySet::class.java to com.ccmonitor.mobile.core.wire.gen.PolicySet.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PolicySetReq::class.java to com.ccmonitor.mobile.core.wire.gen.PolicySetReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Prefs::class.java to com.ccmonitor.mobile.core.wire.gen.Prefs.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PrefsReply::class.java to com.ccmonitor.mobile.core.wire.gen.PrefsReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PrefsSet::class.java to com.ccmonitor.mobile.core.wire.gen.PrefsSet.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Preview::class.java to com.ccmonitor.mobile.core.wire.gen.Preview.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PreviewGate::class.java to com.ccmonitor.mobile.core.wire.gen.PreviewGate.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProbeCell::class.java to com.ccmonitor.mobile.core.wire.gen.ProbeCell.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProbeFrom::class.java to com.ccmonitor.mobile.core.wire.gen.ProbeFrom.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProbeReq::class.java to com.ccmonitor.mobile.core.wire.gen.ProbeReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProbeState::class.java to com.ccmonitor.mobile.core.wire.gen.ProbeState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Probed::class.java to com.ccmonitor.mobile.core.wire.gen.Probed.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfileAccountPick::class.java to com.ccmonitor.mobile.core.wire.gen.ProfileAccountPick.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfileAccountShape::class.java to com.ccmonitor.mobile.core.wire.gen.ProfileAccountShape.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfileAffected::class.java to com.ccmonitor.mobile.core.wire.gen.ProfileAffected.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfileBase::class.java to com.ccmonitor.mobile.core.wire.gen.ProfileBase.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfileChildren::class.java to com.ccmonitor.mobile.core.wire.gen.ProfileChildren.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfileCwdCase::class.java to com.ccmonitor.mobile.core.wire.gen.ProfileCwdCase.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfileForm::class.java to com.ccmonitor.mobile.core.wire.gen.ProfileForm.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfileKind::class.java to com.ccmonitor.mobile.core.wire.gen.ProfileKind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfileMergeRow::class.java to com.ccmonitor.mobile.core.wire.gen.ProfileMergeRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfileOp::class.java to com.ccmonitor.mobile.core.wire.gen.ProfileOp.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfileOwnItem::class.java to com.ccmonitor.mobile.core.wire.gen.ProfileOwnItem.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfileProblem::class.java to com.ccmonitor.mobile.core.wire.gen.ProfileProblem.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfileResolved::class.java to com.ccmonitor.mobile.core.wire.gen.ProfileResolved.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfileRow::class.java to com.ccmonitor.mobile.core.wire.gen.ProfileRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfileSlot::class.java to com.ccmonitor.mobile.core.wire.gen.ProfileSlot.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfileSlotChange::class.java to com.ccmonitor.mobile.core.wire.gen.ProfileSlotChange.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfileSlotId::class.java to com.ccmonitor.mobile.core.wire.gen.ProfileSlotId.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfileTmuxMode::class.java to com.ccmonitor.mobile.core.wire.gen.ProfileTmuxMode.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfileTmuxNaming::class.java to com.ccmonitor.mobile.core.wire.gen.ProfileTmuxNaming.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfileTmuxPick::class.java to com.ccmonitor.mobile.core.wire.gen.ProfileTmuxPick.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfilesBases::class.java to com.ccmonitor.mobile.core.wire.gen.ProfilesBases.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfilesBasesReq::class.java to com.ccmonitor.mobile.core.wire.gen.ProfilesBasesReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfilesBook::class.java to com.ccmonitor.mobile.core.wire.gen.ProfilesBook.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfilesFromLineReq::class.java to com.ccmonitor.mobile.core.wire.gen.ProfilesFromLineReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfilesImpact::class.java to com.ccmonitor.mobile.core.wire.gen.ProfilesImpact.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfilesImpactReq::class.java to com.ccmonitor.mobile.core.wire.gen.ProfilesImpactReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfilesResolveReq::class.java to com.ccmonitor.mobile.core.wire.gen.ProfilesResolveReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfilesWriteDone::class.java to com.ccmonitor.mobile.core.wire.gen.ProfilesWriteDone.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProfilesWriteReq::class.java to com.ccmonitor.mobile.core.wire.gen.ProfilesWriteReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ProseHlReq::class.java to com.ccmonitor.mobile.core.wire.gen.ProseHlReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PsExecPolicy::class.java to com.ccmonitor.mobile.core.wire.gen.PsExecPolicy.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PsHost::class.java to com.ccmonitor.mobile.core.wire.gen.PsHost.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PtyOpts::class.java to com.ccmonitor.mobile.core.wire.gen.PtyOpts.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PubkeyPushReq::class.java to com.ccmonitor.mobile.core.wire.gen.PubkeyPushReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PubkeyPushed::class.java to com.ccmonitor.mobile.core.wire.gen.PubkeyPushed.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Pulled::class.java to com.ccmonitor.mobile.core.wire.gen.Pulled.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PushOutcome::class.java to com.ccmonitor.mobile.core.wire.gen.PushOutcome.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.PushVia::class.java to com.ccmonitor.mobile.core.wire.gen.PushVia.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QueueCancelled::class.java to com.ccmonitor.mobile.core.wire.gen.QueueCancelled.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QueueItem::class.java to com.ccmonitor.mobile.core.wire.gen.QueueItem.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QueueList::class.java to com.ccmonitor.mobile.core.wire.gen.QueueList.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QueuePut::class.java to com.ccmonitor.mobile.core.wire.gen.QueuePut.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QueuePutOutcome::class.java to com.ccmonitor.mobile.core.wire.gen.QueuePutOutcome.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QueueState::class.java to com.ccmonitor.mobile.core.wire.gen.QueueState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuietUnread::class.java to com.ccmonitor.mobile.core.wire.gen.QuietUnread.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaAccount::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaAccount.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaAccountHead::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaAccountHead.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaCandidate::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaCandidate.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaCell::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaCell.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaKind::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaKind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaNames::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaNames.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaOverageShown::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaOverageShown.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaProbeReq::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaProbeReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaRead::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaRead.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaReadReq::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaReadReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaReadState::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaReadState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaReadingShown::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaReadingShown.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaResetsAt::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaResetsAt.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaShow::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaShow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaSource::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaSource.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaState::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaStatus::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaStatus.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaUnseen::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaUnseen.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaUnseenHead::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaUnseenHead.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaUnseenUsage::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaUnseenUsage.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaVerdict::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaVerdict.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaWarm::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaWarm.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaWarmAct::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaWarmAct.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaWarmReq::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaWarmReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaWindow::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaWindow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaWindowRead::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaWindowRead.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.QuotaWindowSeen::class.java to com.ccmonitor.mobile.core.wire.gen.QuotaWindowSeen.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ReachRow::class.java to com.ccmonitor.mobile.core.wire.gen.ReachRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ReadChunkReply::class.java to com.ccmonitor.mobile.core.wire.gen.ReadChunkReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ReadChunkReq::class.java to com.ccmonitor.mobile.core.wire.gen.ReadChunkReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ReadRow::class.java to com.ccmonitor.mobile.core.wire.gen.ReadRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ReadTextReply::class.java to com.ccmonitor.mobile.core.wire.gen.ReadTextReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ReadTextReq::class.java to com.ccmonitor.mobile.core.wire.gen.ReadTextReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RebuildReply::class.java to com.ccmonitor.mobile.core.wire.gen.RebuildReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RebuildReq::class.java to com.ccmonitor.mobile.core.wire.gen.RebuildReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RecentDir::class.java to com.ccmonitor.mobile.core.wire.gen.RecentDir.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RecordClass::class.java to com.ccmonitor.mobile.core.wire.gen.RecordClass.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RecordLine::class.java to com.ccmonitor.mobile.core.wire.gen.RecordLine.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RecordProbe::class.java to com.ccmonitor.mobile.core.wire.gen.RecordProbe.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Refused::class.java to com.ccmonitor.mobile.core.wire.gen.Refused.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ReplyError::class.java to com.ccmonitor.mobile.core.wire.gen.ReplyError.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RereadWhy::class.java to com.ccmonitor.mobile.core.wire.gen.RereadWhy.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RestartHint::class.java to com.ccmonitor.mobile.core.wire.gen.RestartHint.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RestartReq::class.java to com.ccmonitor.mobile.core.wire.gen.RestartReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Restarted::class.java to com.ccmonitor.mobile.core.wire.gen.Restarted.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ResultKind::class.java to com.ccmonitor.mobile.core.wire.gen.ResultKind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ResumeWhere::class.java to com.ccmonitor.mobile.core.wire.gen.ResumeWhere.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ResumeWith::class.java to com.ccmonitor.mobile.core.wire.gen.ResumeWith.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ResyncReq::class.java to com.ccmonitor.mobile.core.wire.gen.ResyncReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Resynced::class.java to com.ccmonitor.mobile.core.wire.gen.Resynced.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RetryOutcome::class.java to com.ccmonitor.mobile.core.wire.gen.RetryOutcome.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RetryRun::class.java to com.ccmonitor.mobile.core.wire.gen.RetryRun.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Rewind::class.java to com.ccmonitor.mobile.core.wire.gen.Rewind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RewindChoiceRow::class.java to com.ccmonitor.mobile.core.wire.gen.RewindChoiceRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RewindHow::class.java to com.ccmonitor.mobile.core.wire.gen.RewindHow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RewindOptions::class.java to com.ccmonitor.mobile.core.wire.gen.RewindOptions.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RewindWhen::class.java to com.ccmonitor.mobile.core.wire.gen.RewindWhen.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RewindWhenRow::class.java to com.ccmonitor.mobile.core.wire.gen.RewindWhenRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotAccount::class.java to com.ccmonitor.mobile.core.wire.gen.RotAccount.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotBadCell::class.java to com.ccmonitor.mobile.core.wire.gen.RotBadCell.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotBadStrategy::class.java to com.ccmonitor.mobile.core.wire.gen.RotBadStrategy.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotBoard::class.java to com.ccmonitor.mobile.core.wire.gen.RotBoard.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotCan::class.java to com.ccmonitor.mobile.core.wire.gen.RotCan.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotCell::class.java to com.ccmonitor.mobile.core.wire.gen.RotCell.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotChoice::class.java to com.ccmonitor.mobile.core.wire.gen.RotChoice.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotDaySeg::class.java to com.ccmonitor.mobile.core.wire.gen.RotDaySeg.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotDone::class.java to com.ccmonitor.mobile.core.wire.gen.RotDone.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotEdit::class.java to com.ccmonitor.mobile.core.wire.gen.RotEdit.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotEditAddValue::class.java to com.ccmonitor.mobile.core.wire.gen.RotEditAddValue.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotEditMoveValue::class.java to com.ccmonitor.mobile.core.wire.gen.RotEditMoveValue.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotEditPruneValue::class.java to com.ccmonitor.mobile.core.wire.gen.RotEditPruneValue.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotEditRemoveValue::class.java to com.ccmonitor.mobile.core.wire.gen.RotEditRemoveValue.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotEditResetValue::class.java to com.ccmonitor.mobile.core.wire.gen.RotEditResetValue.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotEditSameValue::class.java to com.ccmonitor.mobile.core.wire.gen.RotEditSameValue.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotEditSegValue::class.java to com.ccmonitor.mobile.core.wire.gen.RotEditSegValue.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotEditSetValue::class.java to com.ccmonitor.mobile.core.wire.gen.RotEditSetValue.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotFormKind::class.java to com.ccmonitor.mobile.core.wire.gen.RotFormKind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotFrom::class.java to com.ccmonitor.mobile.core.wire.gen.RotFrom.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotGroup::class.java to com.ccmonitor.mobile.core.wire.gen.RotGroup.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotHint::class.java to com.ccmonitor.mobile.core.wire.gen.RotHint.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotInUse::class.java to com.ccmonitor.mobile.core.wire.gen.RotInUse.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotLine::class.java to com.ccmonitor.mobile.core.wire.gen.RotLine.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotLog::class.java to com.ccmonitor.mobile.core.wire.gen.RotLog.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotLogCause::class.java to com.ccmonitor.mobile.core.wire.gen.RotLogCause.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotLogRow::class.java to com.ccmonitor.mobile.core.wire.gen.RotLogRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotMark::class.java to com.ccmonitor.mobile.core.wire.gen.RotMark.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotNote::class.java to com.ccmonitor.mobile.core.wire.gen.RotNote.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotOldFile::class.java to com.ccmonitor.mobile.core.wire.gen.RotOldFile.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotPlate::class.java to com.ccmonitor.mobile.core.wire.gen.RotPlate.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotRefused::class.java to com.ccmonitor.mobile.core.wire.gen.RotRefused.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotRow::class.java to com.ccmonitor.mobile.core.wire.gen.RotRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotSaw::class.java to com.ccmonitor.mobile.core.wire.gen.RotSaw.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotSawAcct::class.java to com.ccmonitor.mobile.core.wire.gen.RotSawAcct.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotSeg::class.java to com.ccmonitor.mobile.core.wire.gen.RotSeg.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotStale::class.java to com.ccmonitor.mobile.core.wire.gen.RotStale.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotStrategyError::class.java to com.ccmonitor.mobile.core.wire.gen.RotStrategyError.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotStrategyId::class.java to com.ccmonitor.mobile.core.wire.gen.RotStrategyId.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotStrategyItem::class.java to com.ccmonitor.mobile.core.wire.gen.RotStrategyItem.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotStrategyRev::class.java to com.ccmonitor.mobile.core.wire.gen.RotStrategyRev.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotTarget::class.java to com.ccmonitor.mobile.core.wire.gen.RotTarget.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotTryDecision::class.java to com.ccmonitor.mobile.core.wire.gen.RotTryDecision.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotTryLane::class.java to com.ccmonitor.mobile.core.wire.gen.RotTryLane.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotTryRow::class.java to com.ccmonitor.mobile.core.wire.gen.RotTryRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotTrySeg::class.java to com.ccmonitor.mobile.core.wire.gen.RotTrySeg.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotTrySwitch::class.java to com.ccmonitor.mobile.core.wire.gen.RotTrySwitch.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotTryTick::class.java to com.ccmonitor.mobile.core.wire.gen.RotTryTick.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotTryTimeline::class.java to com.ccmonitor.mobile.core.wire.gen.RotTryTimeline.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotTryUse::class.java to com.ccmonitor.mobile.core.wire.gen.RotTryUse.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotUnusable::class.java to com.ccmonitor.mobile.core.wire.gen.RotUnusable.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotationFactsReply::class.java to com.ccmonitor.mobile.core.wire.gen.RotationFactsReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotationFactsReq::class.java to com.ccmonitor.mobile.core.wire.gen.RotationFactsReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotationForNewReq::class.java to com.ccmonitor.mobile.core.wire.gen.RotationForNewReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotationPick::class.java to com.ccmonitor.mobile.core.wire.gen.RotationPick.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotationReloadReq::class.java to com.ccmonitor.mobile.core.wire.gen.RotationReloadReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotationSettingsReq::class.java to com.ccmonitor.mobile.core.wire.gen.RotationSettingsReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotationStrategiesReply::class.java to com.ccmonitor.mobile.core.wire.gen.RotationStrategiesReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotationStrategiesReq::class.java to com.ccmonitor.mobile.core.wire.gen.RotationStrategiesReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotationStrategyAddReq::class.java to com.ccmonitor.mobile.core.wire.gen.RotationStrategyAddReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotationStrategyCopyReq::class.java to com.ccmonitor.mobile.core.wire.gen.RotationStrategyCopyReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotationStrategyDeleteReq::class.java to com.ccmonitor.mobile.core.wire.gen.RotationStrategyDeleteReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotationStrategyRenameReq::class.java to com.ccmonitor.mobile.core.wire.gen.RotationStrategyRenameReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotationStrategyTzReq::class.java to com.ccmonitor.mobile.core.wire.gen.RotationStrategyTzReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotationTempReply::class.java to com.ccmonitor.mobile.core.wire.gen.RotationTempReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotationTempReq::class.java to com.ccmonitor.mobile.core.wire.gen.RotationTempReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotationTryReply::class.java to com.ccmonitor.mobile.core.wire.gen.RotationTryReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotationTryReq::class.java to com.ccmonitor.mobile.core.wire.gen.RotationTryReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotationUseNowReq::class.java to com.ccmonitor.mobile.core.wire.gen.RotationUseNowReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotationUseReq::class.java to com.ccmonitor.mobile.core.wire.gen.RotationUseReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotationViewReply::class.java to com.ccmonitor.mobile.core.wire.gen.RotationViewReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RotationViewReq::class.java to com.ccmonitor.mobile.core.wire.gen.RotationViewReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RowEdit::class.java to com.ccmonitor.mobile.core.wire.gen.RowEdit.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RowLaunched::class.java to com.ccmonitor.mobile.core.wire.gen.RowLaunched.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RowPlan::class.java to com.ccmonitor.mobile.core.wire.gen.RowPlan.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Rules::class.java to com.ccmonitor.mobile.core.wire.gen.Rules.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RunDid::class.java to com.ccmonitor.mobile.core.wire.gen.RunDid.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RunEndLine::class.java to com.ccmonitor.mobile.core.wire.gen.RunEndLine.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RunEnded::class.java to com.ccmonitor.mobile.core.wire.gen.RunEnded.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RunExplain::class.java to com.ccmonitor.mobile.core.wire.gen.RunExplain.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RunGroups::class.java to com.ccmonitor.mobile.core.wire.gen.RunGroups.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RunInfo::class.java to com.ccmonitor.mobile.core.wire.gen.RunInfo.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RunLine::class.java to com.ccmonitor.mobile.core.wire.gen.RunLine.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RunRow::class.java to com.ccmonitor.mobile.core.wire.gen.RunRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RunState::class.java to com.ccmonitor.mobile.core.wire.gen.RunState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.RunWhy::class.java to com.ccmonitor.mobile.core.wire.gen.RunWhy.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ScannedAccount::class.java to com.ccmonitor.mobile.core.wire.gen.ScannedAccount.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ScreenLine::class.java to com.ccmonitor.mobile.core.wire.gen.ScreenLine.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SearchDown::class.java to com.ccmonitor.mobile.core.wire.gen.SearchDown.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SearchHit::class.java to com.ccmonitor.mobile.core.wire.gen.SearchHit.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SearchRow::class.java to com.ccmonitor.mobile.core.wire.gen.SearchRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SearchScope::class.java to com.ccmonitor.mobile.core.wire.gen.SearchScope.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SeenAt::class.java to com.ccmonitor.mobile.core.wire.gen.SeenAt.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SeenKind::class.java to com.ccmonitor.mobile.core.wire.gen.SeenKind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SegHl::class.java to com.ccmonitor.mobile.core.wire.gen.SegHl.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SegMark::class.java to com.ccmonitor.mobile.core.wire.gen.SegMark.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Segs::class.java to com.ccmonitor.mobile.core.wire.gen.Segs.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Sent::class.java to com.ccmonitor.mobile.core.wire.gen.Sent.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SentAct::class.java to com.ccmonitor.mobile.core.wire.gen.SentAct.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SentResult::class.java to com.ccmonitor.mobile.core.wire.gen.SentResult.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionAccountReply::class.java to com.ccmonitor.mobile.core.wire.gen.SessionAccountReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionAccountReplyBadge::class.java to com.ccmonitor.mobile.core.wire.gen.SessionAccountReplyBadge.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionAccountReplyWhen::class.java to com.ccmonitor.mobile.core.wire.gen.SessionAccountReplyWhen.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionAccountReq::class.java to com.ccmonitor.mobile.core.wire.gen.SessionAccountReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionAct::class.java to com.ccmonitor.mobile.core.wire.gen.SessionAct.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionActivity::class.java to com.ccmonitor.mobile.core.wire.gen.SessionActivity.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionAdded::class.java to com.ccmonitor.mobile.core.wire.gen.SessionAdded.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionAttachmentCheckReply::class.java to com.ccmonitor.mobile.core.wire.gen.SessionAttachmentCheckReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionAttachmentCheckReq::class.java to com.ccmonitor.mobile.core.wire.gen.SessionAttachmentCheckReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionAttachmentCommitReply::class.java to com.ccmonitor.mobile.core.wire.gen.SessionAttachmentCommitReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionAttachmentCommitReq::class.java to com.ccmonitor.mobile.core.wire.gen.SessionAttachmentCommitReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionAttachmentDropReply::class.java to com.ccmonitor.mobile.core.wire.gen.SessionAttachmentDropReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionAttachmentDropReq::class.java to com.ccmonitor.mobile.core.wire.gen.SessionAttachmentDropReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionBook::class.java to com.ccmonitor.mobile.core.wire.gen.SessionBook.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionBranchPayload::class.java to com.ccmonitor.mobile.core.wire.gen.SessionBranchPayload.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionCan::class.java to com.ccmonitor.mobile.core.wire.gen.SessionCan.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionCommand::class.java to com.ccmonitor.mobile.core.wire.gen.SessionCommand.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionCommands::class.java to com.ccmonitor.mobile.core.wire.gen.SessionCommands.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionCommandsReq::class.java to com.ccmonitor.mobile.core.wire.gen.SessionCommandsReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionContainer::class.java to com.ccmonitor.mobile.core.wire.gen.SessionContainer.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionDot::class.java to com.ccmonitor.mobile.core.wire.gen.SessionDot.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionFace::class.java to com.ccmonitor.mobile.core.wire.gen.SessionFace.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionFacts::class.java to com.ccmonitor.mobile.core.wire.gen.SessionFacts.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionFileNoticePayload::class.java to com.ccmonitor.mobile.core.wire.gen.SessionFileNoticePayload.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionFileReread::class.java to com.ccmonitor.mobile.core.wire.gen.SessionFileReread.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionHits::class.java to com.ccmonitor.mobile.core.wire.gen.SessionHits.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionInterruptReq::class.java to com.ccmonitor.mobile.core.wire.gen.SessionInterruptReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionItem::class.java to com.ccmonitor.mobile.core.wire.gen.SessionItem.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionLeft::class.java to com.ccmonitor.mobile.core.wire.gen.SessionLeft.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionModelReq::class.java to com.ccmonitor.mobile.core.wire.gen.SessionModelReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionOpenReq::class.java to com.ccmonitor.mobile.core.wire.gen.SessionOpenReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionOutcome::class.java to com.ccmonitor.mobile.core.wire.gen.SessionOutcome.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionPermissionReq::class.java to com.ccmonitor.mobile.core.wire.gen.SessionPermissionReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionQueueCancelReq::class.java to com.ccmonitor.mobile.core.wire.gen.SessionQueueCancelReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionQueuePutReq::class.java to com.ccmonitor.mobile.core.wire.gen.SessionQueuePutReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionQueueReadReq::class.java to com.ccmonitor.mobile.core.wire.gen.SessionQueueReadReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionRewindOptionsReq::class.java to com.ccmonitor.mobile.core.wire.gen.SessionRewindOptionsReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionRuns::class.java to com.ccmonitor.mobile.core.wire.gen.SessionRuns.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionSelf::class.java to com.ccmonitor.mobile.core.wire.gen.SessionSelf.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionSkill::class.java to com.ccmonitor.mobile.core.wire.gen.SessionSkill.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionState::class.java to com.ccmonitor.mobile.core.wire.gen.SessionState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionStatus::class.java to com.ccmonitor.mobile.core.wire.gen.SessionStatus.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionStreamFrame::class.java to com.ccmonitor.mobile.core.wire.gen.SessionStreamFrame.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionWindow::class.java to com.ccmonitor.mobile.core.wire.gen.SessionWindow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionWindowReq::class.java to com.ccmonitor.mobile.core.wire.gen.SessionWindowReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionsBatchReply::class.java to com.ccmonitor.mobile.core.wire.gen.SessionsBatchReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionsBatchReq::class.java to com.ccmonitor.mobile.core.wire.gen.SessionsBatchReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SessionsNeedsReq::class.java to com.ccmonitor.mobile.core.wire.gen.SessionsNeedsReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SetEdit::class.java to com.ccmonitor.mobile.core.wire.gen.SetEdit.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SetLaunchReq::class.java to com.ccmonitor.mobile.core.wire.gen.SetLaunchReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SetLaunched::class.java to com.ccmonitor.mobile.core.wire.gen.SetLaunched.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SetOut::class.java to com.ccmonitor.mobile.core.wire.gen.SetOut.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SetPlan::class.java to com.ccmonitor.mobile.core.wire.gen.SetPlan.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SetPlanReq::class.java to com.ccmonitor.mobile.core.wire.gen.SetPlanReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SetRowOut::class.java to com.ccmonitor.mobile.core.wire.gen.SetRowOut.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SettingsScope::class.java to com.ccmonitor.mobile.core.wire.gen.SettingsScope.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Sha256Expect::class.java to com.ccmonitor.mobile.core.wire.gen.Sha256Expect.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ShellKind::class.java to com.ccmonitor.mobile.core.wire.gen.ShellKind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Showing::class.java to com.ccmonitor.mobile.core.wire.gen.Showing.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ShownWhy::class.java to com.ccmonitor.mobile.core.wire.gen.ShownWhy.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SidReq::class.java to com.ccmonitor.mobile.core.wire.gen.SidReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SidsReq::class.java to com.ccmonitor.mobile.core.wire.gen.SidsReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SignInState::class.java to com.ccmonitor.mobile.core.wire.gen.SignInState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SizeReply::class.java to com.ccmonitor.mobile.core.wire.gen.SizeReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SizeReq::class.java to com.ccmonitor.mobile.core.wire.gen.SizeReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SkillApplied::class.java to com.ccmonitor.mobile.core.wire.gen.SkillApplied.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SkillApplyReq::class.java to com.ccmonitor.mobile.core.wire.gen.SkillApplyReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SkillLedgerFile::class.java to com.ccmonitor.mobile.core.wire.gen.SkillLedgerFile.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SkillPlan::class.java to com.ccmonitor.mobile.core.wire.gen.SkillPlan.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SkillPlanReq::class.java to com.ccmonitor.mobile.core.wire.gen.SkillPlanReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SkillPlanRow::class.java to com.ccmonitor.mobile.core.wire.gen.SkillPlanRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SkillRead::class.java to com.ccmonitor.mobile.core.wire.gen.SkillRead.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SkillReadFile::class.java to com.ccmonitor.mobile.core.wire.gen.SkillReadFile.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SkillReadReq::class.java to com.ccmonitor.mobile.core.wire.gen.SkillReadReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SkillSourceFile::class.java to com.ccmonitor.mobile.core.wire.gen.SkillSourceFile.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SkillSuspect::class.java to com.ccmonitor.mobile.core.wire.gen.SkillSuspect.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SkillTargetFile::class.java to com.ccmonitor.mobile.core.wire.gen.SkillTargetFile.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SlotRow::class.java to com.ccmonitor.mobile.core.wire.gen.SlotRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SlotShow::class.java to com.ccmonitor.mobile.core.wire.gen.SlotShow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SnapshotInflightPayload::class.java to com.ccmonitor.mobile.core.wire.gen.SnapshotInflightPayload.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Span::class.java to com.ccmonitor.mobile.core.wire.gen.Span.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Speaker::class.java to com.ccmonitor.mobile.core.wire.gen.Speaker.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SshCommand::class.java to com.ccmonitor.mobile.core.wire.gen.SshCommand.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SshConnection::class.java to com.ccmonitor.mobile.core.wire.gen.SshConnection.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SshImportGroup::class.java to com.ccmonitor.mobile.core.wire.gen.SshImportGroup.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SshImportMember::class.java to com.ccmonitor.mobile.core.wire.gen.SshImportMember.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SshImportReq::class.java to com.ccmonitor.mobile.core.wire.gen.SshImportReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SshImported::class.java to com.ccmonitor.mobile.core.wire.gen.SshImported.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Stage::class.java to com.ccmonitor.mobile.core.wire.gen.Stage.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.StageRow::class.java to com.ccmonitor.mobile.core.wire.gen.StageRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Staged::class.java to com.ccmonitor.mobile.core.wire.gen.Staged.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.StandingWord::class.java to com.ccmonitor.mobile.core.wire.gen.StandingWord.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.StartedBy::class.java to com.ccmonitor.mobile.core.wire.gen.StartedBy.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.StatReply::class.java to com.ccmonitor.mobile.core.wire.gen.StatReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.StatReq::class.java to com.ccmonitor.mobile.core.wire.gen.StatReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.StepAsk::class.java to com.ccmonitor.mobile.core.wire.gen.StepAsk.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.StepBody::class.java to com.ccmonitor.mobile.core.wire.gen.StepBody.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.StepResult::class.java to com.ccmonitor.mobile.core.wire.gen.StepResult.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.StepWait::class.java to com.ccmonitor.mobile.core.wire.gen.StepWait.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.StrategyFail::class.java to com.ccmonitor.mobile.core.wire.gen.StrategyFail.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.StrategyFailKind::class.java to com.ccmonitor.mobile.core.wire.gen.StrategyFailKind.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.StreamEv::class.java to com.ccmonitor.mobile.core.wire.gen.StreamEv.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SurfaceRow::class.java to com.ccmonitor.mobile.core.wire.gen.SurfaceRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SurfaceState::class.java to com.ccmonitor.mobile.core.wire.gen.SurfaceState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.SyncRange::class.java to com.ccmonitor.mobile.core.wire.gen.SyncRange.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Tab::class.java to com.ccmonitor.mobile.core.wire.gen.Tab.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TabSession::class.java to com.ccmonitor.mobile.core.wire.gen.TabSession.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TailHow::class.java to com.ccmonitor.mobile.core.wire.gen.TailHow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TailPlan::class.java to com.ccmonitor.mobile.core.wire.gen.TailPlan.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Task::class.java to com.ccmonitor.mobile.core.wire.gen.Task.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TaskEnd::class.java to com.ccmonitor.mobile.core.wire.gen.TaskEnd.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TasksList::class.java to com.ccmonitor.mobile.core.wire.gen.TasksList.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TasksListReq::class.java to com.ccmonitor.mobile.core.wire.gen.TasksListReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TermInput::class.java to com.ccmonitor.mobile.core.wire.gen.TermInput.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TermSession::class.java to com.ccmonitor.mobile.core.wire.gen.TermSession.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TermState::class.java to com.ccmonitor.mobile.core.wire.gen.TermState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Terminal::class.java to com.ccmonitor.mobile.core.wire.gen.Terminal.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalAt::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalAt.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalCan::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalCan.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalCanEnd::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalCanEnd.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalCanEndObjValue::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalCanEndObjValue.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalCanInput::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalCanInput.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalCanInputObjValue::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalCanInputObjValue.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalCanInputObjValueNo::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalCanInputObjValueNo.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalChoices::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalChoices.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalEndReq::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalEndReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalEnded::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalEnded.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalFollowEnd::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalFollowEnd.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalHoldReq::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalHoldReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalHost::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalHost.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalInputReq::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalInputReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalItem::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalItem.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalOutcome::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalOutcome.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalPreviewReq::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalPreviewReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalScreen::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalScreen.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalSshReq::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalSshReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalSshReqCwd::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalSshReqCwd.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalSshReqCwdObjValue::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalSshReqCwdObjValue.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalsBoard::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalsBoard.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalsEndItem::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalsEndItem.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalsEndReply::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalsEndReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalsEndReq::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalsEndReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalsList::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalsList.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TerminalsListReq::class.java to com.ccmonitor.mobile.core.wire.gen.TerminalsListReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TextPreview::class.java to com.ccmonitor.mobile.core.wire.gen.TextPreview.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TitleBy::class.java to com.ccmonitor.mobile.core.wire.gen.TitleBy.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TokenUse::class.java to com.ccmonitor.mobile.core.wire.gen.TokenUse.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Tone::class.java to com.ccmonitor.mobile.core.wire.gen.Tone.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ToolCard::class.java to com.ccmonitor.mobile.core.wire.gen.ToolCard.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.ToolStep::class.java to com.ccmonitor.mobile.core.wire.gen.ToolStep.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Topic::class.java to com.ccmonitor.mobile.core.wire.gen.Topic.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TransferCopyAsk::class.java to com.ccmonitor.mobile.core.wire.gen.TransferCopyAsk.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TransferCopyReq::class.java to com.ccmonitor.mobile.core.wire.gen.TransferCopyReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TransferDownloadAsk::class.java to com.ccmonitor.mobile.core.wire.gen.TransferDownloadAsk.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TransferDownloadReq::class.java to com.ccmonitor.mobile.core.wire.gen.TransferDownloadReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TransferOpened::class.java to com.ccmonitor.mobile.core.wire.gen.TransferOpened.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TransferStartReq::class.java to com.ccmonitor.mobile.core.wire.gen.TransferStartReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TransferStopReq::class.java to com.ccmonitor.mobile.core.wire.gen.TransferStopReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TransferUploadAsk::class.java to com.ccmonitor.mobile.core.wire.gen.TransferUploadAsk.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TransferUploadReq::class.java to com.ccmonitor.mobile.core.wire.gen.TransferUploadReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TurnPart::class.java to com.ccmonitor.mobile.core.wire.gen.TurnPart.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TurnPhase::class.java to com.ccmonitor.mobile.core.wire.gen.TurnPhase.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TurnRow::class.java to com.ccmonitor.mobile.core.wire.gen.TurnRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.TurnSpan::class.java to com.ccmonitor.mobile.core.wire.gen.TurnSpan.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.UiErrorPayload::class.java to com.ccmonitor.mobile.core.wire.gen.UiErrorPayload.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Unavailable::class.java to com.ccmonitor.mobile.core.wire.gen.Unavailable.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.UnclearWhy::class.java to com.ccmonitor.mobile.core.wire.gen.UnclearWhy.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.UndoAt::class.java to com.ccmonitor.mobile.core.wire.gen.UndoAt.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.UnfollowReq::class.java to com.ccmonitor.mobile.core.wire.gen.UnfollowReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Uninstalled::class.java to com.ccmonitor.mobile.core.wire.gen.Uninstalled.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.UnknownOnMachine::class.java to com.ccmonitor.mobile.core.wire.gen.UnknownOnMachine.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.UnreadWhy::class.java to com.ccmonitor.mobile.core.wire.gen.UnreadWhy.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.UploadItem::class.java to com.ccmonitor.mobile.core.wire.gen.UploadItem.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Usage::class.java to com.ccmonitor.mobile.core.wire.gen.Usage.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.UsageFact::class.java to com.ccmonitor.mobile.core.wire.gen.UsageFact.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.UserInputRow::class.java to com.ccmonitor.mobile.core.wire.gen.UserInputRow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.UserInputs::class.java to com.ccmonitor.mobile.core.wire.gen.UserInputs.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.UserInputsReq::class.java to com.ccmonitor.mobile.core.wire.gen.UserInputsReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.UserPath::class.java to com.ccmonitor.mobile.core.wire.gen.UserPath.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.UserPathOp::class.java to com.ccmonitor.mobile.core.wire.gen.UserPathOp.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.UserPathReq::class.java to com.ccmonitor.mobile.core.wire.gen.UserPathReq.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.UserPathState::class.java to com.ccmonitor.mobile.core.wire.gen.UserPathState.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.UserText::class.java to com.ccmonitor.mobile.core.wire.gen.UserText.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.VerifyCheck::class.java to com.ccmonitor.mobile.core.wire.gen.VerifyCheck.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.VerifyReport::class.java to com.ccmonitor.mobile.core.wire.gen.VerifyReport.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.VersionRelation::class.java to com.ccmonitor.mobile.core.wire.gen.VersionRelation.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Warmed::class.java to com.ccmonitor.mobile.core.wire.gen.Warmed.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.WatchAsk::class.java to com.ccmonitor.mobile.core.wire.gen.WatchAsk.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.WatchFrom::class.java to com.ccmonitor.mobile.core.wire.gen.WatchFrom.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.WatchReply::class.java to com.ccmonitor.mobile.core.wire.gen.WatchReply.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Whereabout::class.java to com.ccmonitor.mobile.core.wire.gen.Whereabout.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.Whereabouts::class.java to com.ccmonitor.mobile.core.wire.gen.Whereabouts.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.WinFact::class.java to com.ccmonitor.mobile.core.wire.gen.WinFact.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.WindowShow::class.java to com.ccmonitor.mobile.core.wire.gen.WindowShow.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.WirePath::class.java to com.ccmonitor.mobile.core.wire.gen.WirePath.ADAPTER,
            com.ccmonitor.mobile.core.wire.gen.WirePathObjValue::class.java to com.ccmonitor.mobile.core.wire.gen.WirePathObjValue.ADAPTER,
        )
    }
}
