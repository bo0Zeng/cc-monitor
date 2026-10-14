package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.HistoryListPage
import com.ccmonitor.mobile.core.claude.link.wire.NeedsList
import com.ccmonitor.mobile.core.claude.link.wire.NeedsRow
import com.ccmonitor.mobile.core.claude.link.wire.Tone
import com.ccmonitor.mobile.core.claude.link.wire.Wire

/*
 * 核心成品的解码：只认格、不判、不写字（`架构.md` D3 · D10）。每一格的意思以 `IPC-COMMANDS.md` 为准；
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
)

/**
 * 会话表：把流上的会话帧折成「此刻有哪些会话、各自怎么样」。纯函数，一帧一步。
 * `overflow` 里丢了会话帧 ⇒ [needsResync]（这张表不可信了，重接一次让后端整份重报）。
 */
data class SessionTable(
    val sessions: Map<String, LiveSession> = emptyMap(),
    /** 后端说「清单报完了」（`sessions_replayed`）。 */
    val replayed: Boolean = false,
    val needsResync: Boolean = false,
) {
    /** 喂一帧；不是会话帧 ⇒ 原样返回。缺必填格 ⇒ [onBreak] 报、表不动。 */
    fun step(
        frame: Map<String, Any?>,
        onBreak: (String) -> Unit,
    ): SessionTable =
        when (frame.str("kind")) {
            "session_added" -> added(frame) ?: also { onBreak("session_added 缺 sid / activity_text") }
            "session_status" -> status(frame) ?: also { onBreak("session_status 缺 sid / activity_text") }
            "session_removed" -> removed(frame) ?: also { onBreak("session_removed 缺 sid") }
            "sessions_replayed" -> copy(replayed = true)
            "overflow" -> if (lostSessionFrames(frame)) copy(needsResync = true) else this
            else -> this
        }

    private fun added(frame: Map<String, Any?>): SessionTable? = live(frame)?.let { copy(sessions = sessions + (it.sid to it)) }

    private fun removed(frame: Map<String, Any?>): SessionTable? = frame.str("sid")?.let { copy(sessions = sessions - it) }

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
            )
        return copy(sessions = sessions + (sid to now))
    }

    companion object {
        private val SESSION_KINDS = setOf("session_added", "session_status", "session_removed", "session_state")

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
            )
        }

        /** 帧上 `activity` 那一格判得出「一轮在跑」的那个词（核心的闭集；缺席 ＝ 判不出）。 */
        private const val RUNNING = "working"

        /** 丢的里头有会话帧（或身份表截过）⇒ 一次性结论丢了，别处补不回来。 */
        private fun lostSessionFrames(m: Map<String, Any?>): Boolean =
            m.bool("lost_truncated") == true || m.objs("lost").orEmpty().any { it.str("kind") in SESSION_KINDS }
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
