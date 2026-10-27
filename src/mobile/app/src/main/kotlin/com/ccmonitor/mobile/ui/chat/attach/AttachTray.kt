package com.ccmonitor.mobile.ui.chat.attach

import com.ccmonitor.mobile.core.claude.link.AttachAsk
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.FilesCommitUpload
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionAttachmentCheck
import com.ccmonitor.mobile.core.claude.link.SessionAttachmentCommit
import com.ccmonitor.mobile.core.claude.link.SessionAttachmentDrop
import com.ccmonitor.mobile.core.claude.link.Transfer
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import java.io.IOException
import java.io.InputStream
import kotlin.coroutines.CoroutineContext

/** 一份附件此刻走到哪。 */
sealed interface AttachPhase {
    /** 排队：还没轮到 · 断着等连上。 */
    data object Queued : AttachPhase

    /** 传输中：已送 [done] / 共 [total] 字节（核心分块循环报的）。 */
    data class Moving(
        val done: Long,
        val total: Long,
    ) : AttachPhase

    /** 已落好：那台回的编号（送那一句时交它）。 */
    data class Landed(
        val id: String,
    ) : AttachPhase

    /** 开传前那台就不收：那台写好的那一句（写在卡上）。 */
    data class Refused(
        val said: String,
    ) : AttachPhase

    /** 传 · 落没成：那台那一句（进错条，可重试）。 */
    data class Failed(
        val said: String,
    ) : AttachPhase
}

/** 输入框上方的一张卡。 */
data class AttachCard(
    val key: String,
    val name: String,
    val bytes: Long,
    /** 从拍照 · 图片进来的 ⇒ 画缩略图；从文件进来的 ⇒ 名 ＋ 状态的小卡。 */
    val image: Boolean,
    /** 本机那一份副本（缩略图照它画）。 */
    val file: File,
    val phase: AttachPhase,
)

/** 卡下那一条：那一句 ＋ 按钮（没有按钮 ⇒ [act] 为 `null`）。 */
data class AttachNote(
    val text: String,
    val act: Act?,
) {
    enum class Act {
        /** 那台的错：重传失败的那几份。 */
        RETRY,

        /** 手机这边存不下：再挑一份。 */
        REPICK,
    }
}

/** 那一排此刻的样子。 */
data class AttachUi(
    val cards: List<AttachCard> = emptyList(),
    val note: AttachNote? = null,
    /** 断着：排队的卡画虚线。 */
    val offline: Boolean = false,
) {
    /** 全落好了（没有卡也算）：➤ 才亮。 */
    val ready: Boolean get() = cards.all { it.phase is AttachPhase.Landed }
}

/**
 * 一条会话输入框上方那一排附件：选好一份先拷进本机（[AttachStore]，被杀回来照样在），再逐份
 * `session-attachment-check`（整排按先后问，收不收 · 上限全由那台判）→ 分块送进暂存区（[CoreLink.transfer]，循环在核心）→
 * `session-attachment-commit` ⇒ 编号。断着 · 送到一半断了 ⇒ 回排队、连上（[CoreLink.state] 通了 · [CoreLink.epoch] 换了）接着传。
 * 同一刻只传一份。
 */
