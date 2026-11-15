package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.cache.ChatCache
import com.ccmonitor.mobile.cache.Pending
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionWatch
import com.ccmonitor.mobile.core.claude.model.DeliveryState
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.db.PendingOutcome
import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.Changed
import com.ccmonitor.mobile.core.wire.gen.Failed
import com.ccmonitor.mobile.core.wire.gen.RewindChoiceRow
import com.ccmonitor.mobile.core.wire.gen.RewindHow
import com.ccmonitor.mobile.core.wire.gen.RewindOptions
import com.ccmonitor.mobile.core.wire.gen.RewindWhen
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 聊天屏的输入队那一份：只照会话那台推来的名单画（`changed {session_queue, key: 这条}`），不各自记；每条可撤（`session-queue-cancel`）。
 * 名单里每一条的那一句（`said`）是会话那台写好的，这里照摆；不摆哪一端。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ChatQueueTest {
    private fun item(
        id: String,
        said: String,
    ) = mapOf("seq" to 1, "id" to id, "end" to "monitor", "text" to "text of $id", "state" to "queued", "said" to said)

    private fun list(vararg items: Map<String, Any?>) = mapOf("items" to items.toList(), "sent" to emptyList<String>())

    private fun pushed(
        sid: String,
        body: Map<String, Any?>?,
    ): Changed = Wire.bind<Changed>(mapOf("topic" to "session_queue", "key" to sid) + listOfNotNull(body?.let { "body" to it }))!!

    @Test
    fun `★ 桌面撤掉第 1 条 ⇒ 手机那份名单当场少一条、原第 2 条排第 1`() =
        runTest(UnconfinedTestDispatcher()) {
            val link = FakeCoreLink()
            link.answer = { cmd, _ -> if (cmd == "session-queue-read") Reply.Ok(list(item("a", "place-1"), item("b", "place-2"))) else Reply.LinkDown }
            val changed = MutableSharedFlow<Changed>()
            val q = ChatQueue(link, backgroundScope) {}
            q.watch(changed) { "s1" }
            q.read("s1")
            assertEquals(listOf("a" to "place-1", "b" to "place-2"), q.lines().map { it.id to it.said })
            changed.emit(pushed("s1", list(item("b", "place-1"))))
            assertEquals(listOf("b" to "place-1"), q.lines().map { it.id to it.said })
            assertEquals("text of b", q.lines().single().text)
        }

    @Test
    fun `别的会话那一份不动；没带名单 ⇒ 重问`() =
        runTest(UnconfinedTestDispatcher()) {
            val link = FakeCoreLink()
            link.answer = { cmd, _ -> if (cmd == "session-queue-read") Reply.Ok(list(item("a", "place-1"))) else Reply.LinkDown }
            val changed = MutableSharedFlow<Changed>()
            val q = ChatQueue(link, backgroundScope) {}
            q.watch(changed) { "s1" }
            changed.emit(pushed("s-other", list()))
            assertEquals(0, link.calls("session-queue-read").size)
            changed.emit(pushed("s1", null))
            assertEquals(listOf("a"), q.lines().map { it.id })
            assertEquals(mapOf("machine" to "<local>", "sid" to "s1"), link.calls("session-queue-read").single())
        }

    @Test
    fun `★ 撤名单里那一条、那台答没撤成 ⇒ 字不交回、交出撤回包的那一句（框下照它说）`() =
        runTest(UnconfinedTestDispatcher()) {
            val link = FakeCoreLink()
            link.answer = { cmd, _ ->
                when (cmd) {
                    "session-queue-read" -> Reply.Ok(list(item("a", "place-1")))
                    "session-queue-cancel" -> Reply.Ok(mapOf("cancelled" to false, "said" to "S-已送进会话", "queue" to list()))
                    else -> Reply.LinkDown
                }
            }
            val q = ChatQueue(link, backgroundScope) {}
            q.read("s1")
            var back: String? = null
            var said: String? = null
            q.cancel("a", "s1", kept = { said = it }) { back = it }
            assertNull(back)
            assertEquals("S-已送进会话", said)
        }

    @Test
    fun `撤 ⇒ 照编号请那台撤，照回的整份名单画`() =
        runTest(UnconfinedTestDispatcher()) {
            val link = FakeCoreLink()
            link.answer = { cmd, _ ->
                when (cmd) {
                    "session-queue-read" -> Reply.Ok(list(item("a", "place-1"), item("b", "place-2")))
                    "session-queue-cancel" -> Reply.Ok(mapOf("cancelled" to true, "said" to "S-已撤", "queue" to list(item("b", "place-1"))))
                    else -> Reply.LinkDown
                }
            }
            val q = ChatQueue(link, backgroundScope) {}
            q.read("s1")
            var back: String? = null
            q.cancel("a", "s1") { back = it }
            assertEquals(mapOf("machine" to "<local>", "sid" to "s1", "id" to "a"), link.calls("session-queue-cancel").single())
            assertEquals(listOf("b"), q.lines().map { it.id })
            assertEquals("★ 撤成了 ⇒ 那一句的字交回（回到输入框）", "text of a", back)
        }

    @Test
    fun `★ 名单与待发排成同一种行：一条一行只摆那一句与开头那一行，不写哪一端；没送到 · 拒了 ⇒ 错误色；没回执 ⇒ 淡着`() =
        runTest(UnconfinedTestDispatcher()) {
            val link = FakeCoreLink()
            val gone = item("b", "未送达") + ("state" to "undelivered") + ("text" to "第一行\n第二行")
            link.answer = { cmd, _ -> if (cmd == "session-queue-read") Reply.Ok(list(item("a", "等需手动"), gone)) else Reply.LinkDown }
            val q = ChatQueue(link, backgroundScope) {}
            q.read("s1")
            val outbox =
                listOf(
                    OutboxLine("o1", "发着的", "等确认", failed = false, held = false),
                    OutboxLine("o2", "没回执的", "离线", failed = false, held = true),
                    OutboxLine("o3", "被拒的", "队满", failed = true, held = false),
                )
            val rows = queueRows(q.lines(), outbox).rows
            assertEquals(listOf("a", "b", "o1", "o2", "o3"), rows.map { it.id })
            assertEquals(listOf("等需手动", "未送达", "等确认", "离线", "队满"), rows.map { it.state })
            assertEquals("一条只占一行：开头只取第一行", "第一行", rows[1].head)
            assertEquals(listOf(false, true, false, false, true), rows.map { it.error })
            assertEquals(listOf(false, false, false, true, false), rows.map { it.held })
            assertEquals(listOf(true, true, false, false, false), rows.map { it.listed })
        }

    @Test
    fun `★ 待发里那一句名单里已有（回执还没到、那台已排进队）⇒ 只照名单画一行（同桌面 queue-row）`() {
        val listed = listOf(QueueLine("o1", "排着", "那一句", undelivered = false))
        val pending =
            listOf(
                OutboxLine("o1", "那一句", "等确认", failed = false, held = false),
                OutboxLine("o2", "另一句", "等确认", failed = false, held = false),
            )
        val rows = queueRows(listed, pending).rows
        assertEquals(listOf("o1" to true, "o2" to false), rows.map { it.id to it.listed })
        assertEquals("排着", rows.first().state)
    }

    @Test
    fun `★ 那台离线 ⇒ 那一排排头单独一行「名单为上次所知」、不拼离线那句；连着 · 一行都没有 ⇒ 不说（同桌面 queue-row）`() {
        val one = listOf(QueueLine("a", "等确认", "那一句", undelivered = false))
        val stale = queueRows(one, emptyList(), offline = true)
        assertEquals(copyText("terminal.queue.stale"), stale.stale)
        assertEquals(listOf("a"), stale.rows.map { it.id })
        assertNull("连着 ⇒ 照常", queueRows(one, emptyList(), offline = false).stale)
        assertNull("没有一行 ⇒ 那一排收起、不说", queueRows(emptyList(), emptyList(), offline = true).stale)
        val pending = listOf(OutboxLine("o1", "发着的", "等确认", failed = false, held = false))
        assertEquals("只有待发也算有行", stale.stale, queueRows(emptyList(), pending, offline = true).stale)
    }

    /** 只记待发与草稿的缓存（别的照不缓存）：结局连那台拒时那一句照记，重开读回来。 */
    private open class OutboxCache : ChatCache by ChatCache.None {
        val pend = mutableListOf<Pending>()
        val drafts = mutableMapOf<String, String>()

        override suspend fun draft(sid: String) = drafts[sid]

        override suspend fun saveDraft(
            sid: String,
            text: String,
        ) {
            if (text.isEmpty()) drafts.remove(sid) else drafts[sid] = text
        }

        override suspend fun pending(sid: String) = pend.filter { it.sid == sid }

        override suspend fun promote(
            draftSid: String,
            p: Pending,
        ) {
            pend += p
        }

        override suspend fun settle(
            id: String,
            outcome: String,
            attempts: Int,
            refusedSaid: String?,
        ) {
            pend.replaceAll { if (it.id == id) it.copy(outcome = outcome, attempts = attempts, refusedSaid = refusedSaid) else it }
        }

        override suspend fun forget(ids: List<String>) {
            pend.removeAll { it.id in ids }
        }
    }

    /** 待发那一排：字 · 那一句 · 错误色。 */
    private fun ChatSession.box() = state.value.outbox.map { Triple(it.text, it.said, it.failed) }

    @Test
    fun `★ 那台拒了 ⇒ 关掉重开，待发那一行仍是那台拒时说的那一句（错误色），不再交`() =
        runTest {
            val link = FakeCoreLink()
            val file = FakeCoreLink.File()
            link.answer = { cmd, args -> if (cmd == "session-queue-put") Reply.Failed(Failed("queue_full", "s-refused", "")) else file.answer(cmd, args) ?: Reply.LinkDown }
            link.report("s1", "/w")
            val cache = OutboxCache()
            val first = ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope, cache = cache).also { it.open() }
            runCurrent()
            first.send("t-1")
            advanceTimeBy(1_000)
            runCurrent()
            assertEquals(listOf(Triple("t-1", "s-refused", true)), first.box())

            val again = ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope, cache = cache).also { it.open() }
            runCurrent()
            assertEquals(listOf(Triple("t-1", "s-refused", true)), again.box())
            assertEquals("拒了的不再交", 1, link.calls("session-queue-put").size)
        }

    @Test
    fun `★ 状态库迁上来的被拒行没存那台那一句 ⇒ 重开照实写原因未留存（错误色），不再交`() =
        runTest {
            val link = FakeCoreLink()
            val file = FakeCoreLink.File()
            link.answer = { cmd, args -> file.answer(cmd, args) ?: Reply.LinkDown }
            link.report("s1", "/w")
            val cache = OutboxCache().apply { pend += Pending("p1", "s1", "t-1", PendingOutcome.REFUSED, 1, refusedSaid = null) }
            val s = ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope, cache = cache).also { it.open() }
            runCurrent()
            assertEquals(listOf(Triple("t-1", copyText("beQueue.said.noWhy"), true)), s.box())
            assertEquals("被拒的不再交", 0, link.calls("session-queue-put").size)
        }

    /** 替身外面套一层：[held] 那几问挂着，直到判据放行（[let]）；别的照替身答。 */
    private class Gated(
        private val f: FakeCoreLink,
        vararg held: String,
    ) : CoreLink by f,
        SessionWatch by f {
        private val held = held.toSet()
        private val gates = mutableMapOf<String, CompletableDeferred<Reply>>()

        override suspend fun call(
            cmd: String,
            args: Map<String, Any?>?,
            withinMs: Long,
        ): Reply {
            if (cmd !in held) return f.call(cmd, args, withinMs)
            f.calls += cmd to args
            return CompletableDeferred<Reply>().also { gates[cmd] = it }.await()
        }

        fun let(
            cmd: String,
            r: Reply,
        ) = gates.remove(cmd)!!.complete(r)
    }

    /** 已有的那条 s1：那台推来的 `changed` 照接（输入队名单从这里来）。 */
    private fun TestScope.opened(
        link: CoreLink,
        f: FakeCoreLink,
        cache: ChatCache,
        scope: CoroutineScope = backgroundScope,
    ) = ChatSession(link, f, ChatTarget.Existing("s1"), "box", scope, cache = cache, queueMoved = f.changed).also { it.open() }

    /** 那台推来 s1 的整份名单（[body] 缺 ⇒ 没带名单，重问）。 */
    private suspend fun FakeCoreLink.pushQueue(body: Map<String, Any?>?) = changed(mapOf("topic" to "session_queue", "key" to "s1") + listOfNotNull(body?.let { "body" to it }))

    private fun sentOnly(vararg ids: String) = mapOf("items" to emptyList<Any>(), "sent" to ids.toList())

    private fun ChatSession.rows() = queueRows(state.value.queue, state.value.outbox).rows

    @Test
    fun `★ 待发在飞，推来的名单 items 空、sent 含它 ⇒ 那一排没这一行、本地存储删了；那一问后来回没回执也不挂回、不再重问名单`() =
        runTest {
            val f = FakeCoreLink()
            val file = FakeCoreLink.File()
            f.answer = { cmd, args -> file.answer(cmd, args) ?: Reply.LinkDown }
            f.report("s1", "/w")
            val link = Gated(f, "session-queue-put")
            val cache = OutboxCache()
            val s = opened(link, f, cache)
            runCurrent()
            s.send("t-1")
            runCurrent()
            val id =
                s.state.value.outbox
                    .single()
                    .id
            f.pushQueue(sentOnly(id))
            runCurrent()
            assertTrue(
                "名单说已送 ⇒ 离开待发",
                s.state.value.outbox
                    .isEmpty(),
            )
            assertTrue("那一排没这一行、没［撤回］", s.rows().isEmpty())
            assertTrue("本地存的那份删了", cache.pend.isEmpty())
            val reads = f.calls("session-queue-read").size
            link.let("session-queue-put", Reply.LinkDown)
            runCurrent()
            assertTrue(
                "那一问回来没回执 ⇒ 不挂回「无应答」",
                s.state.value.outbox
                    .isEmpty(),
            )
            assertEquals("不再重问名单", reads, f.calls("session-queue-read").size)
        }

    @Test
    fun `★ 交了没回执的那一句，重问回来的名单 sent 含它 ⇒ 那一行没了、本地存储也没了；接上那台不重交`() =
        runTest {
            val f = FakeCoreLink()
            f.state.value = linkDown("断开")
            val file = FakeCoreLink.File()
            f.answer = { cmd, args -> if (cmd == "session-queue-read") Reply.Ok(sentOnly("p1")) else file.answer(cmd, args) ?: Reply.LinkDown }
            f.report("s1", "/w")
            val cache = OutboxCache().apply { pend += Pending("p1", "s1", "t-1", PendingOutcome.PENDING, 1) }
            val s = opened(f, f, cache)
            runCurrent()
            assertEquals(
                listOf("p1"),
                s.state.value.outbox
                    .map { it.id },
            )
            f.pushQueue(null)
            runCurrent()
            assertTrue(
                s.state.value.outbox
                    .isEmpty(),
            )
            assertTrue(cache.pend.isEmpty())
            f.state.value = linkUp()
            runCurrent()
            assertTrue("名单说已送 ⇒ 接上不重交", f.calls("session-queue-put").isEmpty())
        }

    /** 交了没回执的那一句（那一问回来断着）：s1 上那一行淡着挂着。撤那一问照 [f] 原先摆的答。 */
    private fun TestScope.held(
        link: CoreLink,
        f: FakeCoreLink,
        cache: ChatCache = OutboxCache(),
        scope: CoroutineScope = backgroundScope,
    ): ChatSession {
        val file = FakeCoreLink.File()
        val prior = f.answer
        f.answer = { cmd, args -> if (cmd == "session-queue-cancel") prior(cmd, args) else file.answer(cmd, args) ?: Reply.LinkDown }
        f.report("s1", "/w")
        val s = opened(link, f, cache, scope)
        runCurrent()
        s.send("t-1")
        runCurrent()
        assertTrue(
            s.state.value.outbox
                .single()
                .held,
        )
        return s
    }

    /** 断一下再接上（待发里该再交的照编号再交一遍）。 */
    private fun TestScope.bounce(f: FakeCoreLink) {
        f.state.value = linkDown("断开")
        runCurrent()
        f.state.value = linkUp()
        runCurrent()
    }

    @Test
    fun `★ 撤待发：答之前那一行还在、字没回框、再点不另问；那台答没撤成 ⇒ 那一行没了、字不回框、本地存储删了、接上不重交`() =
        runTest {
            val f = FakeCoreLink()
            val link = Gated(f, "session-queue-cancel")
            val cache = OutboxCache()
            val s = held(link, f, cache)
            val id =
                s.state.value.outbox
                    .single()
                    .id
            s.withdraw(id)
            runCurrent()
            assertEquals(
                "答之前那一行还在",
                listOf(id),
                s.state.value.outbox
                    .map { it.id },
            )
            assertNull("答之前字不回框", s.state.value.recall)
            s.withdraw(id)
            runCurrent()
            assertEquals("撤着时再点不另问", 1, f.calls("session-queue-cancel").size)
            bounce(f)
            assertEquals("撤着的接上不重交", 1, f.calls("session-queue-put").size)
            link.let("session-queue-cancel", Reply.Ok(mapOf("cancelled" to false, "said" to "S-已送进会话", "queue" to sentOnly(id))))
            runCurrent()
            assertTrue(
                s.state.value.outbox
                    .isEmpty(),
            )
            assertNull("没撤成 ⇒ 字不回框", s.state.value.recall)
            assertEquals("框下照撤回包的 said 说", "S-已送进会话", s.state.value.note)
            assertTrue(cache.pend.isEmpty())
            bounce(f)
            assertEquals("已送进会话 ⇒ 不重交", 1, f.calls("session-queue-put").size)
        }

    @Test
    fun `★ 撤待发：那台答撤成了 ⇒ 答了才回字`() =
        runTest {
            val f = FakeCoreLink()
            val link = Gated(f, "session-queue-cancel")
            val s = held(link, f)
            val id =
                s.state.value.outbox
                    .single()
                    .id
            s.withdraw(id)
            runCurrent()
            assertNull(s.state.value.recall)
            link.let("session-queue-cancel", Reply.Ok(mapOf("cancelled" to true, "said" to "S-已撤", "queue" to sentOnly())))
            runCurrent()
            assertTrue(
                s.state.value.outbox
                    .isEmpty(),
            )
            assertEquals(
                "t-1",
                s.state.value.recall
                    ?.text,
            )
        }

    @Test
    fun `★ 撤待发：那一问没答复 ⇒ 本地拿掉、字不回框、框下写那一问回来的那一句；接上不重交`() =
        runTest {
            val f = FakeCoreLink()
            f.answer = { _, _ -> Reply.TimedOut("T-撤那一问到期限", "d") }
            val s = held(f, f)
            s.withdraw(
                s.state.value.outbox
                    .single()
                    .id,
            )
            runCurrent()
            assertTrue(
                s.state.value.outbox
                    .isEmpty(),
            )
            assertNull("送没送进去说不清 ⇒ 字不回框", s.state.value.recall)
            assertEquals("T-撤那一问到期限", s.state.value.note)
            bounce(f)
            assertEquals("本地拿掉的不再交", 1, f.calls("session-queue-put").size)
        }

    @Test
    fun `★ 还没交出去（落库那一下还没完）就撤 ⇒ 不问那台、字当场回框、之后也不交、本地不留`() =
        runTest {
            val f = FakeCoreLink()
            val file = FakeCoreLink.File()
            f.answer = { cmd, args -> if (cmd == "session-queue-cancel") Reply.Ok(mapOf("cancelled" to false, "said" to "S-已送进会话", "queue" to sentOnly())) else file.answer(cmd, args) ?: Reply.LinkDown }
            f.report("s1", "/w")
            val kept = CompletableDeferred<Unit>()
            val cache =
                object : OutboxCache() {
                    override suspend fun promote(
                        draftSid: String,
                        p: Pending,
                    ) {
                        kept.await()
                        super.promote(draftSid, p)
                    }
                }
            val s = opened(f, f, cache)
            runCurrent()
            s.send("t-1")
            runCurrent()
            s.withdraw(
                s.state.value.outbox
                    .single()
                    .id,
            )
            runCurrent()
            assertTrue(f.calls("session-queue-cancel").isEmpty())
            assertEquals(
                "t-1",
                s.state.value.recall
                    ?.text,
            )
            kept.complete(Unit)
            runCurrent()
            assertTrue(f.calls("session-queue-put").isEmpty())
            assertTrue("落库晚于撤 ⇒ 落下的那一份也删掉", cache.pend.isEmpty())
        }

    @Test
    fun `★ 新建会话起着时撤第二句 ⇒ 字当场回框，起好以后也不交`() =
        runTest {
            val f = FakeCoreLink()
            val arrival = mapOf("by" to mapOf("kind" to "sid", "sid" to "pre"), "since" to 2, "session" to "proj-cc")
            val prompt = mapOf("result" to "delivered", "why" to null, "said" to null, "note" to "N-ok")
            val opened = mapOf("landed" to "started", "session" to "proj-cc", "line" to "ccm", "sid" to "pre", "agent" to "claude", "cwd" to "/h/proj", "arrival" to arrival, "prompt" to prompt)
            f.answer = { cmd, _ -> if (cmd == "session-open") Reply.Ok(opened) else Reply.LinkDown }
            val s = ChatSession(f, f, ChatTarget.Draft(ticket = "t-1", cwd = "~/proj"), "box", backgroundScope)
            s.send("一")
            s.send("二")
            runCurrent()
            val second =
                s.state.value.outbox
                    .last()
            s.withdraw(second.id)
            runCurrent()
            assertEquals(
                "二",
                s.state.value.recall
                    ?.text,
            )
            f.arrive("pre")
            runCurrent()
            assertEquals("pre", s.state.value.sid)
            assertTrue("撤了的那一句起好以后不交", f.calls("session-queue-put").isEmpty())
        }

    @Test
    fun `★ 那台拒过的待发撤 ⇒ 不问那台、那一行没了、字回框`() =
        runTest {
            val f = FakeCoreLink()
            val file = FakeCoreLink.File()
            f.answer = { cmd, args -> if (cmd == "session-queue-put") Reply.Failed(Failed("queue_full", "s-refused", "")) else file.answer(cmd, args) ?: Reply.LinkDown }
            f.report("s1", "/w")
            val s = opened(f, f, OutboxCache())
            runCurrent()
            s.send("t-1")
            runCurrent()
            s.withdraw(
                s.state.value.outbox
                    .single()
                    .id,
            )
            runCurrent()
            assertTrue(f.calls("session-queue-cancel").isEmpty())
            assertTrue(
                s.state.value.outbox
                    .isEmpty(),
            )
            assertEquals(
                "t-1",
                s.state.value.recall
                    ?.text,
            )
        }

    @Test
    fun `★ 撤名单里那一条，撤回包名单的 sent 含某句待发 ⇒ 那一句离开待发、本地存储也没了`() =
        runTest {
            val f = FakeCoreLink()
            f.state.value = linkDown("断开")
            val file = FakeCoreLink.File()
            f.answer = { cmd, args ->
                when (cmd) {
                    "session-queue-read" -> Reply.Ok(list(item("a", "place-1")))
                    "session-queue-cancel" -> Reply.Ok(mapOf("cancelled" to true, "said" to "S-已撤", "queue" to sentOnly("p1")))
                    else -> file.answer(cmd, args) ?: Reply.LinkDown
                }
            }
            f.report("s1", "/w")
            val cache = OutboxCache().apply { pend += Pending("p1", "s1", "t-1", PendingOutcome.PENDING, 1) }
            val s = opened(f, f, cache)
            runCurrent()
            f.pushQueue(null)
            runCurrent()
            assertEquals(listOf("a", "p1"), s.rows().map { it.id })
            s.cancelQueued("a")
            runCurrent()
            assertTrue(
                s.state.value.outbox
                    .isEmpty(),
            )
            assertTrue(cache.pend.isEmpty())
        }

    /** 一条会话自己的作用域（关屏 ⇒ 收掉它，别的照跑）。 */
    private fun TestScope.screen() = CoroutineScope(backgroundScope.coroutineContext + Job())

    private fun cancelled() = Reply.Ok(mapOf("cancelled" to true, "said" to "S-已撤", "queue" to sentOnly()))

    @Test
    fun `★ 撤着时关屏 ⇒ 下次开屏不照原编号重交，接着问那台撤没撤成；答撤成了 ⇒ 字回框、那一行没了、本地不留`() =
        runTest {
            val f = FakeCoreLink()
            val link = Gated(f, "session-queue-cancel")
            val cache = OutboxCache()
            val s = held(link, f, cache, screen())
            val id =
                s.state.value.outbox
                    .single()
                    .id
            s.withdraw(id)
            runCurrent()
            s.close()
            runCurrent()
            val prior = f.answer
            f.answer = { cmd, args -> if (cmd == "session-queue-cancel") cancelled() else prior(cmd, args) }
            val again = opened(f, f, cache)
            runCurrent()
            assertEquals("撤着的那一句开屏不重交", 1, f.calls("session-queue-put").size)
            assertEquals("接着问那台撤没撤成（同一个编号）", listOf(id, id), f.calls("session-queue-cancel").map { it!!["id"] })
            assertEquals(
                "t-1",
                again.state.value.recall
                    ?.text,
            )
            assertTrue(
                again.state.value.outbox
                    .isEmpty(),
            )
            assertTrue(cache.pend.isEmpty())
        }

    @Test
    fun `★ 点了撤当下就关屏 ⇒ 撤那一问照样交到那台（不随屏的作用域取消）`() =
        runTest {
            val f = FakeCoreLink()
            val link = Gated(f, "session-queue-cancel")
            val s = held(link, f, OutboxCache(), screen())
            s.withdraw(
                s.state.value.outbox
                    .single()
                    .id,
            )
            s.close()
            runCurrent()
            assertEquals(1, f.calls("session-queue-cancel").size)
        }

    @Test
    fun `★ 新建会话的头一句随 session-open 在送 ⇒ 那一行不给［撤回］，点撤也不回字；起好照常离开待发`() =
        runTest {
            val f = FakeCoreLink()
            val link = Gated(f, "session-open")
            val arrival = mapOf("by" to mapOf("kind" to "sid", "sid" to "pre"), "since" to 2, "session" to "proj-cc")
            val prompt = mapOf("result" to "delivered", "why" to null, "said" to null, "note" to "N-ok")
            val opened = mapOf("landed" to "started", "session" to "proj-cc", "line" to "ccm", "sid" to "pre", "agent" to "claude", "cwd" to "/h/proj", "arrival" to arrival, "prompt" to prompt)
            val s = ChatSession(link, f, ChatTarget.Draft(ticket = "t-1", cwd = "~/proj"), "box", backgroundScope)
            s.send("一")
            runCurrent()
            assertEquals(1, f.calls("session-open").size)
            val row = s.rows().single()
            assertFalse("随起交着、撤不回 ⇒ 不摆［撤回］", row.withdrawable)
            s.withdraw(row.id)
            runCurrent()
            assertNull("撤不回 ⇒ 字不回框", s.state.value.recall)
            assertEquals(
                listOf("一"),
                s.state.value.outbox
                    .map { it.text },
            )
            link.let("session-open", Reply.Ok(opened))
            f.arrive("pre")
            runCurrent()
            assertTrue(
                s.state.value.outbox
                    .isEmpty(),
            )
        }

    @Test
    fun `★ 编辑已发的那一句：回执先到、撤成了后到 ⇒ 字不丢（在编辑草稿或输入框里）`() =
        runTest {
            val f = FakeCoreLink()
            val file = FakeCoreLink.File()
            f.answer = { cmd, args -> file.answer(cmd, args) ?: Reply.LinkDown }
            f.report("s1", "/w")
            val link = Gated(f, "session-queue-put", "session-queue-cancel")
            val s = opened(link, f, OutboxCache())
            runCurrent()
            s.edits.send("u2", "改过的", RewindHow.CONVERSATION, RewindWhen.AFTER_TURN)
            runCurrent()
            val id =
                s.state.value.outbox
                    .single()
                    .id
            s.withdraw(id)
            runCurrent()
            link.let("session-queue-put", Reply.Ok(mapOf("outcome" to "queued", "seq" to 1L, "said" to "q-1")))
            runCurrent()
            link.let("session-queue-cancel", cancelled())
            runCurrent()
            val kept = s.state.value.editDrafts["u2"]
            val back =
                s.state.value.recall
                    ?.text
            assertTrue("字在编辑草稿（$kept）或输入框（$back）里", kept == "改过的" || back == "改过的")
        }

    @Test
    fun `★ 撤着时关屏、开屏时那台断着 ⇒ 撤着那一行留着、不删不重交；接上再问，没答复照样留着，答撤成了才回字`() =
        runTest {
            val f = FakeCoreLink()
            val link = Gated(f, "session-queue-cancel")
            val cache = OutboxCache()
            val s = held(link, f, cache, screen())
            val id =
                s.state.value.outbox
                    .single()
                    .id
            s.withdraw(id)
            runCurrent()
            s.close()
            runCurrent()
            f.state.value = linkDown("断开")
            val prior = f.answer
            var answer: Reply = Reply.TimedOut("T-到期限", "d")
            f.answer = { cmd, args -> if (cmd == "session-queue-cancel") answer else prior(cmd, args) }
            val again = opened(f, f, cache)
            runCurrent()
            assertEquals(
                "断着 ⇒ 那一行照旧挂着",
                listOf(id),
                again.state.value.outbox
                    .map { it.id },
            )
            assertEquals("本地那一份留着", 1, cache.pend.size)
            f.state.value = linkUp()
            runCurrent()
            assertEquals("接上再问", 2, f.calls("session-queue-cancel").size)
            assertEquals("没答复 ⇒ 仍撤着、留着", 1, cache.pend.size)
            assertNull(again.state.value.recall)
            answer = cancelled()
            bounce(f)
            assertEquals(
                "t-1",
                again.state.value.recall
                    ?.text,
            )
            assertTrue(cache.pend.isEmpty())
            assertEquals("撤着的从不重交", 1, f.calls("session-queue-put").size)
        }

    @Test
    fun `★ 编辑已发的那一句撤着时关屏 ⇒ 下次开屏不照原编号重交，字留在编辑草稿`() =
        runTest {
            val f = FakeCoreLink()
            val file = FakeCoreLink.File()
            f.answer = { cmd, args -> file.answer(cmd, args) ?: Reply.LinkDown }
            f.report("s1", "/w")
            val link = Gated(f, "session-queue-put", "session-queue-cancel")
            val cache = OutboxCache()
            val s = opened(link, f, cache, screen())
            runCurrent()
            s.edits.send("u2", "改过的", RewindHow.CONVERSATION, RewindWhen.AFTER_TURN)
            runCurrent()
            s.withdraw(
                s.state.value.outbox
                    .single()
                    .id,
            )
            runCurrent()
            s.close()
            runCurrent()
            f.state.value = linkDown("断开")
            val again = opened(f, f, cache)
            runCurrent()
            assertEquals(
                "开屏时那台断着 ⇒ 撤着那一行淡着、照那台成品的 withdrawHeld",
                listOf(FakeCoreLink.WITHDRAW to true),
                again.state.value.outbox
                    .map { it.said to it.held },
            )
            f.state.value = linkUp()
            runCurrent()
            assertEquals("不重交", 1, f.calls("session-queue-put").size)
            assertEquals("改过的", again.state.value.editDrafts["u2"])
            assertTrue("开屏读回仍撤着 ⇒ 抽屉主按钮灰着", "u2" in again.state.value.editHeld)
            again.edits.send("u2", "再改的", RewindHow.CONVERSATION, RewindWhen.AFTER_TURN)
            runCurrent()
            assertEquals("撤着时再确认一次也不再退", 1, f.calls("session-queue-put").size)
        }

    @Test
    fun `编辑草稿：撤着那一格读回来一样；少一格的旧串读不出、不当没在撤读回`() {
        val w = EditDraft("u2", "改过的", EditDraft.Sent("local-1", RewindHow.CONVERSATION, RewindWhen.NOW, withdrawing = true))
        assertEquals(listOf(w), EditDrafts.decode(EditDrafts.encode(listOf(w))))
        val five = listOf("u2", "改过的", "local-1", RewindHow.CONVERSATION.wire, RewindWhen.NOW.wire).joinToString("\u0001")
        assertEquals(emptyList<EditDraft>() to true, EditDrafts.read(five))
        assertEquals("空的不算读不出", emptyList<EditDraft>() to false, EditDrafts.read(null))
        assertEquals(emptyList<EditDraft>() to false, EditDrafts.read(""))
        val good = EditDraft("u3", "好的")
        assertEquals("只丢读不出的那一句", listOf(good) to true, EditDrafts.read(EditDrafts.encode(listOf(good)) + "\u0002" + five))
    }

    @Test
    fun `★ 开屏读到读不出的编辑草稿 ⇒ 照实说一句、丢掉那一格、不再交`() =
        runTest {
            val f = FakeCoreLink()
            val file = FakeCoreLink.File()
            f.answer = { cmd, args -> file.answer(cmd, args) ?: Reply.LinkDown }
            f.report("s1", "/w")
            val cache = OutboxCache()
            val good = EditDraft("u3", "好的")
            val five = listOf("u2", "改过的", "local-1", RewindHow.CONVERSATION.wire, RewindWhen.NOW.wire).joinToString("\u0001")
            cache.drafts[EditDrafts.key("s1")] = EditDrafts.encode(listOf(good)) + "\u0002" + five
            val s = opened(f, f, cache)
            runCurrent()
            assertEquals(copyText("mobSaidEdit.draft.unreadable"), s.state.value.note)
            assertEquals(mapOf("u3" to "好的"), s.state.value.editDrafts)
            assertEquals("读不出的那一句丢掉、读得出的留着", listOf(good), EditDrafts.decode(cache.drafts[EditDrafts.key("s1")]))
            assertTrue("不再交", f.calls("session-queue-put").isEmpty())
        }

    @Test
    fun `★ 编辑那一句撤着 ⇒ 退法抽屉［回退并发送］灰着；撤完不再撤着 ⇒ 照常能点`() =
        runTest {
            val f = FakeCoreLink()
            val file = FakeCoreLink.File()
            f.answer = { cmd, args -> file.answer(cmd, args) ?: Reply.LinkDown }
            f.report("s1", "/w")
            val link = Gated(f, "session-queue-put", "session-queue-cancel")
            val s = opened(link, f, OutboxCache())
            runCurrent()
            s.edits.send("u2", "改过的", RewindHow.CONVERSATION, RewindWhen.AFTER_TURN)
            runCurrent()
            assertFalse("u2" in s.state.value.editHeld)
            s.withdraw(
                s.state.value.outbox
                    .single()
                    .id,
            )
            runCurrent()
            assertEquals(setOf("u2"), s.state.value.editHeld)
            val got = RewindPick.Got(RewindOptions(listOf(RewindChoiceRow(RewindHow.CONVERSATION, "a", true, true)), emptyList()))
            val held = sheetFace(got, "u2" in s.state.value.editHeld, s.state.value.rewindHeld)
            assertFalse("撤着 ⇒ 主按钮灰着", held.confirm)
            assertEquals("抽屉里照那台成品的 rewindHeld 说为什么", FakeCoreLink.REWIND, held.why)
            link.let("session-queue-cancel", cancelled())
            runCurrent()
            assertFalse("撤完 ⇒ 不再撤着", "u2" in s.state.value.editHeld)
            val free = sheetFace(got, "u2" in s.state.value.editHeld, s.state.value.rewindHeld)
            assertTrue(free.confirm)
            assertNull("不撤着 ⇒ 不写那一句", free.why)
        }

    @Test
    fun `★ 点撤那一下那一句已不在待发里 ⇒ 不记成撤着、不问那台；在 ⇒ 记成撤着、问那台`() =
        runTest {
            val f = FakeCoreLink()
            val link = Gated(f, "session-queue-cancel")
            val box = ChatOutbox(link, "box", backgroundScope, OutboxCache(), null) {}
            var asking = 0
            box.withdraw("local-gone", "s1", { asking++ }) {}
            runCurrent()
            assertEquals("不在待发里 ⇒ 不记成撤着", 0, asking)
            assertTrue("不问那台", f.calls("session-queue-cancel").isEmpty())
            val o = Outgoing("local-1", "改过的", DeliveryState.SENDING, null).also { it.asked = true }
            box.lines += o
            box.withdraw("local-1", "s1", { asking++ }) {}
            runCurrent()
            assertEquals("在 ⇒ 记成撤着", 1, asking)
            assertTrue(o.withdrawing)
            assertEquals(1, f.calls("session-queue-cancel").size)
        }

    @Test
    fun `★ 撤着时关屏、开屏时那台断着 ⇒ 撤着那一行淡着、照那台成品的 withdrawHeld，不借 heldSaid 说连上后补交`() =
        runTest {
            val f = FakeCoreLink()
            val link = Gated(f, "session-queue-cancel")
            val cache = OutboxCache()
            val s = held(link, f, cache, screen())
            s.withdraw(
                s.state.value.outbox
                    .single()
                    .id,
            )
            runCurrent()
            f.state.value = linkDown("断开")
            runCurrent()
            assertEquals(
                "头一回问着时断开（没答复会拿掉、不接着问）⇒ 照旧那一句",
                listOf(FakeCoreLink.HELD to true),
                s.state.value.outbox
                    .map { it.said to it.held },
            )
            s.close()
            runCurrent()
            val again = opened(f, f, cache)
            runCurrent()
            assertEquals(
                "撤着、没在问、断着 ⇒ 淡着、照那台成品的 withdrawHeld",
                listOf(FakeCoreLink.WITHDRAW to true),
                again.state.value.outbox
                    .map { it.said to it.held },
            )
            f.state.value = linkUp()
            runCurrent()
            assertTrue(
                "连着 ⇒ 不说离线那一句",
                again.state.value.outbox
                    .all { it.said.isNotEmpty() && it.said != FakeCoreLink.WITHDRAW },
            )
        }

    @Test
    fun `★ 编辑已发的那一句撤着时关屏 ⇒ 下次开屏不再退，接着问那台；答没撤成 ⇒ 编辑草稿删掉、框下照那台那一句说`() =
        runTest {
            val f = FakeCoreLink()
            val file = FakeCoreLink.File()
            f.answer = { cmd, args -> if (cmd == "session-queue-cancel") Reply.Ok(mapOf("cancelled" to false, "said" to "S-已送进会话", "queue" to sentOnly())) else file.answer(cmd, args) ?: Reply.LinkDown }
            f.report("s1", "/w")
            val link = Gated(f, "session-queue-put", "session-queue-cancel")
            val cache = OutboxCache()
            val s = opened(link, f, cache, screen())
            runCurrent()
            s.edits.send("u2", "改过的", RewindHow.CONVERSATION, RewindWhen.AFTER_TURN)
            runCurrent()
            val id =
                s.state.value.outbox
                    .single()
                    .id
            s.withdraw(id)
            runCurrent()
            s.close()
            runCurrent()
            val again = opened(f, f, cache)
            runCurrent()
            assertEquals("不再退", 1, f.calls("session-queue-put").size)
            assertEquals("接着问那台（同一个编号）", listOf(id, id), f.calls("session-queue-cancel").map { it!!["id"] })
            assertNull("没撤成 ⇒ 当送进去了，编辑草稿删掉", again.state.value.editDrafts["u2"])
            assertEquals("S-已送进会话", again.state.value.note)
            assertTrue(
                again.state.value.outbox
                    .isEmpty(),
            )
        }
}
