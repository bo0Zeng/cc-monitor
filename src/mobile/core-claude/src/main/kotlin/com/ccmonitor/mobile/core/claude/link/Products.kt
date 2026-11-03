package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.RawPatch
import com.ccmonitor.mobile.core.claude.link.wire.SyncHand
import com.ccmonitor.mobile.core.claude.link.wire.rawPatch
import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.Allow
import com.ccmonitor.mobile.core.wire.gen.Annotated
import com.ccmonitor.mobile.core.wire.gen.GroupsRead
import com.ccmonitor.mobile.core.wire.gen.Hide
import com.ccmonitor.mobile.core.wire.gen.HistoryGroup
import com.ccmonitor.mobile.core.wire.gen.HistoryHead
import com.ccmonitor.mobile.core.wire.gen.HistoryMachineRead
import com.ccmonitor.mobile.core.wire.gen.HistoryRow
import com.ccmonitor.mobile.core.wire.gen.LOCAL
import com.ccmonitor.mobile.core.wire.gen.Merged
import com.ccmonitor.mobile.core.wire.gen.NeedsItem
import com.ccmonitor.mobile.core.wire.gen.NeedsList
import com.ccmonitor.mobile.core.wire.gen.NeedsRow
import com.ccmonitor.mobile.core.wire.gen.SessionBook
import com.ccmonitor.mobile.core.wire.gen.SessionFace
import com.ccmonitor.mobile.core.wire.gen.Tone

/*
 * 核心成品的解码：只认格、不判、不写字。每一格的意思以 `IPC-COMMANDS.md` 为准；
 * 形状对不上（缺必填格）⇒ `null`，由调用方报「两端契约对不上」，不补默认值。
 */

/** 一格写好的字 ＋ 它的语气。 */
data class Toned(
    val text: String,
    val tone: Tone?,
)

/** 一样此刻做不了 ⇒ 那台写好的那一句；做得了 ⇒ `null`。 */
fun Allow.blocked(): String? =
    when (this) {
        Allow.Yes -> null
        is Allow.No -> why
    }

/*
 * 会话簿（[SessionBook]）是嵌的 Rust 核心折好交来的整份（`face_core::book`，桌面壳同一份折法）：活着的（[SessionBook.live]）·
 * 走了的（[SessionBook.left]）· 顶替 · 重读几回 · 清单报完没有。这里只有几个读法，不折。
 */

/** 还没有一帧的那一本（连上之前 · 换了一条流）。 */
val EMPTY_BOOK: SessionBook = SessionBook(emptyMap(), emptyMap(), emptyMap(), emptyMap(), emptyMap(), false)

/** 那一条此刻的整份脸：活的 ⇒ 最新状态那一张；走了的 ⇒ 去向那一张；簿里没有 ⇒ `null`。 */
fun SessionBook.face(sid: String): SessionFace? = live[sid]?.face ?: left[sid]?.face

/** 簿里每一条此刻的整份脸。 */
fun SessionBook.faces(): Map<String, SessionFace> = left.mapValues { it.value.face } + live.mapValues { it.value.face }

/** 那一条的记录被从头重读过几回（`session_file_reread`）：值变了 ⇒ 读过的那一代作废、整份重来。 */
fun SessionBook.generation(sid: String): Long = rereads[sid] ?: 0L

/** 那台说那一条此刻停得下（`face.can.stop`，一轮在跑时才是；聊天屏的停止键只认它）；不在 ⇒ 不算。 */
fun SessionBook.canStop(sid: String): Boolean = face(sid)?.can?.stop == Allow.Yes

/** `sessions-needs`：这台需手动的整份（在等的会话 · 计划格 · 会话外的），那台排好——出口照这个先后，不另排。核心按 `cmd-wire` 那一份解过。 */
object SessionsNeeds {
    const val COMMAND: String = "sessions-needs"

    /** 会话那几项（照那台的先后）。 */
    fun of(data: Any?): List<NeedsRow>? = list(data)?.items?.mapNotNull { (it as? NeedsItem.Session)?.session }

    /** 整份（每一项 ＋ 替人答过的那几条 ＋ 会外那一截读不出的那一句）。 */
    fun list(data: Any?): NeedsList? = Wire.bind<NeedsList>(data)
}

/**
 * `history-list` 的一行与整份。手机只连一台、直接问那台的常驻：`origin` 交 [LOCAL]（那台眼里的这台）⇒
 * 那台自己的清单、并上那台的注解、筛、排（`history/history_list.rs` 头注；`origin` 必填，缺了那台答 `bad_args`）。
 * 回包是带键集合那一形（[SyncPatch]）：带着上次的记号问 ⇒ 没变 / 只给变了的那几行，手上那一份照它换（[SyncHand.apply]）。
 */
