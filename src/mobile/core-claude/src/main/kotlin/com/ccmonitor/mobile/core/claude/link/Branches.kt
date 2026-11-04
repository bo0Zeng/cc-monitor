package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.gen.Branch
import com.ccmonitor.mobile.core.wire.gen.BranchReq
import com.ccmonitor.mobile.core.wire.gen.BranchRun
import com.ccmonitor.mobile.core.wire.gen.Commands
import com.ccmonitor.mobile.core.wire.gen.Field
import com.ccmonitor.mobile.core.wire.gen.Str
import com.ccmonitor.mobile.core.wire.gen.U64
import com.ccmonitor.mobile.core.wire.gen.arr
import com.ccmonitor.mobile.core.wire.gen.obj
import com.squareup.moshi.JsonAdapter

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

/**
 * 流上 `session_branch` 那一帧（`cmd-wire` 的 `frame::SessionBranch`）：那份记录（[path]）的清单前 [at] 条照旧，其后换成 [off]；段首落在这一截里的段 [runs]。
 * `sum` 只给交回 `stream-watch` 的那一端用，手机交 `at: 0`，不收它。
 * 那一型没导出线上类型（生成的类里没有它）⇒ 编解码照生成的那几格的写法在这里写；格名与 `frame.rs` 一一对应。
 */
@Suppress("UNCHECKED_CAST")
data class BranchFrame(
    val sid: String,
    val path: String,
    val at: Long,
    val off: List<String>,
    val runs: List<BranchRun>,
) {
    companion object {
        val ADAPTER: JsonAdapter<BranchFrame> =
            obj(
                listOf(
                    Field("sid", { Str }, optional = false, nullable = false),
                    Field("path", { Str }, optional = false, nullable = false),
                    Field("at", { U64 }, optional = false, nullable = false),
                    Field("off", { arr(Str) }, optional = false, nullable = false),
                    Field("runs", { arr(BranchRun.ADAPTER) }, optional = false, nullable = false),
                ),
                { a -> BranchFrame(a[0] as String, a[1] as String, a[2] as Long, a[3] as List<String>, a[4] as List<BranchRun>) },
                { v -> arrayOf(v.sid, v.path, v.at, v.off, v.runs) },
            )
    }
}
