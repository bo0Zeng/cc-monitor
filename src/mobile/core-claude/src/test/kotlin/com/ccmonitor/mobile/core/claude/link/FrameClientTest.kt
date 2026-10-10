package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.remote.RemoteDuplex
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.async
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** 按台本走的双向 exec：[push] 一行进 stdout，[written] 收客户端写进 stdin 的每一行。 */
internal class FakeDuplex : RemoteDuplex {
    private val out = Channel<ByteArray>(Channel.UNLIMITED)
    val written = Channel<String>(Channel.UNLIMITED)
    var closed = false

    fun push(line: String) {
        out.trySend((line + "\n").toByteArray())
    }

    fun end() {
        out.close()
    }

    override val stdout: Flow<ByteArray> = out.receiveAsFlow()

    override suspend fun write(bytes: ByteArray) {
        if (closed) throw java.io.IOException("closed")
        String(bytes).lines().filter { it.isNotEmpty() }.forEach { written.trySend(it) }
    }

    override val exitStatus: Int? = null
    override val stderrTail: String = ""

    override fun close() {
        closed = true
        out.close()
    }
}

class FrameClientTest {
    private val hello = """{"kind":"hello","v":1,"build_id":"${EmbeddedBuild.ID}","host_arch":"x86_64","claude_dir":"/h/.claude"}"""
    private val breaks = mutableListOf<String>()

    private suspend fun attached(
        d: FakeDuplex,
        scope: CoroutineScope,
    ): FrameClient {
        d.push(hello)
        d.push("""{"attach":"ok"}""")
        val got = FrameClient.attach(d, listOf("--tail-only"), scope, 1_000, "n1", TZ) { breaks += it }
        return (got as AttachOutcome.Attached).client
    }

    @Test
    fun `先读 hello 再交 attach 行，带这条连接要的旗标`() =
        runTest {
            val d = FakeDuplex()
            val c = attached(d, backgroundScope)
            assertEquals("attach 行旁边带看的这一台的时区（不进 flags）", """{"attach":true,"flags":["--tail-only"],"tz":"$TZ"}""", d.written.receive())
            assertEquals(EmbeddedBuild.ID, c.hello.str("build_id"))
        }

    @Test
    fun `一问一答按 id 对上，失败带核心写好的 message 与 detail`() =
        runTest {
            val d = FakeDuplex()
            val c = attached(d, backgroundScope)
            d.written.receive() // attach 行
            val ok = async { c.call("sessions-needs", null, 5_000) }
            val req1 = Json.obj(d.written.receive())!!
            assertEquals("sessions-needs", req1.str("cmd"))
            assertEquals(5_000L, req1.num("within_ms"))
            assertEquals("请求信封带看的这一台的时区：回包里的「几点」按它写", TZ, req1.str("tz"))
            val bad = async { c.call("history-list", mapOf("limit" to 3), 5_000) }
            val req2 = Json.obj(d.written.receive())!!
            // 倒着答：按 id 对，不按先后
            d.push("""{"kind":"reply","id":"${req2.str("id")}","ok":false,"code":"no_listing","message":"那一句","detail":"码：no_listing"}""")
            d.push("""{"kind":"reply","id":"${req1.str("id")}","ok":true,"data":{"waiting":[]}}""")
            assertEquals(Reply.Ok(mapOf("waiting" to emptyList<Any>())), ok.await())
            assertEquals(Reply.Failed(CoreFailure("no_listing", "那一句", "码：no_listing")), bad.await())
            assertTrue(breaks.isEmpty())
        }

    @Test
    fun `到期限就撤单，迟到的应答不当契约对不上`() =
        runTest {
            val d = FakeDuplex()
            val c = attached(d, backgroundScope)
            d.written.receive()
            val r = async { c.call("quota-read", null, 100) }
            val req = Json.obj(d.written.receive())!!
            val cancel = Json.obj(d.written.receive())!!
            assertEquals(Reply.TimedOut, r.await())
            assertEquals("cancel", cancel.str("cmd"))
            assertEquals(req.str("id"), cancel.obj("args")!!.str("target"))
            d.push("""{"kind":"reply","id":"${req.str("id")}","ok":true}""")
            d.push("""{"kind":"reply","id":"${cancel.str("id")}","ok":true}""")
            // 再问一条，确认分派还活着、上面两行没被报成对不上
            val next = async { c.call("ping", null, 5_000) }
            val req3 = Json.obj(d.written.receive())!!
            d.push("""{"kind":"reply","id":"${req3.str("id")}","ok":true}""")
            assertEquals(Reply.Ok(null), next.await())
            assertTrue(breaks.toString(), breaks.isEmpty())
        }

    @Test
    fun `帧原样进 frames，坏行与对不上的应答报出来`() =
        runTest {
            val d = FakeDuplex()
            val c = attached(d, backgroundScope)
            d.push("""{"kind":"session_status","sid":"s1","activity":"working","activity_text":"运行中","activity_tone":"now"}""")
            d.push("not json")
            d.push("""{"kind":"reply","id":"nobody","ok":true}""")
            d.push("""{"kind":"turn_end","session_id":"s1","uuid":"u"}""")
            assertEquals("session_status", c.frames.receive().str("kind"))
            assertEquals("turn_end", c.frames.receive().str("kind"))
            assertEquals(2, breaks.size)
        }

    @Test
    fun `流断了：未答的一律 LinkDown，之后的调用也是`() =
        runTest {
            val d = FakeDuplex()
            val c = attached(d, backgroundScope)
            d.written.receive()
            val r = async { c.call("history-list", null, 60_000) }
            d.written.receive()
            d.end()
            assertEquals(Reply.LinkDown, r.await())
            withTimeout(1_000) { while (c.isUp) kotlinx.coroutines.yield() }
            assertEquals(Reply.LinkDown, c.call("ping", null, 1_000))
            assertFalse(c.isUp)
        }

    @Test
    fun `小中继连不上：第一行就是拒绝，原因原样`() =
        runTest {
            val d = FakeDuplex()
            d.push("""{"attach":"refused","reason":"absent"}""")
            val got = FrameClient.attach(d, emptyList(), backgroundScope, 1_000, "n", TZ) {}
            assertEquals(AttachOutcome.Refused("absent"), got)
            assertTrue(d.closed)
        }

    @Test
    fun `第一行不是 hello ⇒ 那头不是我们的后端；握手中途断 ⇒ Cut`() =
        runTest {
            val a = FakeDuplex()
            a.push("""{"hello":"world"}""")
            assertTrue(FrameClient.attach(a, emptyList(), backgroundScope, 1_000, "n", TZ) {} is AttachOutcome.NotOurs)
            val b = FakeDuplex()
            b.push(hello)
            b.end()
            val got = FrameClient.attach(b, emptyList(), backgroundScope, 1_000, "n", TZ) {}
            assertEquals(AttachOutcome.Stage.ATTACH_REPLY, (got as AttachOutcome.Cut).stage)
        }

    @Test
    fun `应答帧按金样解：全格与最少格两行`() {
        val replies =
            Fixtures
                .text("session-stream.golden.jsonl")
                .lines()
                .mapNotNull(Json::obj)
                .filter { it.str("kind") == "reply" }
        assertEquals(2, replies.size)
        val decoded = replies.map { FrameClient.replyOf(it, "") }
        // 一行失败（带 message 与 detail）、一行成功：两种都解得出，没有一行落进 Unreadable
        assertTrue(decoded.toString(), decoded.none { it is Reply.Unreadable })
    }
}

private const val TZ = "Asia/Shanghai"
