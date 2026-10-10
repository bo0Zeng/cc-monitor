package com.ccmonitor.mobile.core.claude.bridge

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 上行那一行带的标识：跨进程唯一、同一条消息重发不变。
 *
 * 每条断言防的 bug：
 * - [twoUnrelatedInstancesNeverMintTheSameIdForTheirFirstMessage]：直接拿 `localId` 当标识 ⇒
 *   重启后 `local#0` 重来 ⇒ 重启后的第一条被吞掉。
 * - [retryingTheSameRequestMintsTheByteIdenticalId]：标识在 `send()` 里现铸而不是绑在实例上 ⇒ 重试时铸新的、幂等失效。
 * - [theNeedleIsALiteralSubstringOfTheLineItGuards]：针与那一行是两处手拼的字节，对不上 ⇒ 每次都追加。
 *
 * 只断言「`localId` 不为空」或「那一行里有某个键」而不比值，都是恒绿的判据。
 */
class UplinkIdentityTest {
    /**
     * 两个互不相识的实例（模拟 app 重启：各自计数器从 0 起）发出的第一条消息，标识必须不同。
     *
     * 它防的 bug 比重复更坏：直接拿 `localId` 当标识时，重启后的 `local#0` 会被远端 `grep`
     * 到上一次的那一行，于是这条消息一个字节都不写、界面还显示成功，消息被静默吞掉。
     */
    @Test
    fun twoUnrelatedInstancesNeverMintTheSameIdForTheirFirstMessage() {
        val first = SendRequest("local#0", "重启前那句")
        val second = SendRequest("local#0", "重启后那句")

        // 前提：两条请求的 localId 真的相同；不先证明这个，下面那条断言可能因别的原因通过
        assertEquals("前提：重启后计数器从 0 重来，localId 必然撞上", first.localId, second.localId)

        val beforeRestart = UplinkIdentity().idFor(first)
        val afterRestart = UplinkIdentity().idFor(second)
        assertNotEquals("换一个实例（≈换一个进程）标识必须不同", beforeRestart, afterRestart)

        // 一次不同可能是巧合；连造 64 个 origin，一个重复都不许有
        val origins = (1..ORIGIN_SAMPLES).map { UplinkIdentity().origin }
        assertEquals("origin 必须真随机，不许是计数器", ORIGIN_SAMPLES, origins.toSet().size)
    }

    /**
     * 同一条消息重发 N 次，标识逐字节相同。
     *
     * 模型：`ChatSession.retry` 拿同一条 `LocalMessage` 走同一个 `deliver`，
     * 于是 sink 每次收到的是一个新造的、但值相等的 [SendRequest]。
     */
    @Test
    fun retryingTheSameRequestMintsTheByteIdenticalId() {
        val identity = UplinkIdentity()
        val ids = (1..RETRY_TIMES).map { identity.idFor(SendRequest("local#3", "重试我")) }
        assertEquals("重发多少次都是同一串：$ids", 1, ids.toSet().size)

        // 反面：同一个实例、不同 localId ⇒ 必须是两条不同的消息
        assertNotEquals(
            "两条不同的消息不许共用标识",
            identity.idFor(SendRequest("local#3", "x")),
            identity.idFor(SendRequest("local#4", "x")),
        )

        // 标识不吃正文：改了一个字再重试仍是同一条消息
        assertEquals(
            "改了正文仍是同一条消息",
            identity.idFor(SendRequest("local#3", "重试我")),
            identity.idFor(SendRequest("local#3", "重试我，改了一个字")),
        )
    }

    /**
     * 远端 `grep` 的针与那一行 JSON 是两处手拼的字节。
     *
     * 对不上时完全静默：`grep` 永远查不到 ⇒ 每次重试都追加。必须有一条判据把它们焊死。
     */
    @Test
    fun theNeedleIsALiteralSubstringOfTheLineItGuards() {
        val identity = UplinkIdentity()
        val request = SendRequest("local#0", "他说\"你好\"\n还有 \\ 反斜杠，以及 ' 单引号")
        val id = identity.idFor(request)
        val line = PipeSession.userLineJson(request.text, id)
        val needle = PipeSession.uplinkIdNeedle(id)

        assertTrue("针必须是那一行的字面子串：\nneedle=$needle\nline=$line", line.contains(needle))

        // 针要认得出「同一条消息」，也要认不出「另一条消息」
        val other = PipeSession.userLineJson(request.text, identity.idFor(SendRequest("local#1", request.text)))
        assertFalse("针不许命中别的消息：$other", other.contains(needle))
    }

    /**
     * 标识里不许有需要 JSON / shell 转义的字节：有的话，`uplinkIdNeedle` 拼出来的针
     * 与 Moshi 转义后的那一行就对不上，与上一条是同一个坑的另一侧。
     */
    @Test
    fun theIdItselfNeverNeedsEscaping() {
        val id = UplinkIdentity().idFor(SendRequest("local#0", "任意正文"))
        assertTrue("标识形状：$id", id.matches(Regex("""aterm-[0-9a-f]{32}-local#\d+""")))
        assertFalse("标识里不许有引号", id.contains('"') || id.contains('\''))
        assertFalse("标识里不许有反斜杠", id.contains('\\'))
    }

    private companion object {
        const val ORIGIN_SAMPLES = 64
        const val RETRY_TIMES = 5
    }
}
