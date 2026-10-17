package com.ccmonitor.mobile.ui.files

import com.ccmonitor.mobile.core.claude.link.BatchAnswer
import com.ccmonitor.mobile.core.claude.link.BatchEvent
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.FilesLs
import com.ccmonitor.mobile.link.Problem
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import java.io.IOException
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong

/** 本机那个文件夹（系统「选文件夹」给的那一棵）。 */
interface LocalFolder {
    /** 此刻里面有的名字。 */
    fun names(): List<String>

    /** 写一份：[over] ＝ 盖里面叫这个名的那一份（同名答了覆盖）；`null` ⇒ 新建（同名时系统另起名）。开不了 ⇒ [IOException]。 */
    fun create(
        name: String,
        over: String?,
    ): LocalFile
}

/** 本机开好的那一份：[fd] 现开（从头写）· [drop] 没下成时删掉。 */
class LocalFile(
    val fd: () -> Int,
    val drop: () -> Unit,
)

/**
 * 下载一组（逐项拉在这一圈里走：每项现开本机那一份、交核心拉，[FileOps.download]）：本机同名问一次 → 逐项拉 →
 * 断开 ⇒ 停在那里等［继续］（没下完的那几项整份重下）· 收场 ⇒ 结局。每一格的字都由核心写（[CoreLink.downloadEvent]），
 * 这里只记逐项结局；走到哪经 [show] 交给屏（与核心跑的那几组同一张浮卡）。编号是负数，不与核心那几组撞。
 */
