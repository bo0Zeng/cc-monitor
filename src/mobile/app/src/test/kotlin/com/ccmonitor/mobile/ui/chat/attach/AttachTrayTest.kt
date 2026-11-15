package com.ccmonitor.mobile.ui.chat.attach

import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.wire.gen.AttachAct
import com.ccmonitor.mobile.core.wire.gen.AttachCardPhase
import com.ccmonitor.mobile.core.wire.gen.AttachDo
import com.ccmonitor.mobile.core.wire.gen.AttachDone
import com.ccmonitor.mobile.core.wire.gen.AttachFaceNote
import com.ccmonitor.mobile.core.wire.gen.AttachKept
import com.ccmonitor.mobile.core.wire.gen.AttachLocal
import com.ccmonitor.mobile.core.wire.gen.AttachShown
import com.ccmonitor.mobile.core.wire.gen.AttachShownCard
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import com.ccmonitor.mobile.ui.chat.linkDown
import com.ccmonitor.mobile.ui.chat.linkUp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.IOException
import kotlin.coroutines.EmptyCoroutineContext

/**
 * 核心那一排的替身：那一圈在核心里（`chunk_core::attach`，判据在 Rust 那边），这里只照几条最简单的规矩摆 ——
 * 通着 ⇒ 加进来的当场落好（编号 `id-<n>`）；断着 ⇒ 排队、等 `up`；`take` 全落好才交编号。每一下记进 [ops]。
 */
internal class CoreTray(
    val base: FakeCoreLink = FakeCoreLink(),
) : CoreLink by base {
    val ops = mutableListOf<AttachDo>()
    private val flow = MutableSharedFlow<AttachShown>(extraBufferCapacity = 64)
    override val attaches: SharedFlow<AttachShown> = flow

    /** 那一排此刻：键 ⇒（走到哪 · 编号）。 */
    private val cards = LinkedHashMap<String, Pair<AttachCardPhase, String?>>()
    private var n = 0
    var note: AttachFaceNote? = null

    private fun up() = base.state.value is com.ccmonitor.mobile.core.wire.gen.LinkLine.Up

    private fun landQueued() {
        if (!up()) return
        for ((k, v) in cards) if (v.first == AttachCardPhase.Queued) cards[k] = AttachCardPhase.Landed to "id-${++n}"
    }

    /** 核心交一格（照此刻）。 */
    fun emit() {
        val offline = !up() && cards.values.any { it.first == AttachCardPhase.Queued }
        flow.tryEmit(
            AttachShown(
                TRAY,
                cards.map { (k, v) -> AttachShownCard(k, v.first, 0L, v.second, null, "撤 $k") },
                note,
                offline,
                cards.values.all { it.first == AttachCardPhase.Landed },
            ),
        )
    }

    /** 测试直接摆一张卡此刻在哪（替核心）。 */
    fun set(
        key: String,
        phase: AttachCardPhase,
    ) {
        cards[key] = phase to cards[key]?.second
        emit()
    }

    override fun attach(op: AttachDo): AttachDone? {
        ops += op
        when (op) {
            is AttachDo.Open -> op.cards.forEach { c -> cards[c.key] = (if (c.id != null) AttachCardPhase.Landed else AttachCardPhase.Queued) to c.id }
            is AttachDo.Add -> cards[op.card.key] = AttachCardPhase.Queued to null
            is AttachDo.Remove -> cards.remove(op.key)
            is AttachDo.Retry -> cards.replaceAll { _, v -> if (v.first is AttachCardPhase.Failed) AttachCardPhase.Queued to v.second else v }
            is AttachDo.Take -> {
                if (!cards.values.all { it.first == AttachCardPhase.Landed }) return AttachDone(TRAY, null)
                val ids = cards.values.mapNotNull { it.second }
                cards.clear()
                return AttachDone(TRAY, ids)
            }
            else -> {}
        }
        if (op !is AttachDo.Close && op !is AttachDo.Local) landQueued()
        emit()
        return AttachDone(TRAY, null)
    }

    companion object {
        const val TRAY = 1L
    }
}

