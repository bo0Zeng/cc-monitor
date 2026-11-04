package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.gen.Branch
import com.ccmonitor.mobile.core.wire.gen.BranchReq
import com.ccmonitor.mobile.core.wire.gen.Commands

/**
 * `history-branch`：一份记录的主线外清单（Esc 回退掉的那几条记录的 `id`）与后端分好的段，一页：清单里从第 [from] 条起的那几条。
 * 下一页从回包的 `next` 起；之后的由流上 `session_branch` 接着说（[CoreLink.branches]）。
 */
object HistoryBranch {
    const val COMMAND: String = Commands.HistoryBranch.NAME

    fun args(
        path: String,
        from: Long,
    ): Map<String, Any?> = Commands.HistoryBranch.args(BranchReq(path = path, from = from))

    fun of(data: Any?): Branch? = Commands.HistoryBranch.reply(data)
}
