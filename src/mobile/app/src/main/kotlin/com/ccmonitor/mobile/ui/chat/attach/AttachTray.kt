package com.ccmonitor.mobile.ui.chat.attach

import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.wire.gen.AttachAct
import com.ccmonitor.mobile.core.wire.gen.AttachCardPhase
import com.ccmonitor.mobile.core.wire.gen.AttachDo
import com.ccmonitor.mobile.core.wire.gen.AttachFaceNote
import com.ccmonitor.mobile.core.wire.gen.AttachKept
import com.ccmonitor.mobile.core.wire.gen.AttachLocal
import com.ccmonitor.mobile.core.wire.gen.AttachShown
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.awaitCancellation
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
    /** 全落好了（没有卡也算，核心判的）：➤ 才亮。 */
    val ready: Boolean = true,
)

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
 * 一条会话输入框上方那一排附件：选好一份先拷进本机（[AttachStore]，被杀回来照样在），交给核心（[CoreLink.attach]）——
 * 问收不收 · 分块送进暂存区 · 提交 ⇒ 编号 · 断了回排队、连上接着传 · 那台不收的标上，那一圈与卡上的字都在核心里（桌面同一份）。
 * 这里只留本机副本、把加 · 撤 · 重试 · 连上 · 手机这边的错 · 交出编号转给核心，照核心交的那一排（[CoreLink.attaches]）画。
 */
