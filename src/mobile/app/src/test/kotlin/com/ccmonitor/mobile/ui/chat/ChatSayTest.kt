package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.wire.gen.CanSay
import com.ccmonitor.mobile.ui.Faces
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** 输入区照脸上的 `can.say` 画（那台判好、句子写好），手机不另判：脸从整份脸金样取。 */
class ChatSayTest {
    private fun can(face: String) = Faces.of(face).can

    @Test
    fun `说不了 ⇒ 框灰着、占位就是那台那一句；还有终端可去才给切到终端`() {
        val plain = can("live-working-plain")
        assertEquals(SayLook((plain.say as CanSay.No).why, open = false, toTerminal = true), sayLook(down = false, can = plain))
        val gone = can("gone")
        assertEquals(SayLook((gone.say as CanSay.No).why, open = false, toTerminal = false), sayLook(down = false, can = gone))
    }

    @Test
    fun `说得了 ⇒ 占位是那台写的 hint、框照常`() {
        val ended = can("ended")
        assertEquals(SayLook((ended.say as CanSay.Yes).hint, open = true, toTerminal = false), sayLook(down = false, can = ended))
    }

    @Test
    fun `流断着 · 还没收到脸 ⇒ 照常（断着发的成待发）`() {
        assertNull(sayLook(down = true, can = can("live-working-plain")))
        assertNull(sayLook(down = false, can = null))
    }
}
