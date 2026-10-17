package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.Allow
import com.ccmonitor.mobile.core.claude.link.wire.Annotated
import com.ccmonitor.mobile.core.claude.link.wire.CanWord
import com.ccmonitor.mobile.core.claude.link.wire.Hide
import com.ccmonitor.mobile.core.claude.link.wire.HistoryListPage
import com.ccmonitor.mobile.core.claude.link.wire.HistorySearchReply
import com.ccmonitor.mobile.core.claude.link.wire.Merged
import com.ccmonitor.mobile.core.claude.link.wire.NeedsList
import com.ccmonitor.mobile.core.claude.link.wire.NeedsRow
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

/** `sessions-needs`：这台需手动的整份（在等的会话 · 计划格 · 会话外的），那台排好——出口照这个先后，不另排。核心按 `cmd-wire` 那一份解过。 */
object SessionsNeeds {
    const val COMMAND: String = "sessions-needs"

    /** 会话那几项（照那台的先后）。 */
    fun of(data: Any?): List<NeedsRow>? = list(data)?.items?.mapNotNull { it.session }

    /** 整份（每一项 ＋ 替人答过的那几条 ＋ 会外那一截读不出的那一句）。 */
    fun list(data: Any?): NeedsList? = Wire.bind<NeedsList>(data)
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
    fun ofRows(rows: List<Any?>): HistoryListPage? =
        of(mapOf("rows" to rows, "groups" to emptyList<Any?>(), "notice" to null, "truncated" to false))

    /** 入参：设了的筛选各带一格（与桌面历史同几个参数），没设的不带（那台照缺省）；全没设 ⇒ `null`。 */
    fun args(ask: ListAsk): Map<String, Any?>? =
        buildMap {
            ask.query?.let { put("query", it) }
            ask.withinDays?.let { put("within_days", it) }
            ask.sort?.let { put("sort", it.wire) }
            if (ask.hidden) put("hidden", true)
        }.ifEmpty { null }
}

/** 问 `history-list` 带的筛选：标题里的字 · 最近几天 · 排序 · 隐藏的也出。 */
data class ListAsk(
    val query: String? = null,
    val withinDays: Int? = null,
    val sort: ListSort? = null,
    val hidden: Boolean = false,
)

/** 清单按什么排：最后活动 · 开始时刻。 */
enum class ListSort(
    val wire: String,
) {
    ACTIVITY("activity"),
    CREATED("created"),
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

    /** 标星 / 取消标星。 */
    fun star(
        sid: String,
        on: Boolean,
    ): Map<String, Any?> = mapOf("sid" to sid, "patch" to mapOf("starred" to on))

    /** 改标题（去掉首尾空白）；留空 ⇒ 清掉、回到那台算的标题（`null` 在那台是「不改」，不交它）。 */
    fun rename(
        sid: String,
        title: String,
    ): Map<String, Any?> = mapOf("sid" to sid, "patch" to mapOf("customTitle" to title.trim()))

    /** 回包：改之前那一格（`prev`）；撤销「隐藏」原样交回 [hide]（`null` ⇒ 回到规则，不把照规则显示的钉成单独显示）。 */
    fun of(data: Any?): Annotated? = Wire.bind<Annotated>(data)
}

/** `history-search`：这台的会话内容里搜一个词（不分大小写，子串）。回包里的 `sessions` 原样交 [HistorySearchMerge]。 */
object HistorySearch {
    const val COMMAND: String = "history-search"

    fun args(query: String): Map<String, Any?> = mapOf("query" to query)

    fun of(data: Any?): HistorySearchReply? = Wire.bind<HistorySearchReply>(data)

    /** 回包里的 `sessions[]` 原样（交合并那一问，不经这里的类重新序列化：这里只写手机读的几格，重写会丢格）。 */
    fun rawSessions(data: Any?): List<Any?>? = ((data as? Map<*, *>)?.get("sessions") as? List<*>)?.toList()
}

/** `history-search-merge`：几台的搜索结果交一台合成一份（排序 · 时刻字都在那台）。 */
object HistorySearchMerge {
    const val COMMAND: String = "history-search-merge"

    /** 合并的那一台自己 ⇒ 这个名字（线上逐字）。 */
    const val LOCAL: String = "<local>"

    /** [machines]：每台 `(名字, 它答回来的 sessions 原样)`。 */
    fun args(machines: List<Pair<String, List<Any?>>>): Map<String, Any?> = mapOf("machines" to machines.map { (origin, sessions) -> mapOf("origin" to origin, "sessions" to sessions) })

    fun of(data: Any?): Merged? = Wire.bind<Merged>(data)
}