class AttachTray(
    private val link: CoreLink,
    private val store: AttachStore,
    private val scope: CoroutineScope,
    private val sid: () -> String?,
    /** 现开本机那一份的描述符交给核心（核心用完就关）。 */
    private val fdOf: (File) -> Int,
    private val io: CoroutineContext = Dispatchers.IO,
) {
    private val _state = MutableStateFlow(AttachUi())
    val state: StateFlow<AttachUi> = _state.asStateFlow()

    /** 本机副本（按键，先后照加的先后）：缩略图 · 名字照它画。 */
    private val kept = LinkedHashMap<String, AttachStore.Kept>()

    /** 核心那一排的号（还没开成 ⇒ 0：下一次连上再开）。 */
    private var tray = 0L
    private var opened = false

    /** 开屏：照本机记着的那一排交核心摆回（已落的不重传），盯着连接，通了就转一声。 */
    fun open() {
        if (opened) return
        opened = true
        val s = sid() ?: return
        store.list(s).forEach { kept[it.key] = it }
        scope.launch { link.attaches.collect { if (it.tray == tray && tray != 0L) show(it) } }
        scope.launch { link.state.collect { if (it is LinkLine.Up) up() } }
        scope.launch { link.epoch.drop(1).collect { up() } }
        scope.launch {
            try {
                awaitCancellation()
            } finally {
                if (tray != 0L) link.attach(AttachDo.Close(tray))
            }
        }
        // 排在盯那一排的那一格后面：核心开好当场交的那一格有人收。
        scope.launch { reopen(s) }
    }

    /** 选好一份：拷进本机（拷不下 ⇒ 交核心「存不下」，不进那一排），交核心排队。 */
    fun add(
        name: String,
        image: Boolean,
        open: () -> InputStream,
    ) {
        val s = sid() ?: return
        scope.launch {
            val k =
                try {
                    withContext(io) { store.keep(s, name, image, open) }
                } catch (_: IOException) {
                    return@launch local(LocalWhy.NO_ROOM)
                }
            kept[k.key] = k
            if (tray == 0L) return@launch reopen(s)
            link.attach(AttachDo.Add(tray, keptOf(k)))
        }
    }

    /** ✕：核心撤那一份（在传 ⇒ 撤那一趟；已落 ⇒ 那台删掉那个编号）；本机副本删掉。 */
    fun remove(key: String) {
        val s = sid() ?: return
        kept.remove(key) ?: return
        if (tray != 0L) link.attach(AttachDo.Remove(tray, key))
        scope.launch { withContext(io) { store.drop(s, key) } }
    }

    /** 手机这边的错（存不下 · 没有相机）：交核心写进卡下那一条，下一次加 · 撤核心自己收掉。 */
    fun local(why: LocalWhy) {
        if (tray != 0L) link.attach(AttachDo.Local(tray, wireOf(why)))
    }

    /** ［重试］：失败的那几份回排队再走一遍（核心判哪几份）；没落好的那几份重开描述符一起交（本机副本开不了的那份才救得回来）。 */
    fun retry() {
        if (tray != 0L) link.attach(AttachDo.Retry(tray, kept.values.filter { it.id == null }.map(::keptOf)))
    }

    /** 发出去那一下：全落好了 ⇒ 编号（按先后，核心交的）、那一排清空；还有没落好的 ⇒ `null`（不许发）。 */
    fun take(): List<String>? {
        if (kept.isEmpty()) return emptyList()
        val ids = (if (tray != 0L) link.attach(AttachDo.Take(tray)) else null)?.ids ?: return null
        sid()?.let { store.clear(it) }
        kept.clear()
        _state.value = AttachUi()
        return ids
    }

    /** 那一句带出去的编号：记下（重发照带）· 读回 · 送到了不再记。 */
    fun handed(
        localId: String,
        ids: List<String>,
    ) = store.sent(localId, ids)

    fun handedOf(localId: String): List<String> = store.sentOf(localId)

    fun settled(localId: String) = store.settled(localId)

    /** 连上了：还没开成就开，开着就转一声。 */
    private fun up() {
        if (tray == 0L) sid()?.let(::reopen) else link.attach(AttachDo.Up(tray))
    }

    /** 交核心开这一排：本机记着的照摆（没落好的连描述符一起交）。这台不在 ⇒ 0，下一次连上再开。 */
    private fun reopen(s: String) {
        if (tray != 0L) return
        val cards = kept.values.map(::keptOf)
        tray = link.attach(AttachDo.Open(s, cards))?.tray ?: 0L
    }

    /** 交核心的那一份本机副本：没落好的现开描述符一起交，落好了的只带编号。 */
    private fun keptOf(k: AttachStore.Kept) = AttachKept(k.key, k.name, k.bytes, k.id, if (k.id == null) fdOrNull(k.file) else null)

    /** 现开那一份（开不了 ⇒ 不交，核心传到它时落「本机副本开不了」）。 */
    private fun fdOrNull(f: File): Int? =
        try {
            fdOf(f)
        } catch (_: IOException) {
            null
        }

    /** 照核心交的那一排画；核心交了编号、本机还没记的记下（再开照它摆回）。 */
    private fun show(s: AttachShown) {
        val sidNow = sid()
        val cards =
            s.cards.mapNotNull { c ->
                val k = kept[c.key] ?: return@mapNotNull null
                val id = c.id
                if (id != null && k.id == null && sidNow != null) {
                    kept[c.key] = k.copy(id = id)
                    scope.launch { withContext(io) { store.landed(sidNow, k.key, id) } }
                }
                card(k, phaseOf(c.phase, c.done, k.bytes, id)).copy(line = c.line, removeLabel = c.remove)
            }
        _state.value = AttachUi(cards, s.note?.let(::noteOf), s.offline, s.ready)
    }

    private fun card(
        k: AttachStore.Kept,
        phase: AttachPhase,
    ) = AttachCard(k.key, k.name, k.bytes, k.image, k.file, phase)

    private fun phaseOf(
        p: AttachCardPhase,
        done: Long,
        bytes: Long,
        id: String?,
    ): AttachPhase =
        when (p) {
            AttachCardPhase.Queued -> AttachPhase.Queued
            AttachCardPhase.Moving -> AttachPhase.Moving(done, bytes)
            AttachCardPhase.Landed -> AttachPhase.Landed(id.orEmpty())
            is AttachCardPhase.Refused -> AttachPhase.Refused(p.said)
            is AttachCardPhase.Failed -> AttachPhase.Failed(p.said)
            AttachCardPhase.Lost -> AttachPhase.Lost
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
}
