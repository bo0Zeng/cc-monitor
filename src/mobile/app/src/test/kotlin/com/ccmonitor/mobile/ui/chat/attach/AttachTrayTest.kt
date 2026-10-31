package com.ccmonitor.mobile.ui.chat.attach

import com.ccmonitor.mobile.core.claude.link.CoreFailure
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.Transfer
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.rs.Core
import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.AttachAct
import com.ccmonitor.mobile.core.wire.gen.AttachCardPhase
import com.ccmonitor.mobile.core.wire.gen.AttachFace
import com.ccmonitor.mobile.core.wire.gen.AttachWant
import com.ccmonitor.mobile.core.wire.gen.AttachWantCard
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import com.ccmonitor.mobile.ui.chat.linkDown
import com.ccmonitor.mobile.ui.chat.linkUp
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.IOException
import kotlin.coroutines.EmptyCoroutineContext

/** 那台判「收」的那一格（照生成的那一形：类别 ＋ 类别字必填）。 */
internal val OK: Map<String, Any?> = mapOf("ok" to true, "class" to "image", "class_text" to "图")

/**
 * 输入框上方那一排：选好一份 ⇒ 拷进本机 ⇒ 那台问收不收 ⇒ 分块送进暂存区 ⇒ 落成编号；收不收 · 上限 · 那一句全是那台的。
 * 三类错各一条：手机这边（存不下 ⇒ 换一张）· 那台的错（那台那一句 ＋ 重试）· 离线（排队、连上接着传）。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class AttachTrayTest {
    @get:Rule val dir = TemporaryFolder()

    private val staged = mapOf("key" to "k1", "chunks" to 2.0, "bytes" to 5.0, "sha256" to "ab".repeat(32))

    private fun landed(id: String) = mapOf("id" to id, "name" to "n", "path" to "/w/.ccm-attach/$id/n", "rel" to ".ccm-attach/$id/n", "bytes" to 5.0, "sha256" to "ab", "class" to "image")

    private fun remote(said: String) = Reply.Failed(CoreFailure("dir_full", said, "d", null))

    /** 那台：全收；落一份回一个新编号；撤照收。 */
    private fun FakeCoreLink.happy(verdicts: (Int) -> List<Map<String, Any?>> = { n -> List(n) { OK } }) {
        var n = 0
        answer = { cmd, args ->
            when (cmd) {
                "session-attachment-check" -> Reply.Ok(mapOf("items" to verdicts((args!!["items"] as List<*>).size)))
                "session-attachment-commit" -> Reply.Ok(landed("id-${++n}"))
                "session-attachment-drop" -> Reply.Ok(mapOf("removed" to true))
                else -> Reply.LinkDown
            }
        }
        transferred = { Reply.Ok(staged) }
    }

    private fun TestScope.tray(
        link: FakeCoreLink,
        store: AttachStore = AttachStore(dir.root),
    ) = AttachTray(link, "box", store, backgroundScope, sid = { "s1" }, fdOf = { 7 }, io = EmptyCoroutineContext).also {
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
    fun a_picked_file_is_asked_staged_and_committed_into_an_id() =
        runTest {
            val link = FakeCoreLink().apply { happy() }
            link.moves = listOf(2L to 5L, 5L to 5L)
            val t = tray(link)
            t.put("shot.png")
            runCurrent()
            assertEquals(listOf<AttachPhase>(AttachPhase.Landed("id-1")), t.phases)
            assertEquals(listOf("shot.png"), link.calls("session-attachment-check").flatMap { a -> (a!!["items"] as List<*>).map { (it as Map<*, *>)["name"] } })
            assertEquals(listOf<Pair<Transfer, Int>>(Transfer.Stage("shot.png") to 7), link.transfers)
            val commit = link.calls("session-attachment-commit").single()!!
            assertEquals(listOf("s1", "k1", "shot.png", 2L, 5L), listOf(commit["sid"], commit["key"], commit["name"], commit["chunks"], commit["bytes"]))
            assertTrue(t.state.value.ready)
            assertNull(t.state.value.note)
        }

    @Test
    fun progress_shows_while_the_chunks_move() =
        runTest {
            val link = FakeCoreLink().apply { happy() }
            val seen = mutableListOf<AttachPhase>()
            link.transferred = {
                seen += tray0!!.phases
                Reply.Ok(staged)
            }
            link.moves = listOf(3L to 6L)
            val t = tray(link)
            tray0 = t
            t.put("a.png")
            runCurrent()
            assertEquals(listOf<AttachPhase>(AttachPhase.Moving(3L, 6L)), seen)
        }

    private var tray0: AttachTray? = null

    @Test
    fun what_the_far_side_refuses_before_sending_carries_its_sentence_and_is_never_sent() =
        runTest {
            val link = FakeCoreLink().apply { happy { n -> List(n) { i -> if (i == 1) OK + mapOf("ok" to false, "why" to "too_big", "said" to "超上限 · ≤50 MB") else OK } } }
            val t = tray(link)
            t.put("a.png")
            runCurrent()
            t.put("dataset.zip", image = false)
            runCurrent()
            assertEquals(listOf(AttachPhase.Landed("id-1"), AttachPhase.Refused("超上限 · ≤50 MB")), t.phases)
            assertEquals(1, link.transfers.size)
            assertFalse(t.state.value.ready)
            assertNull("开传前就拒的因写在卡上，不进错条", t.state.value.note)
        }

    @Test
    fun a_far_side_failure_says_its_sentence_counts_alike_and_retries() =
        runTest {
            val link = FakeCoreLink().apply { happy() }
            val ok = link.answer
            link.answer = { cmd, args -> if (cmd == "session-attachment-commit") remote("devbox 磁盘满") else ok(cmd, args) }
            val t = tray(link)
            t.put("a.png")
            t.put("b.png")
            runCurrent()
            assertEquals(List(2) { AttachPhase.Failed("devbox 磁盘满") }, t.phases)
            assertEquals(AttachNote(copyText("mobAttach.note.failedMany", "n" to 2, "said" to "devbox 磁盘满"), AttachNote.Act.RETRY), t.state.value.note)

            link.answer = ok
            t.retry()
            runCurrent()
            assertEquals(listOf(AttachPhase.Landed("id-1"), AttachPhase.Landed("id-2")), t.phases)
            assertNull(t.state.value.note)
        }

    @Test
    fun one_failure_says_it_without_a_count() =
        runTest {
            val link = FakeCoreLink().apply { happy() }
            link.transferred = { remote("devbox 磁盘满") }
            val t = tray(link)
            t.put("a.png")
            runCurrent()
            assertEquals(
                copyText("mobAttach.note.failed", "said" to "devbox 磁盘满"),
                t.state.value.note
                    ?.text,
            )
        }

    @Test
    fun offline_queues_and_picks_up_once_the_link_is_back() =
        runTest {
            val link = FakeCoreLink().apply { happy() }
            link.state.value = linkDown("down")
            val t = tray(link)
            t.put("a.png")
            runCurrent()
            assertEquals(listOf<AttachPhase>(AttachPhase.Queued), t.phases)
            assertTrue(t.state.value.offline)
            assertEquals(AttachNote(copyText("mobAttach.note.offline", "said" to FakeCoreLink.OFFLINE), null, dashed = true), t.state.value.note)
            assertEquals(
                copyText("mobAttach.card.queued"),
                t.state.value.cards
                    .single()
                    .line,
            )
            assertEquals(emptyList<Any>(), link.transfers)

            link.state.value = linkUp()
            runCurrent()
            assertEquals(listOf<AttachPhase>(AttachPhase.Landed("id-1")), t.phases)
            assertFalse(t.state.value.offline)
        }

    @Test
    fun a_cut_mid_transfer_goes_back_to_the_queue_not_to_failed() =
        runTest {
            val link = FakeCoreLink().apply { happy() }
            link.transferred = { Reply.LinkDown }
            val t = tray(link)
            t.put("a.png")
            runCurrent()
            assertEquals(listOf<AttachPhase>(AttachPhase.Queued), t.phases)
            assertTrue(t.state.value.offline)
            assertEquals("断了不当场重试（等连上）", 1, link.transfers.size)

            link.transferred = { Reply.Ok(staged) }
            link.epoch.value = 1L
            runCurrent()
            assertEquals(listOf<AttachPhase>(AttachPhase.Landed("id-1")), t.phases)
        }

    @Test
    fun removing_a_landed_one_drops_it_over_there_and_here() =
        runTest {
            val link = FakeCoreLink().apply { happy() }
            val store = AttachStore(dir.root)
            val t = tray(link, store)
            t.put("a.png")
            runCurrent()
            val card =
                t.state.value.cards
                    .single()
            t.remove(card.key)
            runCurrent()
            assertEquals(listOf(mapOf("sid" to "s1", "id" to "id-1")), link.calls("session-attachment-drop"))
            assertEquals(emptyList<AttachCard>(), t.state.value.cards)
            assertEquals(emptyList<AttachStore.Kept>(), store.list("s1"))
            assertFalse(card.file.exists())
        }

    @Test
    fun take_hands_the_ids_in_order_only_when_all_have_landed() =
        runTest {
            val link = FakeCoreLink().apply { happy() }
            link.state.value = linkDown("down")
            val store = AttachStore(dir.root)
            val t = tray(link, store)
            t.put("a.png")
            t.put("b.png")
            runCurrent()
            assertNull("还在排队 ⇒ 不许发", t.take())
            link.state.value = linkUp()
            runCurrent()
            assertEquals(listOf("id-1", "id-2"), t.take())
            assertEquals(emptyList<AttachCard>(), t.state.value.cards)
            assertEquals(emptyList<AttachStore.Kept>(), store.list("s1"))
            assertEquals(emptyList<String>(), t.take())
        }

    @Test
    fun killed_and_back_landed_ones_stay_and_the_rest_send_again() =
        runTest {
            val link = FakeCoreLink().apply { happy() }
            val store = AttachStore(dir.root)
            val first = store.keep("s1", "a.png", image = true) { ByteArray(5).inputStream() }
            store.landed("s1", first.key, "old-1")
            store.keep("s1", "b.png", image = true) { ByteArray(5).inputStream() }
            val t = tray(link, store)
            runCurrent()
            assertEquals(listOf(AttachPhase.Landed("old-1"), AttachPhase.Landed("id-1")), t.phases)
            assertEquals(1, link.transfers.size)
            assertEquals(listOf("old-1", "id-1"), store.list("s1").map { it.id })
        }

    @Test
    fun a_copy_that_cannot_be_kept_says_so_with_a_repick_and_adds_no_card() =
        runTest {
            val link = FakeCoreLink().apply { happy() }
            val t = tray(link)
            t.add("big.bin", image = false) { throw IOException("ENOSPC") }
            runCurrent()
            assertEquals(emptyList<AttachCard>(), t.state.value.cards)
            assertEquals(AttachNote(copyText("mobAttach.note.local"), AttachNote.Act.REPICK), t.state.value.note)
            t.put("a.png")
            runCurrent()
            assertNull("下一份进来那一条就收", t.state.value.note)
        }

    /** 进出核心那一排的字是两端同一份类型：交生成的 [AttachWant]，回的照生成的 [AttachFace] 解得开（认不得的一步 ⇒ 核心回空串）。 */
    @Test
    fun the_tray_s_words_cross_the_core_as_generated_types() {
        val want =
            AttachWant(
                cards = listOf(AttachWantCard("a.png", AttachCardPhase.Refused("超上限")), AttachWantCard("b.png", AttachCardPhase.Lost)),
                offline = null,
                local = null,
            )
        val got = Wire.read<AttachFace>(Core.attachFace(Wire.line(want)))
        assertEquals(listOf("超上限", copyText("mobAttach.card.failed")), got?.cards?.map { it.line })
        assertEquals(AttachAct.RETRY, got?.note?.act)
        assertEquals("", Core.attachFace("""{"cards":[{"name":"a.png","phase":{"kind":"nope"}}],"offline":null,"local":null}"""))
    }
}
