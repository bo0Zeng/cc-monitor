package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.Allow
import com.ccmonitor.mobile.core.claude.link.wire.Annotated
import com.ccmonitor.mobile.core.claude.link.wire.CanWord
import com.ccmonitor.mobile.core.claude.link.wire.Hide
import com.ccmonitor.mobile.core.claude.link.wire.HistoryListPage
import com.ccmonitor.mobile.core.claude.link.wire.NeedsList
import com.ccmonitor.mobile.core.claude.link.wire.NeedsRow
import com.ccmonitor.mobile.core.claude.link.wire.QuotaReadReply
import com.ccmonitor.mobile.core.claude.link.wire.SessionActivityWord
import com.ccmonitor.mobile.core.claude.link.wire.SessionAddedFrame
import com.ccmonitor.mobile.core.claude.link.wire.SessionFace
import com.ccmonitor.mobile.core.claude.link.wire.SessionFileRereadFrame
import com.ccmonitor.mobile.core.claude.link.wire.SessionRemovedFrame
import com.ccmonitor.mobile.core.claude.link.wire.SessionStateFrame
import com.ccmonitor.mobile.core.claude.link.wire.SessionStatusFrame
import com.ccmonitor.mobile.core.claude.link.wire.Tone
import com.ccmonitor.mobile.core.claude.link.wire.Wire

/*
 * 核心成品的解码：只认格、不判、不写字。每一格的意思以 `IPC-COMMANDS.md` 为准；
 * 形状对不上（缺必填格）⇒ `null`，由调用方报「两端契约对不上」，不补默认值。
 */

/** 一格写好的字 ＋ 它的语气。 */
data class Toned(
    val text: String,
    val tone: Tone?,
)

/** 一样此刻做不了 ⇒ 那台写好的那一句；做得了 ⇒ `null`。`{can: no}` 却没带那一句 ⇒ 照「不能」、那一句空着。 */
fun Allow.blocked(): String? = if (can == CanWord.YES) null else why.orEmpty()

/** 会话帧里那一条会话的宣告格（`session_added` 折出来、`session_status` 带来的路径换上）；此刻什么样在 [SessionTable.faces]。 */
data class LiveSession(
    val sid: String,
    val path: String? = null,
    val cwd: String? = null,
    val name: String? = null,
    val agentKind: String? = null,
    val attachable: Boolean? = null,
)

/**
 * 会话表：把流上的会话帧折成「此刻有哪些会话、各自怎么样」。纯函数，一帧一步。
 * 拥塞丢了帧由那台在原连接上整份重报（覆盖式折进来），这里不判、不重接。
 */
data class SessionTable(
    val sessions: Map<String, LiveSession> = emptyMap(),
    /** 每条会话那台给的最新一份整份脸（宣告 · 状态 · 离开活三帧都带）；摘掉了还没裁去向 ⇒ 没有。 */
    val faces: Map<String, SessionFace> = emptyMap(),
    /** 后端说「清单报完了」（`sessions_replayed`）。 */
    val replayed: Boolean = false,
    /** 被顶替的会话 ⇒ 顶替它的那一条（`session_removed.by`：清空之后同一个进程另起的那一份记录）。 */
    val successors: Map<String, String> = emptyMap(),
    /** 每条会话的记录被从头重读过几回（见 [generation]）。 */
    val rereads: Map<String, Long> = emptyMap(),
) {
    /** 那一条的记录被从头重读过几回（`session_file_reread`）：值变了 ⇒ 读过的那一代作废、整份重来。 */
    fun generation(sid: String): Long = rereads[sid] ?: 0L

    /** 那台判得出那一条一轮在跑（聊天屏的停止键只认它）；判不出 · 不在 ⇒ 不算。 */
    fun running(sid: String): Boolean = faces[sid]?.live?.activity == SessionActivityWord.WORKING

    /** 喂一帧（核心按 `cmd-wire` 那一份类型重写过）；不是会话帧 ⇒ 原样返回。装不进声明的类 ⇒ [onBreak] 报、表不动。 */
    fun step(
        frame: Map<String, Any?>,
        onBreak: (String) -> Unit,
    ): SessionTable {
        val kind = frame.str("kind")
        val folded: SessionTable? =
            when (kind) {
                "session_added" -> Wire.bind<SessionAddedFrame>(frame)?.let(::added)
                "session_status" -> Wire.bind<SessionStatusFrame>(frame)?.let(::status)
                "session_removed" -> Wire.bind<SessionRemovedFrame>(frame)?.let(::removed)
                "session_state" -> Wire.bind<SessionStateFrame>(frame)?.let { copy(faces = faces + (it.sid to it.face)) }
                "session_file_reread" -> Wire.bind<SessionFileRereadFrame>(frame)?.let { copy(rereads = rereads + (it.sid to generation(it.sid) + 1)) }
                "sessions_replayed" -> return copy(replayed = true)
                else -> return this
            }
        return folded ?: also { onBreak("$kind 装不进声明的那一份") }
    }

    private fun added(f: SessionAddedFrame): SessionTable {
        val s = LiveSession(sid = f.sid, path = f.path, cwd = f.cwd, name = f.name, agentKind = f.agentKind, attachable = f.attachable)
        return copy(sessions = sessions + (s.sid to s), faces = faces + (s.sid to f.face))
    }

    private fun removed(f: SessionRemovedFrame): SessionTable =
        copy(sessions = sessions - f.sid, faces = faces - f.sid, successors = if (f.by != null) successors + (f.sid to f.by) else successors)

    /** 状态帧：换上那一条的脸；带着 `path`（宣告以后记录才出现 · 换了一份，那台在那一行的 line 帧之前先发）⇒ 路径换成它。 */
    private fun status(f: SessionStatusFrame): SessionTable {
        val moved = f.path?.let { p -> sessions[f.sid]?.copy(path = p) }
        return copy(sessions = if (moved != null) sessions + (f.sid to moved) else sessions, faces = faces + (f.sid to f.face))
    }
}

