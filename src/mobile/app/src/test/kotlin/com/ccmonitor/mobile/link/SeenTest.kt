package com.ccmonitor.mobile.link

import com.ccmonitor.mobile.core.claude.link.seen
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.core.wire.gen.MachineStateKind
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import com.ccmonitor.mobile.ui.chat.linkConnecting
import com.ccmonitor.mobile.ui.chat.linkDown
import com.ccmonitor.mobile.ui.chat.machineOf
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

/** `CoreLink.seen`：这台每接上一回（核心交一行新的 Up）交一格；同一个 Up 重交不算；断着不交。 */
class SeenTest {
    private fun up(epoch: Long) = LinkLine.Up(epoch, machineOf(MachineStateKind.UP))

    @Test
    fun `接上一回交一格：同一个 Up 重交不算，断了再接上才再交`() =
        runTest {
            val l = FakeCoreLink()
            l.state.value = null
            var n = 0
            backgroundScope.launch { l.seen.collect { n++ } }
            runCurrent()
            for (s in listOf(linkConnecting(), up(1), up(1), linkDown("x"), linkConnecting(), up(2))) {
                l.state.value = s
                runCurrent()
            }
            assertEquals(2, n)
        }

    @Test
    fun `订的那一刻已通着 ⇒ 当场一格；订的那一刻断着 ⇒ 不交，接上才交`() =
        runTest {
            val on = FakeCoreLink().also { it.state.value = up(3) }
            var a = 0
            backgroundScope.launch { on.seen.collect { a++ } }
            runCurrent()
            assertEquals(1, a)
            val off = FakeCoreLink().also { it.state.value = linkDown("x") }
            var b = 0
            backgroundScope.launch { off.seen.collect { b++ } }
            runCurrent()
            assertEquals(0, b)
            off.state.value = up(1)
            runCurrent()
            assertEquals(1, b)
        }
}
