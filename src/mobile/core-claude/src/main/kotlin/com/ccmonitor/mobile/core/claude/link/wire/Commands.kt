// 生成的文件（tests/scripts/gen-kotlin-wire.mjs 读 ts-rs 导出的声明出的）。不要手改：改 Rust 那一份类型，再跑 `npm run gen:types`。
@file:Suppress("ktlint", "detekt:all")

package com.ccmonitor.mobile.core.claude.link.wire.gen

import com.squareup.moshi.JsonAdapter

/** 手机按类型读回包的那几条命令 ⇒ 回包怎么解（`cmd-wire/src/replies.rs` 那张表 · 类型照 TypedCommands）。 */
object Commands {
    val REPLIES: Map<String, JsonAdapter<*>> = mapOf(
        "accounts-list" to AccountsList.ADAPTER,
        "bookmarks-edit" to Bookmarks.ADAPTER,
        "bookmarks-read" to Bookmarks.ADAPTER,
        "files-chmod" to BatchReply.ADAPTER,
        "files-commit-upload" to FilesPathBytesReply.ADAPTER,
        "files-create" to FilesPathBytesReply.ADAPTER,
        "files-delete" to BatchReply.ADAPTER,
        "files-home" to FilesPathReply.ADAPTER,
        "files-ls" to SyncPatch.adapter(LsHead.ADAPTER, LsEntry.ADAPTER),
        "files-mkdir" to FilesPathReply.ADAPTER,
        "files-read-chunk" to ReadChunkReply.ADAPTER,
        "files-read-text" to ReadTextReply.ADAPTER,
        "files-rename" to FilesPathReply.ADAPTER,
        "files-stage-chunk" to FilesStageChunkReply.ADAPTER,
        "files-stat" to StatReply.ADAPTER,
        "files-write-text" to FilesWrittenReply.ADAPTER,
        "history-lines" to HistoryLines.ADAPTER,
        "history-list" to SyncPatch.adapter(HistoryHead.ADAPTER, HistoryRow.ADAPTER),
        "history-read" to HistoryRead.ADAPTER,
        "history-search" to HistorySearchReply.ADAPTER,
        "history-tail" to HistoryTail.ADAPTER,
        "kill" to Killed.ADAPTER,
        "needs-answer" to Answered.ADAPTER,
        "quota-read" to QuotaRead.ADAPTER,
        "rotation-facts" to RotationFactsReply.ADAPTER,
        "rotation-for-new" to RotDone.ADAPTER,
        "rotation-reload" to RotDone.ADAPTER,
        "rotation-settings" to RotStrategyRev.ADAPTER,
        "rotation-strategies" to RotationStrategiesReply.ADAPTER,
        "rotation-strategy-add" to RotStrategyId.ADAPTER,
        "rotation-strategy-copy" to RotStrategyId.ADAPTER,
        "rotation-strategy-delete" to RotDone.ADAPTER,
        "rotation-strategy-rename" to RotStrategyId.ADAPTER,
        "rotation-strategy-tz" to RotStrategyRev.ADAPTER,
        "rotation-temp" to RotationTempReply.ADAPTER,
        "rotation-try" to RotationTryReply.ADAPTER,
        "rotation-use" to RotDone.ADAPTER,
        "rotation-use-now" to RotDone.ADAPTER,
        "rotation-view" to RotationViewReply.ADAPTER,
        "session-commands" to SessionCommands.ADAPTER,
        "session-model" to ModelSwitched.ADAPTER,
        "session-open-facts" to OpenFacts.ADAPTER,
        "session-open" to Opened.ADAPTER,
        "session-permission" to ModeSwitched.ADAPTER,
        "session-say" to Sent.ADAPTER,
        "sessions-needs" to NeedsList.ADAPTER,
        "terminal-input" to Sent.ADAPTER,
        "terminal-preview" to Preview.ADAPTER,
        "terminals-list" to TerminalsList.ADAPTER,
    )
}
