package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.claude.bridge.PipeSession
import com.ccmonitor.mobile.core.claude.bridge.SendOutcome
import com.ccmonitor.mobile.core.claude.bridge.SendRequest
import com.ccmonitor.mobile.core.claude.bridge.UplinkAppendOutcome
import com.ccmonitor.mobile.core.claude.bridge.UplinkIdentity
import com.ccmonitor.mobile.core.claude.command.PipeCommands
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 「这句话发出去了吗」：「说不清」这一档到得了，但说不出。
 *
 * 这里钉的是一个现状事实，不是一个修好了的行为：
 * 1. 远端是先写、后答（[theRemoteWritesBeforeItAnswersSoALostAnswerLeavesAnIrreduciblyUnknownState]
 *    从生产命令串里量）。所以「答复在回来的路上丢了」永远意味着「可能写进去了，而我们不知道」；
 *    就算做到幂等，这一档也不会消失。不确定就该说不确定，不许显示成「成功」或「失败」。
 * 2. 上行结局与界面侧的像里都没有「说不清」这一档（[theThirdStateIsNotExpressible]）。
 * 3. 「把 `origin` 存下来」不是幂等的修法：`core-claude` 的
 *    `UplinkIdentityTest.twoUnrelatedInstancesNeverMintTheSameIdForTheirFirstMessage` 要求两个互不相识的实例
 *    发出的第一条消息标识必须不同，因为吞消息比重复更坏（重复看得见，吞掉是静默的）。
 *    [stabilisingTheKeyAcrossProcessesWouldReintroduceTheSwallowedFirstMessage] 量的就是这件事。
 *    真要跨进程幂等，得有一份落盘的逐条消息账本。
 *
 * 刻意不写的：「断言 `UplinkAppendOutcome` 有 `Failed` 这一档」恒绿、无判别力；
 * 「断言失败时 `retryable == true`」而不说清它为什么可能是谎（远端可能已经写进去了），
 * 就成了「把现状写成期望」的判据。
 */
class UplinkUnknownStateNegativeControlTest {
    /**
     * 「说不清」这一档在类型上不存在。
     *
     * 两头都断：
     * - 上行那三档（[UplinkAppendOutcome]）恰好是「写了」「本来就在」「失败」，没有第四档；
     * - 折给界面的 [SendOutcome] 只有两个像（接受 / 拒绝）。
     *
     * 所以远端「可能写进去了而我们不知道」这件事，没有任何位置可以把它说出来。
     */
    @Test
    fun theThirdStateIsNotExpressible() {
        // 档数由 [labelOf] 那个穷尽 `when` 钉住：它是表达式，所以编译器要求它覆盖全部分支。
        //   加第四档 ⇒ `labelOf` 编译不过 ⇒ 比任何运行期断言都早红（而且躲不过）。
        //   刻意不用 `sealedSubclasses` 反射：`kotlin-reflect` 不在单测 classpath 上，
        //   那条路会在运行期炸成 `KotlinReflectionNotSupportedError`。
        val all =
            listOf(
                UplinkAppendOutcome.Appended,
                UplinkAppendOutcome.AlreadyThere,
                UplinkAppendOutcome.Failed("随便一个原因"),
            )
        val variants = all.map(::labelOf).toSortedSet()
        assertEquals(
            "上行结局的档数变了。若新增的那一档是「说不清」，" +
                "请连同它的上屏表达一起落地，再来改这条判据（改之前先说清它为什么不再成立）。",
            sortedSetOf("Appended", "AlreadyThere", "Failed"),
            variants,
        )
        // 前提：三档互不相同，否则上面那个集合可能因为折叠而恰好成立
        assertEquals("前提：三档得是三个不同的标签", all.size, variants.size)

        // 另一头：三档折成两档之后，界面侧能收到的像只有两个
        val images =
            all
                .map { it.asOutcome() }
                .map { if (it is SendOutcome.Rejected) "Rejected" else "Accepted" }
                .toSortedSet()
        assertEquals("界面侧只有「接受/拒绝」两个像 ⇒ 第三句话无处可放", sortedSetOf("Accepted", "Rejected"), images)
    }

    /**
     * 档名表：本文件的判别力就在这个穷尽 `when` 上。
     *
     * 它是表达式（不是语句），所以 Kotlin 要求分支覆盖 [UplinkAppendOutcome] 的全部变体：
     * 有人加一档（例如真的加上「说不清」）或删一档，这里当场编译失败。
     */
    private fun labelOf(outcome: UplinkAppendOutcome): String =
        when (outcome) {
            UplinkAppendOutcome.Appended -> "Appended"
            UplinkAppendOutcome.AlreadyThere -> "AlreadyThere"
            is UplinkAppendOutcome.Failed -> "Failed"
        }