object HistoryList {
    const val COMMAND: String = "history-list"

    /** 回包（核心按 `cmd-wire` 那一份解过）；不是那一形（`raw` 那一形手机不问）⇒ `null`。 */
    fun patch(data: Any?): RawPatch? = rawPatch(data)

    /** 不带记号那一问的回包（整份）⇒ 一份清单；不是整份 · 解不开 ⇒ `null`。 */
    fun of(data: Any?): HistoryListPage? = patch(data)?.takeIf { it.whole }?.let { page(SyncHand().apply(it)) }

    /** 手上那一份 ⇒ 要画的清单：行照先后、每行与头交给同一个解码器；窗外还有 ⇒ [HistoryListPage.truncated]。有一格解不开 ⇒ `null`。 */
    fun page(hand: SyncHand): HistoryListPage? {
        val rows = hand.order.map { k -> hand.items[k]?.let { Wire.bind<HistoryRow>(it) } ?: return null }
        val info = hand.head?.let { Wire.bind<HistoryHead>(it.info) ?: return null }
        return HistoryListPage(rows, info?.groups.orEmpty(), info?.notice, (hand.head?.total ?: 0) > rows.size)
    }

    /** 入参：`origin` ＝ [LOCAL]；设了的筛选各带一格（与桌面历史同几个参数），没设的不带（那台照缺省）；带着上次的记号 [held] ⇒ 交回。 */
    fun args(
        ask: ListAsk,
        held: String? = null,
    ): Map<String, Any?> =
        buildMap {
            put("origin", LOCAL)
            ask.query?.let { put("query", it) }
            ask.withinDays?.let { put("within_days", it) }
            ask.sort?.let { put("sort", it.wire) }
            if (ask.hidden) put("hidden", true)
            held?.let { put("held", it) }
        }
}

/** 一台的清单：那一窗的行（照那台的先后）· 组 · 注解没并上那一句 · 窗外还有没有。 */
data class HistoryListPage(
    val rows: List<HistoryRow>,
    val groups: List<HistoryGroup>,
    val notice: String?,
    val truncated: Boolean,
)

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
        hide: Hide?,
    ): Map<String, Any?> =
        mapOf(
            "sid" to sid,
            "patch" to
                mapOf(
                    "hide" to
                        when (hide) {
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

/**
 * `groups-read`：手机直连一台、问它自己那一片（`machines` 只写 `<local>`）。组名 · 组头那几句（`counts[].text`）· 组员的脸与标题
 * 都是那台写好的；几台的组不在这里合。
 */
object Groups {
    const val COMMAND: String = "groups-read"

    val ARGS: Map<String, Any?> = mapOf("machines" to listOf(LOCAL))

    /** 回包（核心按 `cmd-wire` 那一份解过）；缺必填格 ⇒ `null`。 */
    fun of(data: Any?): GroupsRead? = Wire.bind<GroupsRead>(data)

    /** 那台这一问没读全的那几句（上次所知 · 读不到，照那台写的）。 */
    fun unread(r: GroupsRead): List<String> =
        r.machines.mapNotNull {
            when (val read = it.read) {
                is HistoryMachineRead.Stale -> read.why
                is HistoryMachineRead.Failed -> read.why
                is HistoryMachineRead.Ok, HistoryMachineRead.Unread -> null
            }
        }
}

/** `history-search`：这台的会话内容里搜一个词（不分大小写，子串）。回包原样交 [SearchMerge]（手机不解各台那一份）。 */
object HistorySearch {
    const val COMMAND: String = "history-search"

    fun args(query: String): Map<String, Any?> = mapOf("query" to query)
}

/**
 * 几台搜内容的结果合成一份：手机逐台直连问完，交嵌的 Rust 核心合（`face_core::search::merge`，本机后端扇出完调的同一份），
 * 不交哪一台当信差。每台原样交它的回包（核心按类型解，整数照整数写）。
 */
object SearchMerge {
    /** [q] 搜的词（结果头那一句由核心写）；[answered]：每台 `(hostId, 它 history-search 回包原样)` ⇒ 交核心的那一行。 */
    fun input(
        q: String,
        answered: List<Pair<String, Any?>>,
    ): String = Json.line(mapOf("q" to q, "answered" to answered.map { (origin, reply) -> mapOf("origin" to origin, "reply" to reply) }))

    /** 核心合好的那一行；读不动 ⇒ `null`。 */
    fun of(text: String): Merged? = Json.value(text)?.let { Wire.bind<Merged>(it) }
}
