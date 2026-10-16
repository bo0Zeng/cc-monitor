package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.Annotated
import com.ccmonitor.mobile.core.claude.link.wire.Hide
import com.ccmonitor.mobile.core.claude.link.wire.HistoryListPage
import com.ccmonitor.mobile.core.claude.link.wire.NeedsList
import com.ccmonitor.mobile.core.claude.link.wire.NeedsRow
import com.ccmonitor.mobile.core.claude.link.wire.Tone
import com.ccmonitor.mobile.core.claude.link.wire.Wire

/*
 * 核心成品的解码：只认格、不判、不写字。每一格的意思以 `IPC-COMMANDS.md` 为准；
 * 形状对不上（缺必填格）⇒ `null`，由调用方报「两端契约对不上」，不补默认值。
 */

/** 语气（[Tone]，线上类）的一个词；认不出 ⇒ `null`（照 plain 画、报一次）。会话帧上那几格不经核心按类型解，在这里认。 */
fun toneOf(s: String?): Tone? = Wire.bind<Tone>(s)

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
        /** `{model: {can, why?}, permission: {can, why?}}`；形状认不出 ⇒ `null`。 */
        fun of(m: Map<String, Any?>?): Switches? {
            m ?: return null
            return Switches(CanSwitch.of(m.obj("model")) ?: return null, CanSwitch.of(m.obj("permission")) ?: return null)
        }
    }
}

/** 一样此刻能不能换：[why] 为 `null` ＝ 能换；不能 ⇒ 那台写好的那一句。 */
data class CanSwitch(
    val why: String?,
) {
    val can: Boolean get() = why == null

    companion object {
        fun of(m: Map<String, Any?>?): CanSwitch? =
            when (m?.str("can")) {
                "yes" -> CanSwitch(null)
                "no" -> m.str("why")?.let(::CanSwitch)
                else -> null
            }
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
    /** 线上两个词（`reconnectable` · `ended`）；认不出 ⇒ `null`（不猜）。 */
    enum class State {
        RECONNECTABLE,
        ENDED,
        ;

        companion object {
            fun of(s: String?): State? = entries.firstOrNull { it.name.equals(s, ignoreCase = true) }
        }
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
) {
    /** 喂一帧；不是会话帧 ⇒ 原样返回。缺必填格 ⇒ [onBreak] 报、表不动。 */
    fun step(
        frame: Map<String, Any?>,
        onBreak: (String) -> Unit,
    ): SessionTable {
        val kind = frame.str("kind")
        FOLDS[kind]?.let { (fold, missing) -> return fold(this, frame) ?: also { onBreak(missing) } }
        return when (kind) {
            "sessions_replayed" -> copy(replayed = true)
            else -> this
        }
    }

    private fun added(frame: Map<String, Any?>): SessionTable? = live(frame)?.let { copy(sessions = sessions + (it.sid to it), fates = fates - it.sid) }

    private fun fate(frame: Map<String, Any?>): SessionTable? {
        val sid = frame.str("sid") ?: return null
        val state = SessionFate.State.of(frame.str("state")) ?: return null
        val text = frame.str("state_text") ?: return null
        return copy(fates = fates + (sid to SessionFate(state, text, Switches.of(frame.obj("switch")))))
    }

    private fun removed(frame: Map<String, Any?>): SessionTable? =
        frame.str("sid")?.let { sid ->
            val by = frame.str("by")
            copy(sessions = sessions - sid, successors = if (by != null) successors + (sid to by) else successors)
        }

    /**
     * 状态帧：改那一条的字与语气；带着 `path`（宣告以后记录才出现 · 换了一份，那台在那一行的 line 帧之前先发）⇒ 路径换成它。
     * 宣告时的其余格不动；没宣告过的 sid ⇒ 不动。缺必填格 ⇒ `null`。
     */
    private fun status(frame: Map<String, Any?>): SessionTable? {
        val sid = frame.str("sid") ?: return null
        val text = frame.str("activity_text") ?: return null
        val prior = sessions[sid] ?: return this
        val now =
            prior.copy(
                activity = Toned(text, toneOf(frame.str("activity_tone"))),
                running = frame.str("activity") == RUNNING,
                path = frame.str("path") ?: prior.path,
                switch = Switches.of(frame.obj("switch")),
            )
        return copy(sessions = sessions + (sid to now))
    }

    companion object {
        /** 会话帧 ⇒ 怎么折 ＋ 缺格时报的那一句（折不进 ⇒ `null`）。 */
        private val FOLDS: Map<String, Pair<SessionTable.(Map<String, Any?>) -> SessionTable?, String>> =
            mapOf(
                "session_added" to (SessionTable::added to "session_added 缺 sid / activity_text"),
                "session_status" to (SessionTable::status to "session_status 缺 sid / activity_text"),
                "session_removed" to (SessionTable::removed to "session_removed 缺 sid"),
                "session_state" to (SessionTable::fate to "session_state 缺 sid / state / state_text，或 state 认不出"),
            )

        private fun live(m: Map<String, Any?>): LiveSession? {
            val sid = m.str("sid") ?: return null
            val text = m.str("activity_text") ?: return null
            return LiveSession(
                sid = sid,
                path = m.str("path"),
                cwd = m.str("cwd"),
                name = m.str("name"),
                agentKind = m.str("agent_kind"),
                attachable = m.bool("attachable"),
                activity = Toned(text, toneOf(m.str("activity_tone"))),
                running = m.str("activity") == RUNNING,
                switch = Switches.of(m.obj("switch")),
            )
        }

        /** 帧上 `activity` 那一格判得出「一轮在跑」的那个词（核心的闭集；缺席 ＝ 判不出）。 */
        private const val RUNNING = "working"
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

/** `quota-read`：这台的额度账。每号一段 `rows`（写好的几行）· `fiveHour`（「5h 41%」那一格）· `warm`（开窗那一判）。 */
object QuotaRead {
    const val COMMAND: String = "quota-read"

    data class Account(
        val account: String,
        /** 首行 名 · 类型 · 标签；其余每行 键 · 值 · ↻ · 距今 —— 照抄，不另排。 */
        val rows: List<List<Toned>>,
        /** 「5h 那一格」写好的字；按量号 · 没出过数的号 ⇒ `null`。 */
        val fiveHour: String?,
        /** 开窗那一判那一句。 */
        val warmText: String?,
    )

    data class Answer(
        /** `present` · `absent` · `unreadable`。 */
        val state: String,
        /** 读不出 / 一个号都没有 ⇒ 那一句；否则 `null`。 */
        val text: String?,
        /** 只在 `unreadable` 时有：复制详情。 */
        val detail: String?,
        /** 出过数的号在前、没出过数的在后，各按核心给的先后。 */
        val accounts: List<Account>,
        val usableNow: List<String>,
    )

    private fun account(m: Map<String, Any?>): Account {
        val rows =
            m["rows"].asList().map { row ->
                row.asList().map { cell -> cell.asObj().let { c -> Toned(c.str("text").need(), toneOf(c.str("tone"))) } }
            }
        return Account(m.str("account").need(), rows, m.str("fiveHour"), m.obj("warm")?.str("text"))
    }

    fun of(data: Any?): Answer? =
        decoding {
            val m = data.asObj()
            Answer(
                state = m.str("state").need(),
                text = m.str("text"),
                detail = m.str("detail"),
                accounts = m.objs("accounts").need().map(::account) + m.objs("unseen").need().map(::account),
                usableNow = m.strs("usableNow").orEmpty(),
            )
        }
}
