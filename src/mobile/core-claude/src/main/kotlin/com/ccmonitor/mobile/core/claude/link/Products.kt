package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.Annotated
import com.ccmonitor.mobile.core.claude.link.wire.CanSwitchCell
import com.ccmonitor.mobile.core.claude.link.wire.CanWord
import com.ccmonitor.mobile.core.claude.link.wire.Hide
import com.ccmonitor.mobile.core.claude.link.wire.HistoryListPage
import com.ccmonitor.mobile.core.claude.link.wire.NeedsList
import com.ccmonitor.mobile.core.claude.link.wire.NeedsRow
import com.ccmonitor.mobile.core.claude.link.wire.QuotaReadReply
import com.ccmonitor.mobile.core.claude.link.wire.SessionActivityWord
import com.ccmonitor.mobile.core.claude.link.wire.SessionAddedFrame
import com.ccmonitor.mobile.core.claude.link.wire.SessionFateWord
import com.ccmonitor.mobile.core.claude.link.wire.SessionFileRereadFrame
import com.ccmonitor.mobile.core.claude.link.wire.SessionRemovedFrame
import com.ccmonitor.mobile.core.claude.link.wire.SessionStateFrame
import com.ccmonitor.mobile.core.claude.link.wire.SessionStatusFrame
import com.ccmonitor.mobile.core.claude.link.wire.SwitchCell
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

/** 会话帧里那一条会话此刻的样子（`session_added` / `session_status` 折出来的）。 */
data class LiveSession(
    val sid: String,
    val path: String? = null,
    val cwd: String? = null,
    val name: String? = null,
    val agentKind: String? = null,
    val attachable: Boolean? = null,
    /** 「运行中 · 空闲 · 后台任务运行中 · 需手动 · 状态不明 · 读记录中」那一句与语气，照抄。 */
    val activity: Toned,
    /** 那台判得出它一轮在跑（帧上 `activity` 是 `working`）。判不出（`activity` 缺席：状态不明 · 读记录中）⇒ 不算在跑。聊天屏的停止键只认它。 */
    val running: Boolean = false,
    /** 此刻能不能换模型 · 权限档（那台判的）；帧上没有这一格 ⇒ `null`（不灰，由两条命令自己照实回）。 */
    val switch: Switches? = null,
)

/** 会话帧 `switch` 那一格：两样各自此刻能不能换。 */
data class Switches(
    val model: CanSwitch,
    val permission: CanSwitch,
) {
    companion object {
        fun of(c: SwitchCell): Switches = Switches(CanSwitch.of(c.model), CanSwitch.of(c.permission))
    }
}

/** 一样此刻能不能换：[why] 为 `null` ＝ 能换；不能 ⇒ 那台写好的那一句。 */
data class CanSwitch(
    val why: String?,
) {
    val can: Boolean get() = why == null

    companion object {
        /** `{can: no}` 却没带那一句 ⇒ 当能换说不出为什么：照「不能」灰、那一行空着。 */
        fun of(c: CanSwitchCell): CanSwitch = CanSwitch(if (c.can == CanWord.YES) null else c.why.orEmpty())
    }
}

/** `session_state` 帧：一条会话离开「活」之后的去向（那台后端自己裁）＋ 写好的短名。 */
data class SessionFate(
    val state: State,
    /** 「Claude 已退出」「已结束」（`state_text`，照抄）。 */
    val text: String,
    /** 离开「活」之后能不能换（那台判的，帧上的 `switch`）；没有 ⇒ `null`。 */
    val switch: Switches? = null,
) {
    /** 线上两个词（`reconnectable` · `ended`）。 */
    enum class State {
        RECONNECTABLE,
        ENDED,
    }
}

/**
 * 状态点九种（V10，与桌面 `kit/status-dot` 的 `DotState` 同一套）：颜色 ＝ 在干什么，形状 ＝ 进程还在不在。
 * 只照核心的格选（[of]），不看字；记录已不在这一种会话列表里没有对应的格，留给有格的地方用。
 * 另加机器那一颗的 [UP]（已连接：实心成功色、不呼吸，同桌面 `MachineDotState.up`）；机器连接失败 ＝ [FAILED]。
 */
enum class SessionDot {
    RUNNING,
    NEEDS_YOU,
    BACKGROUND,
    IDLE,
    EXITED,
    ENDED,
    UNKNOWN,
    GONE,
    FAILED,
    UP,
    ;