/**
 * 输入框上方那一排在 Kotlin 这一侧：选好一份拷进本机，加 · 撤 · 重试 · 连上 · 手机这边的错 · 交出编号各转成对核心的那一下，
 * 照核心交的那一排画；核心交过编号的记进本机，再开照它摆回。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class AttachTrayTest {
    @get:Rule val dir = TemporaryFolder()

    private fun TestScope.tray(
        core: CoreTray,
        store: AttachStore = AttachStore(dir.root),
    ) = AttachTray(core, store, backgroundScope, sid = { "s1" }, fdOf = { 7 }, io = EmptyCoroutineContext).also {
        it.open()
        runCurrent()
    }

    private fun AttachTray.put(
        name: String,
        n: Int = 5,
        image: Boolean = true,
    ) = add(name, image) { ByteArray(n).inputStream() }

    private val AttachTray.phases get() = state.value.cards.map { it.phase }

    @Test
    fun a_picked_file_is_kept_here_and_handed_to_the_core_with_its_descriptor() =
        runTest {
            val core = CoreTray()
            val store = AttachStore(dir.root)
            val t = tray(core, store)
            t.put("shot.png")
            runCurrent()
            val add = core.ops.filterIsInstance<AttachDo.Add>().single()
            assertEquals(AttachKept(add.card.key, "shot.png", 5L, null, 7), add.card)
            assertEquals(listOf<AttachPhase>(AttachPhase.Landed("id-1")), t.phases)
            assertTrue(t.state.value.ready)
            assertEquals("核心交过的编号记进本机（再开照它摆回）", listOf("id-1"), store.list("s1").map { it.id })
        }

    @Test
    fun the_row_is_drawn_from_what_the_core_hands() =
        runTest {
            val core = CoreTray()
            val t = tray(core)
            core.base.state.value = linkDown("down")
            t.put("a.png")
            runCurrent()
            val key =
                t.state.value.cards
                    .single()
                    .key
            core.note = AttachFaceNote("1 份没传上 · 磁盘满", AttachAct.RETRY, false)
            core.set(key, AttachCardPhase.Failed("磁盘满"))
            runCurrent()
            val ui = t.state.value
            assertEquals(listOf<AttachPhase>(AttachPhase.Failed("磁盘满")), t.phases)
            assertEquals("撤 $key", ui.cards.single().removeLabel)
            assertEquals(AttachNote("1 份没传上 · 磁盘满", AttachNote.Act.RETRY), ui.note)
            assertEquals(false, ui.ready)
            core.set(key, AttachCardPhase.Queued)
            runCurrent()
            assertTrue("断着有排队的 ⇒ 虚线", t.state.value.offline)
        }

    @Test
    fun a_link_coming_up_or_a_new_stream_only_says_up() =
        runTest {
            val core = CoreTray()
            core.base.state.value = linkDown("down")
            val t = tray(core)
            t.put("a.png")
            runCurrent()
            assertEquals(listOf<AttachPhase>(AttachPhase.Queued), t.phases)
            core.base.state.value = linkUp()
            runCurrent()
            core.base.epoch.value = 2L
            runCurrent()
            assertEquals(2, core.ops.count { it is AttachDo.Up })
            assertEquals(listOf<AttachPhase>(AttachPhase.Landed("id-1")), t.phases)
        }

    @Test
    fun remove_retry_and_local_are_handed_over_as_they_are() =
        runTest {
            val core = CoreTray()
            val store = AttachStore(dir.root)
            val t = tray(core, store)
            t.put("a.png")
            runCurrent()
            val key =
                t.state.value.cards
                    .single()
                    .key
            t.retry()
            t.local(LocalWhy.NO_CAMERA)
            t.remove(key)
            runCurrent()
            assertEquals(
                listOf(AttachDo.Retry(CoreTray.TRAY, emptyList()), AttachDo.Local(CoreTray.TRAY, AttachLocal.NO_CAMERA), AttachDo.Remove(CoreTray.TRAY, key)),
                core.ops.drop(2),
            )
            assertEquals(emptyList<AttachCard>(), t.state.value.cards)
            assertEquals("本机副本删掉", emptyList<AttachStore.Kept>(), store.list("s1"))
        }

    @Test
    fun take_hands_what_the_core_answers_and_clears_the_row() =
        runTest {
            val core = CoreTray()
            val store = AttachStore(dir.root)
            core.base.state.value = linkDown("down")
            val t = tray(core, store)
            t.put("a.png")
            t.put("b.png")
            runCurrent()
            assertNull("还有没落好的 ⇒ 不交", t.take())
            core.base.state.value = linkUp()
            runCurrent()
            assertEquals(listOf("id-1", "id-2"), t.take())
            assertEquals(emptyList<AttachCard>(), t.state.value.cards)
            assertEquals(emptyList<AttachStore.Kept>(), store.list("s1"))
            assertEquals("没有那一排 ⇒ 空编号", emptyList<String>(), t.take())
        }

    @Test
    fun a_reopened_row_hands_the_kept_copies_landed_ones_without_a_descriptor() =
        runTest {
            val store = AttachStore(dir.root)
            val old = store.keep("s1", "old.png", true) { ByteArray(3).inputStream() }
            store.landed("s1", old.key, "old-1")
            val fresh = store.keep("s1", "new.png", true) { ByteArray(3).inputStream() }
            val core = CoreTray()
            val t = tray(core, store)
            val open = core.ops.first() as AttachDo.Open
            assertEquals("s1", open.sid)
            assertEquals(listOf(AttachKept(old.key, "old.png", 3L, "old-1", null), AttachKept(fresh.key, "new.png", 3L, null, 7)), open.cards)
            assertEquals(listOf<AttachPhase>(AttachPhase.Landed("old-1"), AttachPhase.Landed("id-1")), t.phases)
        }

    @Test
    fun retry_reopens_the_unlanded_copies_and_closing_the_scope_closes_the_row() =
        runTest {
            val core = CoreTray()
            core.base.state.value = linkDown("down")
            val child = CoroutineScope(backgroundScope.coroutineContext + Job())
            val t = AttachTray(core, AttachStore(dir.root), child, sid = { "s1" }, fdOf = { 9 }, io = EmptyCoroutineContext)
            t.open()
            runCurrent()
            t.put("a.png")
            runCurrent()
            val key =
                t.state.value.cards
                    .single()
                    .key
            t.retry()
            assertEquals(AttachDo.Retry(CoreTray.TRAY, listOf(AttachKept(key, "a.png", 5L, null, 9))), core.ops.last())
            child.cancel()
            runCurrent()
            assertEquals(AttachDo.Close(CoreTray.TRAY), core.ops.last())
        }

    @Test
    fun a_copy_that_cannot_be_kept_tells_the_core_and_adds_no_card() =
        runTest {
            val core = CoreTray()
            val t = tray(core)
            t.add("big.bin", image = false) { throw IOException("ENOSPC") }
            runCurrent()
            assertEquals(emptyList<AttachCard>(), t.state.value.cards)
            assertEquals(AttachDo.Local(CoreTray.TRAY, AttachLocal.NO_ROOM), core.ops.last())
        }
}
