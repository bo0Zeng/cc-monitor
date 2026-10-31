package com.ccmonitor.mobile.ui.chat.attach

import com.ccmonitor.mobile.core.claude.link.AttachAsk
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionAttachmentCheck
import com.ccmonitor.mobile.core.claude.link.SessionAttachmentCommit
import com.ccmonitor.mobile.core.claude.link.SessionAttachmentDrop
import com.ccmonitor.mobile.core.claude.link.Transfer
import com.ccmonitor.mobile.core.rs.Core
import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.AttachAct
import com.ccmonitor.mobile.core.wire.gen.AttachCardPhase
import com.ccmonitor.mobile.core.wire.gen.AttachFace
import com.ccmonitor.mobile.core.wire.gen.AttachFaceNote
import com.ccmonitor.mobile.core.wire.gen.AttachLocal
import com.ccmonitor.mobile.core.wire.gen.AttachWant
import com.ccmonitor.mobile.core.wire.gen.AttachWantCard
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.core.wire.gen.Staged
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

    /** 本机那一份副本开不了（进错条，可重试）。 */
    data object Lost : AttachPhase
}

/** 输入框上方的一张卡；[line] · [removeLabel] 是核心写好的字（卡上第二行 · ✕ 的读屏名）。 */
data class AttachCard(
    val key: String,
    val name: String,
    val bytes: Long,
    /** 从拍照 · 图片进来的 ⇒ 画缩略图；从文件进来的 ⇒ 名 ＋ 状态的小卡。 */
    val image: Boolean,
    /** 本机那一份副本（缩略图照它画）。 */
    val file: File,
    val phase: AttachPhase,
    val line: String? = null,
    val removeLabel: String = "",
)

/** 卡下那一条（核心写好）：那一句 ＋ 按钮（没有按钮 ⇒ [act] 为 `null`）；[dashed] ⇒ 断着那一档（虚线灰）。 */
data class AttachNote(
    val text: String,
    val act: Act?,
    val dashed: Boolean = false,
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
 * ➤ 那一下交什么：字与附件都没有 · 那一排还没全落好 ⇒ `null`（不发）；没有那一排 ⇒ 编号为空；
 * 否则那一排的编号（按先后，那一排清空），记在 [localId] 名下（没送到重发照带，[AttachTray.settled] 了才不记）。回 [localId] 与编号。
 */
fun AttachTray?.handOver(
    text: String,
    localId: String,
): Pair<String, List<String>>? {
    val cards =
        this
            ?.state
            ?.value
            ?.cards
            .orEmpty()
    if (text.isBlank() && cards.isEmpty()) return null
    if (this == null) return localId to emptyList()
    return take()?.let { ids -> localId to ids.also { handed(localId, it) } }
}

/** 手机这边的错（那一条最先说它）。 */
enum class LocalWhy {
    /** 拷不进本机（空间满 …）：［换一张］。 */
    NO_ROOM,

    /** 没有能拍照的系统相机。 */
    NO_CAMERA,
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
    /** 那一排的字：交此刻每张卡走到哪，回卡上第二行与卡下那一条（内嵌核心写，[Core.attachFace]）。 */
    private val face: (String) -> String = Core::attachFace,
) {
    private val _state = MutableStateFlow(AttachUi())
    val state: StateFlow<AttachUi> = _state.asStateFlow()

    /** 手机这边的错：下一次加 · 撤就收掉。 */
    private var local: LocalWhy? = null

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
                    local = LocalWhy.NO_ROOM
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

    /** 手机这边的错（没有相机 …）：下一次加 · 撤就收掉。 */
    fun local(why: LocalWhy) {
        local = why
        publish()
    }

    /** ［重试］：失败的那几份回排队再走一遍。 */
    fun retry() {
        publish(cards().map { if (it.phase is AttachPhase.Failed || it.phase == AttachPhase.Lost) it.copy(phase = AttachPhase.Queued) else it })
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
                return set(card.key, AttachPhase.Lost)
            }
        val staged = (r as? Reply.Ok)?.let { Wire.bind<Staged>(it.data) } ?: return failOne(card, r, STAGE)
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

    /** 摆上 [cards]：字交核心照此刻写（卡上第二行 · ✕ 的读屏名 · 卡下那一条）。 */
    private fun publish(cards: List<AttachCard> = cards()) {
        val offline = cards.any { it.phase == AttachPhase.Queued } && !up()
        val want =
            AttachWant(
                cards = cards.map { AttachWantCard(it.name, wireOf(it.phase)) },
                offline = if (offline) link.offline.value ?: machine else null,
                local = local?.let(::wireOf),
            )
        val line = face(Wire.line(want))
        val got = checkNotNull(Wire.read<AttachFace>(line)) { "核心交的附件那一排认不出：${line.take(SNIPPET)}" }
        val shown = cards.zip(got.cards) { c, r -> c.copy(line = r.line, removeLabel = r.remove) }
        _state.value = AttachUi(shown, got.note?.let(::noteOf), offline)
    }

    private fun wireOf(p: AttachPhase): AttachCardPhase =
        when (p) {
            AttachPhase.Queued -> AttachCardPhase.Queued
            is AttachPhase.Moving -> AttachCardPhase.Moving
            is AttachPhase.Landed -> AttachCardPhase.Landed
            is AttachPhase.Refused -> AttachCardPhase.Refused(p.said)
            is AttachPhase.Failed -> AttachCardPhase.Failed(p.said)
            AttachPhase.Lost -> AttachCardPhase.Lost
        }

    private fun wireOf(why: LocalWhy): AttachLocal =
        when (why) {
            LocalWhy.NO_ROOM -> AttachLocal.NO_ROOM
            LocalWhy.NO_CAMERA -> AttachLocal.NO_CAMERA
        }

    private fun noteOf(n: AttachFaceNote): AttachNote {
        val act =
            when (n.act) {
                AttachAct.RETRY -> AttachNote.Act.RETRY
                AttachAct.REPICK -> AttachNote.Act.REPICK
                null -> null
            }
        return AttachNote(n.text, act, n.dashed)
    }

    private companion object {
        /** 认不出时报错里带核心那一行的前这么多字。 */
        const val SNIPPET = 200

        /** 一块 · 落那一下等多久（同文件页上传）。 */
        const val CHUNK_MS: Long = 20_000L

        /** 分块那一半出错时详情里点名的命令。 */
        const val STAGE = "files-stage-chunk"
    }
}