/** `sessions-needs`：这台需手动的会话，已按 `rank` 排（同一档等得久的在前）——出口照这个先后，不另排。核心按 `cmd-wire` 那一份解过。 */
object SessionsNeeds {
    const val COMMAND: String = "sessions-needs"

    fun of(data: Any?): List<NeedsRow>? = Wire.bind<NeedsList>(data)?.waiting
}

/**
 * `history-list` 的一行与整份。手机只连一台、直接问那台的常驻：不带 `origin` 也不带 `listing` ⇒
 * 那台自己的清单、并上那台的注解、筛、排（`history/history_list.rs` 头注「origin 缺席 ⇒ 这台」）。
 */
object HistoryList {
    const val COMMAND: String = "history-list"

    /** 核心按 `cmd-wire` 那一份解过的成品（平时那一形；`raw` 那一形手机不问）。 */
    fun of(data: Any?): HistoryListPage? = Wire.bind<HistoryListPage>(data)

    /** 回包里的 `rows[]` 原样（缓存一行存一格）；不是那一形 ⇒ `null`。 */
    fun rawRows(data: Any?): List<Any?>? = ((data as? Map<*, *>)?.get("rows") as? List<*>)?.toList()

    /** 缓存里那几行（原样）拼回一份清单，交给同一个解码器（[of]）；注解与截断那两格缓存不存。 */
    fun ofRows(rows: List<Any?>): HistoryListPage? = of(mapOf("rows" to rows, "notice" to null, "truncated" to false))

    /** 入参：「显示已隐藏」开着 ⇒ `hidden: true`（隐藏的也出，与桌面历史同一个参数）；关着不带（那台缺省不出）。 */
    fun args(showHidden: Boolean): Map<String, Any?>? = if (showHidden) mapOf("hidden" to true) else null
}

/** `history-annotate`：改那台注解里单个隐藏那一格（与桌面历史 · 标签栏同一条命令）。 */
object HistoryAnnotate {
    const val COMMAND: String = "history-annotate"

    /** 单个那一格交成什么：`ON` 隐藏 · `OFF` 明说显示 · `null` 摘掉、回到规则（`rule`）。 */
    fun hide(
        sid: String,
        pin: Hide?,
    ): Map<String, Any?> =
        mapOf(
            "sid" to sid,
            "patch" to
                mapOf(
                    "hide" to
                        when (pin) {
                            Hide.ON -> "on"
                            Hide.OFF -> "off"
                            null -> "rule"
                        },
                ),
        )

    /** 回包：改之前那一格（`prev`）；撤销「隐藏」原样交回 [hide]（`null` ⇒ 回到规则，不把照规则显示的钉成单独显示）。 */
    fun of(data: Any?): Annotated? = Wire.bind<Annotated>(data)
}

/** `quota-read`：这台的额度账。手机只读每个出过数的号「5h 那一格」写好的字（新会话框每个号后面那一格）。 */
object QuotaRead {
    const val COMMAND: String = "quota-read"

    fun of(data: Any?): QuotaReadReply? = Wire.bind<QuotaReadReply>(data)
}
