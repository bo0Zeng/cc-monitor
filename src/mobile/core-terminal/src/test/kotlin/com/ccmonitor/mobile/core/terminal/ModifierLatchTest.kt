package com.ccmonitor.mobile.core.terminal

import org.junit.Assert.assertEquals
import org.junit.Test

/** 修饰键三态：单击点亮一次 · 双击锁住 · 长按按住期间一直带；软键盘与键条取的是同一份。 */
class ModifierLatchTest {
    private val none = KeyModifiers()
    private val ctrl = KeyModifiers(ctrl = true)

    @Test
    fun tapLightsForOneKeyThenGoesOut() {
        val l = ModifierLatch()
        l.on(Mod.CTRL, ModGesture.TAP)
        assertEquals(ModState.ONCE, l.states.value.of(Mod.CTRL))
        assertEquals(ctrl, l.take())
        assertEquals(ModState.OFF, l.states.value.of(Mod.CTRL))
        assertEquals("带过一次就熄", none, l.take())
    }

    @Test
    fun tapAgainBeforeUseTurnsOff() {
        val l = ModifierLatch()
        l.on(Mod.ALT, ModGesture.TAP)
        l.on(Mod.ALT, ModGesture.TAP)
        assertEquals(ModState.OFF, l.states.value.of(Mod.ALT))
        assertEquals(none, l.take())
    }

    @Test
    fun lockStaysUntilTappedAgain() {
        val l = ModifierLatch()
        l.on(Mod.CTRL, ModGesture.TAP)
        l.on(Mod.CTRL, ModGesture.LOCK)
        assertEquals(ModState.LOCKED, l.states.value.of(Mod.CTRL))
        repeat(3) { assertEquals(ctrl, l.take()) }
        l.on(Mod.CTRL, ModGesture.TAP)
        assertEquals(ModState.OFF, l.states.value.of(Mod.CTRL))
        assertEquals(none, l.take())
    }

    @Test
    fun holdAppliesWhilePressedOnly() {
        val l = ModifierLatch()
        l.on(Mod.SHIFT, ModGesture.HOLD)
        assertEquals(ModState.HELD, l.states.value.of(Mod.SHIFT))
        repeat(2) { assertEquals(KeyModifiers(shift = true), l.take()) }
        l.on(Mod.SHIFT, ModGesture.RELEASE)
        assertEquals(ModState.OFF, l.states.value.of(Mod.SHIFT))
        assertEquals(none, l.take())
    }

    @Test
    fun releaseGoesBackToWhatItWasBefore() {
        val l = ModifierLatch()
        l.on(Mod.CTRL, ModGesture.TAP)
        l.on(Mod.CTRL, ModGesture.LOCK)
        l.on(Mod.CTRL, ModGesture.HOLD)
        l.on(Mod.CTRL, ModGesture.RELEASE)
        assertEquals("锁着时按住再松，仍锁着", ModState.LOCKED, l.states.value.of(Mod.CTRL))
    }

    @Test
    fun modifiersCombineAndOnlyOnceOnesClear() {
        val l = ModifierLatch()
        l.on(Mod.CTRL, ModGesture.TAP)
        l.on(Mod.CTRL, ModGesture.LOCK)
        l.on(Mod.SHIFT, ModGesture.TAP)
        assertEquals(KeyModifiers(ctrl = true, shift = true), l.take())
        assertEquals(ctrl, l.take())
    }

    @Test
    fun softKeyboardShiftUppercasesLetter() {
        val l = ModifierLatch()
        l.on(Mod.SHIFT, ModGesture.TAP)
        assertEquals("A", String(applyKeyModifiers("a".toByteArray(), l.take())))
    }
}