    /**
     * 远端是先写、后答，所以「答复丢了」≠「没写」。
     *
     * 量的是 [PipeCommands.idempotentAppendCommand] 真的吐出来的那一串：
     * 追加（`>>`）出现在「已追加」哨兵之前，两者用 `&&` 相连。网络在 `>>` 之后、哨兵回到手机之前断掉时，
     * 远端已经有这一行了，而我们手上只有一个异常。这一档被报成 `UplinkAppendOutcome.Failed(retryable = true)`
     * （见 `PipeSession.PipeUplinkSink.remoteBody` 的 `getOrElse` 那一支），屏上说「没发出去，可以重试」，
     * 而那句话可能是假的。本条不要求把它改好，它要求这件事被说出来、且改动时会被发现。
     */
    @Test
    fun theRemoteWritesBeforeItAnswersSoALostAnswerLeavesAnIrreduciblyUnknownState() {
        val cmd =
            PipeCommands.idempotentAppendCommand(
                sessionId = SID,
                payload = PipeSession.userLineJson("随便一句", "aterm-deadbeef-local#0"),
                needle = PipeSession.uplinkIdNeedle("aterm-deadbeef-local#0"),
            )

        val appendAt = cmd.indexOf(">>")
        val addedMarkerAt = cmd.indexOf(PipeCommands.APPENDED_MARKER)
        // 前提：两样都真的在这条命令里，否则下面那条比较毫无意义
        assertTrue("前提：命令里得有追加重定向：$cmd", appendAt >= 0)
        assertTrue("前提：命令里得有「已追加」哨兵：$cmd", addedMarkerAt >= 0)

        assertTrue(
            "追加必须出现在哨兵之前：这就是「丢了答复 ≠ 没写」的字面证据。" +
                "若有人把它改成先打哨兵再写，那是另一种病（答了却没写），也该在这里红一次：\n$cmd",
            appendAt < addedMarkerAt,
        )
        assertTrue(
            "两者由 `&&` 相连 ⇒ 哨兵是写成功之后才打的；换成 `;` 就会把「写失败」也答成「已追加」：\n$cmd",
            cmd.substring(appendAt, addedMarkerAt).contains("&&"),
        )
    }

    /**
     * 把 `origin` 存下来（最可能被写出来的那个「修法」）会把更坏的 bug 放回来。
     *
     * 「跨进程稳定的键」在实现上就是「两个进程共用同一个 `origin`」。本条直接造那个情形：
     * 用同一个 [UplinkIdentity] 去铸「重启前的第一条」与「重启后的第一条」，
     * 而重启后 `ChatSession` 的计数器从 0 重来，所以两条的 `localId` 都是 `local#0`。
     *
     * 结果：两根 `grep` 针逐字节相同 ⇒ 远端 `grep -qF` 命中 ⇒ 远端答 `ATERM_UP_DUP` ⇒
     * 本地判成 [UplinkAppendOutcome.AlreadyThere] ⇒ `asOutcome()` 把它折成 `Accepted` ⇒
     * 重启后的第一条消息一个字节都没发出去，而屏上显示成功。
     */
    @Test
    fun stabilisingTheKeyAcrossProcessesWouldReintroduceTheSwallowedFirstMessage() {
        val beforeRestart = SendRequest("local#0", "重启前那句")
        val afterRestart = SendRequest("local#0", "重启后那句")
        // 前提①：重启后计数器从 0 重来 ⇒ localId 必然撞上
        assertEquals("前提：两条的 localId 得真的相同", beforeRestart.localId, afterRestart.localId)
        // 前提②：两条正文不同，不然「被当成重复」是对的，本条就没有判别力了
        assertNotEquals("前提：两条得是真的不同的两句话", beforeRestart.text, afterRestart.text)

        // 「跨进程稳定」= 共用一个 origin。这就是那个修法的字面模型。
        val stable = UplinkIdentity()
        val needleBefore = PipeSession.uplinkIdNeedle(stable.idFor(beforeRestart))
        val needleAfter = PipeSession.uplinkIdNeedle(stable.idFor(afterRestart))

        assertEquals(
            "共用 origin 时两根针逐字节相同 ⇒ 远端 `grep -qF` 会把重启后那句判成重复、" +
                "一个字节都不写，而 `AlreadyThere` 折成 `Accepted` ⇒ 屏上显示成功。" +
                "这就是「吞消息」：比重复更坏的那一种。",
            needleBefore,
            needleAfter,
        )

        // 对照：今天的生产形状（各铸各的 origin）下，这两根针必须不同
        val needleBeforeToday = PipeSession.uplinkIdNeedle(UplinkIdentity().idFor(beforeRestart))
        val needleAfterToday = PipeSession.uplinkIdNeedle(UplinkIdentity().idFor(afterRestart))
        assertNotEquals(
            "今天的形状把「吞掉」换成了「可能重复」：那是刻意选的那一侧。" +
                "本条与上一条合起来才是完整的取舍图。",
            needleBeforeToday,
            needleAfterToday,
        )
    }

    private companion object {
        /** 随便一个合法会话编号：本文件不关心它是哪条，只要过得了网关的白名单。 */
        const val SID = "0f9d3c2a-1111-4222-8333-444455556666"
    }
}