    companion object {
        /** 活着：照语气（同桌面 `activityFace`：还没收到 · `now` ⇒ 在跑，`need` ⇒ 需手动，`busy` ⇒ 后台任务，其余 ⇒ 空闲）。 */
        fun live(tone: Tone?): SessionDot =
            when (tone) {
                null, Tone.NOW -> RUNNING
                Tone.NEED -> NEEDS_YOU
                Tone.BUSY -> BACKGROUND
                Tone.PLAIN, Tone.FAIL, Tone.WARN -> IDLE
            }

        /**
         * 一条会话的点：流上活着 ⇒ 照语气；离开活了 ⇒ 照 `session_state` 的去向（可重连 ⇒ 已退出，只能恢复 ⇒ 已结束）；
         * 流上没有它 ⇒ 照 `history-list` 那一行的 `status`（`reconnectable` ⇒ 已退出 · `ended` · `unknown` 说不清 · `live` 还没报到 ⇒ 同还没收到语气）。
         * 认不出 ⇒ `null`（不画）。
         */
        fun of(
            live: LiveSession?,
            fate: SessionFate?,
            listed: String?,
        ): SessionDot? {
            if (live != null) return live(live.activity.tone)
            when (fate?.state) {
                SessionFate.State.RECONNECTABLE -> return EXITED
                SessionFate.State.ENDED -> return ENDED
                null -> Unit
            }
            return when (listed) {
                "live" -> RUNNING
                "reconnectable" -> EXITED
                "ended" -> ENDED
                "unknown" -> UNKNOWN
                else -> null
            }
        }
    }
}

/**
 * 会话表：把流上的会话帧折成「此刻有哪些会话、各自怎么样」。纯函数，一帧一步。
 * 拥塞丢了帧由那台在原连接上整份重报（覆盖式折进来），这里不判、不重接。
 */
data class SessionTable(
    val sessions: Map<String, LiveSession> = emptyMap(),
    /** 离开「活」的那几条去向（`session_state`）：可重连 · 已结束；又宣告了 ⇒ 抹掉。 */
    val fates: Map<String, SessionFate> = emptyMap(),
    /** 后端说「清单报完了」（`sessions_replayed`）。 */
    val replayed: Boolean = false,
    /** 被顶替的会话 ⇒ 顶替它的那一条（`session_removed.by`：清空之后同一个进程另起的那一份记录）。 */
    val successors: Map<String, String> = emptyMap(),
    /** 每条会话的记录被从头重读过几回（见 [generation]）。 */
    val rereads: Map<String, Long> = emptyMap(),
) {
    /** 那一条的记录被从头重读过几回（`session_file_reread`）：值变了 ⇒ 读过的那一代作废、整份重来。 */
    fun generation(sid: String): Long = rereads[sid] ?: 0L

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
                "session_state" -> Wire.bind<SessionStateFrame>(frame)?.let(::fate)
                "session_file_reread" -> Wire.bind<SessionFileRereadFrame>(frame)?.let { copy(rereads = rereads + (it.sid to generation(it.sid) + 1)) }
                "sessions_replayed" -> return copy(replayed = true)
                else -> return this
            }
        return folded ?: also { onBreak("$kind 装不进声明的那一份") }
    }

    private fun added(f: SessionAddedFrame): SessionTable {
        val s =
            LiveSession(
                sid = f.sid,
                path = f.path,
                cwd = f.cwd,
                name = f.name,
                agentKind = f.agentKind,
                attachable = f.attachable,
                activity = Toned(f.activityText, f.activityTone),
                running = f.activity == SessionActivityWord.WORKING,
                switch = Switches.of(f.switch),
            )
        return copy(sessions = sessions + (s.sid to s), fates = fates - s.sid)
    }

    private fun fate(f: SessionStateFrame): SessionTable {
        val state =
            when (f.state) {
                SessionFateWord.RECONNECTABLE -> SessionFate.State.RECONNECTABLE
                SessionFateWord.ENDED -> SessionFate.State.ENDED
            }
        return copy(fates = fates + (f.sid to SessionFate(state, f.stateText, Switches.of(f.switch))))
    }

    private fun removed(f: SessionRemovedFrame): SessionTable =
        copy(sessions = sessions - f.sid, successors = if (f.by != null) successors + (f.sid to f.by) else successors)

    /**
     * 状态帧：改那一条的字与语气；带着 `path`（宣告以后记录才出现 · 换了一份，那台在那一行的 line 帧之前先发）⇒ 路径换成它。
     * 宣告时的其余格不动；没宣告过的 sid ⇒ 不动。
     */
    private fun status(f: SessionStatusFrame): SessionTable {
        val prior = sessions[f.sid] ?: return this
        val now =
            prior.copy(
                activity = Toned(f.activityText, f.activityTone),
                running = f.activity == SessionActivityWord.WORKING,
                path = f.path ?: prior.path,
                switch = Switches.of(f.switch),
            )
        return copy(sessions = sessions + (f.sid to now))
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