class AttachTray(
    private val link: CoreLink,
    private val machine: String,
    private val store: AttachStore,
    private val scope: CoroutineScope,
    private val sid: () -> String?,
    /** 现开本机那一份的描述符交给核心（核心用完就关）。 */
    private val fdOf: (File) -> Int,
    private val io: CoroutineContext = Dispatchers.IO,
) {
    private val _state = MutableStateFlow(AttachUi())
    val state: StateFlow<AttachUi> = _state.asStateFlow()

    /** 手机这边那一句（存不下）：下一次加 · 撤就收掉。 */
    private var local: AttachNote? = null

    /** 最近一趟断在路上（流还算通着）：等 [CoreLink.epoch] 换了再走。 */
    private var cut = false
    private var pumping: Job? = null
    private var opened = false

    /** 开屏：照本机记着的那一排摆上（已落的不重传），盯着连接，通了就接着传。 */
    fun open() {
        if (opened) return
        opened = true
        val s = sid() ?: return
        val cards = store.list(s).map { k -> AttachCard(k.key, k.name, k.bytes, k.image, k.file, k.id?.let { AttachPhase.Landed(it) } ?: AttachPhase.Queued) }
        publish(cards)
        scope.launch { link.state.collect { if (it is LinkLine.Up) kick() else publish() } }
        scope.launch {
            link.epoch.drop(1).collect {
                cut = false
                kick()
            }
        }
    }

    /** 选好一份：拷进本机（拷不下 ⇒ 「附件存不下」＋［换一张］，不进那一排），进排队。 */
    fun add(
        name: String,
        image: Boolean,
        open: () -> InputStream,
    ) {
        val s = sid() ?: return
        scope.launch {
            val kept =
                try {
                    withContext(io) { store.keep(s, name, image, open) }
                } catch (_: IOException) {
                    local = AttachNote(copyText("mobAttach.note.local"), AttachNote.Act.REPICK)
                    publish()
                    return@launch
                }
            local = null
            publish(cards() + AttachCard(kept.key, kept.name, kept.bytes, kept.image, kept.file, AttachPhase.Queued))
            kick()
        }
    }

    /** ✕：在传 ⇒ 撤那一趟；已落 ⇒ 那台删掉那个编号目录；本机副本删掉。 */
    fun remove(key: String) {
        val s = sid() ?: return
        val card = cards().firstOrNull { it.key == key } ?: return
        if (card.phase is AttachPhase.Moving) pumping?.cancel()
        local = null
        publish(cards().filterNot { it.key == key })
        scope.launch {
            (card.phase as? AttachPhase.Landed)?.let { link.call(SessionAttachmentDrop.COMMAND, SessionAttachmentDrop.args(s, it.id)) }
            withContext(io) { store.drop(s, key) }
        }
        kick()
    }

    /** ［重试］：失败的那几份回排队再走一遍。 */
    fun retry() {
        publish(cards().map { if (it.phase is AttachPhase.Failed) it.copy(phase = AttachPhase.Queued) else it })
        cut = false
        kick()
    }

    /** 发出去那一下：全落好了 ⇒ 编号（按先后）、那一排清空；还有没落好的 ⇒ `null`（不许发）。 */
    fun take(): List<String>? {
        val now = cards()
        if (!_state.value.ready) return null
        if (now.isEmpty()) return emptyList()
        sid()?.let { store.clear(it) }
        publish(emptyList())
        return now.map { (it.phase as AttachPhase.Landed).id }
    }

    /** 那一句带出去的编号：记下（重发照带）· 读回 · 送到了不再记。 */
    fun handed(
        localId: String,
        ids: List<String>,
    ) = store.sent(localId, ids)

    fun handedOf(localId: String): List<String> = store.sentOf(localId)

    fun settled(localId: String) = store.settled(localId)

    private fun cards() = _state.value.cards

    private fun up() = link.state.value is LinkLine.Up && !cut

    /** 有排队的、通着、没在传 ⇒ 起一趟。 */
    private fun kick() {
        if (pumping?.isActive == true) return
        if (!up() || cards().none { it.phase == AttachPhase.Queued }) {
            publish()
            return
        }
        pumping = scope.launch { pump() }
    }

    /** 一份一份传：每一份开传前整排问一次收不收。 */
    private suspend fun pump() {
        while (up()) {
            val s = sid() ?: return
            val asked = cards().filterNot { it.phase is AttachPhase.Refused }
            if (asked.none { it.phase == AttachPhase.Queued }) return
            if (!checked(s, asked)) return
            val next = cards().firstOrNull { it.phase == AttachPhase.Queued } ?: return
            send(s, next)
        }
    }

    /** 整排问一次：不收的那几份标上那台那一句。问不成 ⇒ `false`（断了 ⇒ 回排队；那台的错 ⇒ 排队的都标失败）。 */
    private suspend fun checked(
        s: String,
        asked: List<AttachCard>,
    ): Boolean {
        val r = link.call(SessionAttachmentCheck.COMMAND, SessionAttachmentCheck.args(s, asked.map { AttachAsk(it.name, it.bytes) }))
        val verdicts =
            (r as? Reply.Ok)?.let { SessionAttachmentCheck.of(it.data) }?.takeIf { it.size == asked.size }
                ?: return failQueued(r, SessionAttachmentCheck.COMMAND)
        val no = asked.zip(verdicts).filter { !it.second.ok }.associate { (c, v) -> c.key to (v.said ?: "") }
        if (no.isNotEmpty()) publish(cards().map { c -> no[c.key]?.let { c.copy(phase = AttachPhase.Refused(it)) } ?: c })
        return true
    }

    /** 送一份进暂存区、落成编号。 */
    private suspend fun send(
        s: String,
        card: AttachCard,
    ) {
        set(card.key, AttachPhase.Moving(0L, card.bytes))
        val r =
            try {
                link.transfer(Transfer.Stage(card.name), { fdOf(card.file) }, CHUNK_MS) { done, total ->
                    set(card.key, AttachPhase.Moving(done, if (total < 0) card.bytes else total))
                }
            } catch (_: IOException) {
                return set(card.key, AttachPhase.Failed(copyText("mobFiles.local.readFailed", "name" to card.name)))
            }
        val staged = (r as? Reply.Ok)?.let { FilesCommitUpload.Staged.of(it.data) } ?: return failOne(card, r, STAGE)
        val c = link.call(SessionAttachmentCommit.COMMAND, SessionAttachmentCommit.args(s, card.name, staged), CHUNK_MS)
        val landed = (c as? Reply.Ok)?.let { SessionAttachmentCommit.of(it.data) } ?: return failOne(card, c, SessionAttachmentCommit.COMMAND)
        withContext(io) { store.landed(s, card.key, landed.id) }
        set(card.key, AttachPhase.Landed(landed.id))
    }

    /** 一份没成：断了 ⇒ 回排队等连上；那台的错 ⇒ 那台那一句。 */
    private fun failOne(
        card: AttachCard,
        r: Reply,
        command: String,
    ) {
        if (isCut(r)) {
            cut = true
            return set(card.key, AttachPhase.Queued)
        }
        set(card.key, AttachPhase.Failed(said(r, command)))
    }

    private fun failQueued(
        r: Reply,
        command: String,
    ): Boolean {
        if (isCut(r)) {
            cut = true
            publish()
            return false
        }
        val said = said(r, command)
        publish(cards().map { if (it.phase == AttachPhase.Queued) it.copy(phase = AttachPhase.Failed(said)) else it })
        return false
    }

    private fun said(
        r: Reply,
        command: String,
    ): String =
        when (r) {
            is Reply.Ok -> unreadableReply(machine, command, r.data.toString()).text
            else -> r.problem(machine, command, link.offline.value)?.text.orEmpty()
        }

    private fun isCut(r: Reply) = r is Reply.LinkDown || r is Reply.TimedOut || r is Reply.Cancelled

    private fun set(
        key: String,
        phase: AttachPhase,
    ) = publish(cards().map { if (it.key == key) it.copy(phase = phase) else it })

    /** 摆上 [cards]，错条照此刻重写：手机这边那一句 › 失败的那几份 › 断着。 */
    private fun publish(cards: List<AttachCard> = cards()) {
        val offline = cards.any { it.phase == AttachPhase.Queued } && !up()
        val failed = cards.mapNotNull { (it.phase as? AttachPhase.Failed)?.said }
        val note =
            local
                ?: failed.firstOrNull()?.let { said ->
                    val text = if (failed.size == 1) copyText("mobAttach.note.failed", "said" to said) else copyText("mobAttach.note.failedMany", "n" to failed.size, "said" to said)
                    AttachNote(text, AttachNote.Act.RETRY)
                }
                ?: if (offline) AttachNote(copyText("mobAttach.note.offline", "said" to (link.offline.value ?: machine)), null) else null
        _state.value = AttachUi(cards, note, offline)
    }

    private companion object {
        /** 一块 · 落那一下等多久（同文件页上传）。 */
        const val CHUNK_MS: Long = 20_000L

        /** 分块那一半出错时详情里点名的命令。 */
        const val STAGE = "files-stage-chunk"
    }
}