class Downloads(
    private val feed: CoreLink,
    private val ops: FileOps,
    private val scope: CoroutineScope,
    private val io: CoroutineDispatcher,
    private val machine: suspend () -> String,
    private val show: (BatchEvent) -> Unit,
) {
    /** 要下的一项：[over] ＝ 盖本机那一份（答了覆盖）· [skip] ＝ 答了跳过。 */
    private data class Pull(
        val entry: FilesLs.Entry,
        val over: String? = null,
        val skip: Boolean = false,
    )

    private class Group(
        val id: Long,
        val folder: LocalFolder,
        val items: List<Pull>,
        val machine: String,
    ) {
        /** 逐项结局（交核心那一形）；没轮到 ⇒ `null`。 */
        val outcomes = MutableList<Map<String, Any?>?>(items.size) { null }

        @Volatile var job: Job? = null

        fun out(
            i: Int,
            state: String,
            p: Problem? = null,
        ) {
            outcomes[i] = mapOf("key" to "$i", "state" to state) + (p?.let { mapOf("reason" to it.text, "detail" to it.detail) } ?: emptyMap())
        }

        /** 从 [from] 起还没结局的那几项：答了跳过的记跳过，别的记撤了（没下完）。 */
        fun rest(from: Int) {
            for (j in from until items.size) if (outcomes[j] == null) out(j, if (items[j].skip) SKIPPED else CANCELLED)
        }
    }

    private val last = AtomicLong()

    /** 同名那一问摆着的那几组（答送哪儿）。 */
    private val asks = ConcurrentHashMap<Long, CompletableDeferred<BatchAnswer?>>()

    /** 做着的 · 收场了还留着（重试 · 继续照它另起）的那几组。 */
    private val groups = LinkedHashMap<Long, Group>()

    private fun group(id: Long): Group? = synchronized(groups) { groups[id] }

    /** 起一组：本机同名先问一次（答了取消 ⇒ 卡收起）。 */
    fun start(
        entries: List<FilesLs.Entry>,
        folder: LocalFolder,
    ) {
        scope.launch(io) {
            val m = machine()
            val id = last.decrementAndGet()
            val here = runCatching { folder.names() }.getOrDefault(emptyList())
            val ask = feed.downloadEvent(want(id, m, entries.map { it.name }, BatchEvent.ASKING, mapOf("here" to here))) ?: return@launch
            var items = entries.map { Pull(it) }
            if (ask.phase == BatchEvent.ASKING) {
                val asked = CompletableDeferred<BatchAnswer?>().also { asks[id] = it }
                show(ask)
                val a = asked.await()
                asks.remove(id)
                val d = feed.decided(ask.conflicts.map { it.key }, a)
                if (d == null) {
                    show(BatchEvent(id, BatchEvent.DROPPED, ask.title))
                    return@launch
                }
                items = decided(items, ask.conflicts, d)
            }
            run(keep(Group(id, folder, items, m)))
        }
    }

    /** 同名那一问的答（`null` ＝ 取消）；不是这里的那一组 ⇒ `false`。 */
    fun answer(
        id: Long,
        a: BatchAnswer?,
    ): Boolean = asks[id]?.complete(a) ?: false

    /** ［停］：手上那一项的本机那一份删掉，没轮到的那几项记撤了；不是这里的那一组 ⇒ `false`。 */
    fun stop(id: Long): Boolean {
        asks[id]?.let { return it.complete(null) }
        val g = group(id) ?: return false
        g.job?.cancel()
        return true
    }

    /** ［重试失败的］·［继续］：没成的 · 没下完的那几项另起一组（答过的同名照原答）；不是这里的那一组 ⇒ `false`。 */
    fun again(id: Long): Boolean {
        val g = group(id) ?: return false
        val left = g.items.filterIndexed { i, _ -> g.outcomes[i]?.get("state") in setOf(FAILED, CANCELLED) }
        if (left.isNotEmpty()) run(keep(Group(last.decrementAndGet(), g.folder, left, g.machine)))
        return true
    }

    private fun keep(g: Group): Group =
        synchronized(groups) {
            groups[g.id] = g
            while (groups.size > KEPT) groups.remove(groups.keys.first())
            g
        }

    private fun run(g: Group) {
        g.job = scope.launch(io) { pull(g) }
    }

    private fun Group.event(
        phase: String,
        more: Map<String, Any?>,
    ) = feed.downloadEvent(want(id, machine, items.map { it.entry.name }, phase, more))?.let(show)

    private suspend fun pull(g: Group) {
        try {
            for ((i, p) in g.items.withIndex()) {
                if (!one(g, i, p)) {
                    g.rest(i)
                    g.event(BatchEvent.CUT, mapOf("outcomes" to g.outcomes))
                    return
                }
            }
        } catch (ce: CancellationException) {
            g.rest(0)
            g.event(BatchEvent.ENDED, mapOf("outcomes" to g.outcomes))
            throw ce
        }
        g.event(BatchEvent.ENDED, mapOf("outcomes" to g.outcomes))
    }

    /** 一项：跳过 · 下成 · 没成 ⇒ 记下、回真；断了 ⇒ 回假。没下成的那一份（撤了也算）删掉。 */
    private suspend fun one(
        g: Group,
        i: Int,
        p: Pull,
    ): Boolean {
        if (p.skip) {
            g.out(i, SKIPPED)
            return true
        }
        val name = p.entry.name
        g.event(BatchEvent.RUNNING, mapOf("done" to i, "part" to 0, "current" to name))
        var file: LocalFile? = null
        val open = {
            g.folder
                .create(name, p.over)
                .also { file = it }
                .fd()
        }
        val r =
            try {
                ops.download(p.entry.path, open, name) { got, total ->
                    if (total > 0) g.event(BatchEvent.RUNNING, mapOf("done" to i, "part" to got.toDouble() / total, "current" to name))
                }
            } catch (ce: CancellationException) {
                file?.drop?.invoke()
                throw ce
            }
        val failed = r.exceptionOrNull()
        if (failed == null) {
            g.out(i, DONE)
            return true
        }
        file?.drop?.invoke()
        val f = failed as? FileOps.Failed
        if (f?.cut == true) return false
        g.out(i, FAILED, f?.problem ?: Problem(failed.message ?: failed.toString(), ""))
        return true
    }

    private fun want(
        id: Long,
        machine: String,
        names: List<String>,
        phase: String,
        more: Map<String, Any?>,
    ): Map<String, Any?> = mapOf("id" to id, "machine" to machine, "names" to names, "phase" to phase) + more

    private companion object {
        const val DONE = "done"
        const val SKIPPED = "skipped"
        const val FAILED = "failed"
        const val CANCELLED = "cancelled"

        /** 收场了还留着几组。 */
        const val KEPT = 16

        /** 核心认过的答（每个键一格）⇒ 每项怎么下：跳过 · 盖本机撞的那一份 · 另起名。 */
        fun decided(
            items: List<Pull>,
            rows: List<BatchEvent.Clash>,
            d: Map<String, String>,
        ): List<Pull> {
            val by = rows.associateBy { it.key }
            return items.mapIndexed { i, p ->
                val c = by["$i"] ?: return@mapIndexed p
                when (d[c.key]) {
                    "skip" -> p.copy(skip = true)
                    "overwrite" -> p.copy(over = c.there)
                    else -> p
                }
            }
        }
    }
}
