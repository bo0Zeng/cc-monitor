package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.data.db.AppAction
import com.ccmonitor.mobile.core.data.db.KeyBinding
import com.ccmonitor.mobile.core.data.db.KeyMod
import com.ccmonitor.mobile.core.data.db.KeyPress
import com.ccmonitor.mobile.core.data.db.SpecialKey
import com.ccmonitor.mobile.core.data.db.TermKey
import com.ccmonitor.mobile.ui.session.GestureOut.DOUBLE
import com.ccmonitor.mobile.ui.session.GestureOut.LONG
import com.ccmonitor.mobile.ui.session.GestureOut.LONG_END
import com.ccmonitor.mobile.ui.session.GestureOut.TAP
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** 键条手势的时序（假时钟，毫秒是事件自带的时间）：单击 · 双击 · 长按与连发 · 取消。 */
class KeyGestureTest {
    private fun g(
        double: DoubleTap = DoubleTap.NONE,
        long: LongMode = LongMode.TAP,
    ) = KeyGesture(GestureSpec(double = double, doubleTapMs = 300, longPressMs = 400, long = long, repeatMs = 50))

    private val none = emptyList<GestureOut>()

    @Test
    fun withoutDoubleTapTapFiresOnRelease() {
        val k = g()
        assertEquals(none, k.down(0))
        assertEquals(listOf(TAP), k.up(50))
        assertNull("单击不等", k.deadline())
    }

    @Test
    fun withoutDoubleTapTwoQuickTapsAreTwoTaps() {
        val k = g()
        k.down(0)
        assertEquals(listOf(TAP), k.up(40))
        k.down(100)
        assertEquals(listOf(TAP), k.up(140))
    }

    @Test
    fun withDoubleTapSingleTapWaitsTheWindow() {
        val k = g(DoubleTap.WAIT)
        k.down(0)
        assertEquals(none, k.up(50))
        assertEquals(350L, k.deadline())
        assertEquals(none, k.advance(349))
        assertEquals(listOf(TAP), k.advance(350))
        assertNull(k.deadline())
    }

    @Test
    fun withDoubleTapTwoQuickTapsSendDoubleOnlyOnce() {
        val k = g(DoubleTap.WAIT)
        k.down(0)
        k.up(50)
        assertEquals(listOf(DOUBLE), k.down(200))
        assertEquals(none, k.up(260))
        assertEquals("连按两下不再送两次单击", none, k.advance(5000))
    }

    @Test
    fun withDoubleTapSecondTapTooLateIsAnotherTap() {
        val k = g(DoubleTap.WAIT)
        k.down(0)
        k.up(50)
        assertEquals(listOf(TAP), k.advance(350))
        k.down(400)
        assertEquals(none, k.up(450))
        assertEquals(listOf(TAP), k.advance(750))
    }

    @Test
    fun secondDownPastWindowWithoutTickStillFlushesTapFirst() {
        val k = g(DoubleTap.WAIT)
        k.down(0)
        k.up(50)
        assertEquals("先补上欠着的那下单击", listOf(TAP), k.down(500))
        assertEquals(none, k.up(550))
        assertEquals(listOf(TAP), k.advance(850))
    }

    @Test
    fun eagerDoubleFiresTapAtOnceThenDoubleOnSecondDown() {
        val k = g(DoubleTap.EAGER, LongMode.FIRE)
        k.down(0)
        assertEquals(listOf(TAP), k.up(50))
        assertEquals(listOf(DOUBLE), k.down(200))
        assertEquals(none, k.up(250))
        k.down(900)
        assertEquals("窗外再点是单击", listOf(TAP), k.up(950))
    }

    @Test
    fun longPressBoundFiresOnceAtThresholdAndEndsOnRelease() {
        val k = g(DoubleTap.WAIT, LongMode.FIRE)
        k.down(0)
        assertEquals(400L, k.deadline())
        assertEquals(none, k.advance(399))
        assertEquals(listOf(LONG), k.advance(400))
        assertEquals(none, k.advance(2000))
        assertEquals(listOf(LONG_END), k.up(2100))
        assertEquals("长按过后不等双击", none, k.advance(9000))
    }

    @Test
    fun repeatFiresAtThresholdThenEveryInterval() {
        val k = g(long = LongMode.REPEAT)
        k.down(0)
        assertEquals(listOf(TAP), k.advance(400))
        assertEquals(450L, k.deadline())
        assertEquals(none, k.advance(449))
        assertEquals(listOf(TAP), k.advance(450))
        assertEquals(listOf(TAP), k.advance(500))
        assertEquals(none, k.up(520))
        assertNull(k.deadline())
    }

    @Test
    fun repeatDoesNotBurstAfterALateTick() {
        val k = g(long = LongMode.REPEAT)
        k.down(0)
        k.advance(400)
        assertEquals(listOf(TAP), k.advance(700))
        assertEquals(750L, k.deadline())
    }

    @Test
    fun repeatReleasedBeforeThresholdIsOneTap() {
        val k = g(long = LongMode.REPEAT)
        k.down(0)
        assertEquals(listOf(TAP), k.up(100))
    }

    @Test
    fun longPressUnsetAndNotRepeatableIsOneTapWithoutWaiting() {
        val k = g(DoubleTap.WAIT, LongMode.TAP)
        k.down(0)
        assertEquals(none, k.advance(400))
        assertEquals(listOf(TAP), k.up(900))
        assertNull(k.deadline())
    }

    @Test
    fun cancelDropsThePressAndEndsAHold() {
        val k = g(DoubleTap.WAIT, LongMode.FIRE)
        k.down(0)
        assertEquals(none, k.cancel())
        assertEquals(none, k.advance(5000))
        k.down(6000)
        k.advance(6400)
        assertEquals(listOf(LONG_END), k.cancel())
    }

    private val ctrlC = KeyBinding.Input(listOf(KeyPress.Chord("c", setOf(KeyMod.CTRL))))
    private val up = KeyBinding.Input(listOf(KeyPress.Special(SpecialKey.UP)))
    private val edit = KeyBinding.App(AppAction.EDIT_KEYS)

    private fun key(
        tap: KeyBinding? = null,
        doubleTap: KeyBinding? = null,
        longPress: KeyBinding? = null,
        mod: KeyMod? = null,
        doubleMs: Int? = null,
        longMs: Int? = null,
    ) = TermKey("k", "g", 0, modifier = mod, tap = tap, doubleTap = doubleTap, longPress = longPress, doubleTapMs = doubleMs, longPressMs = longMs)

    @Test
    fun specComesFromTheKeyAndTheTimingsFallBackKeyThenGlobalThenSystem() {
        val sys = SystemTimings(doubleTapMs = 300, longPressMs = 400, repeatMs = 50)
        assertEquals(GestureSpec(DoubleTap.NONE, 300, 400, LongMode.TAP, 50), gestureSpec(key(tap = ctrlC), null, null, sys))
        assertEquals(GestureSpec(DoubleTap.NONE, 300, 400, LongMode.REPEAT, 50), gestureSpec(key(tap = up), null, null, sys))
        assertEquals(GestureSpec(DoubleTap.WAIT, 250, 600, LongMode.FIRE, 50), gestureSpec(key(tap = up, doubleTap = edit, longPress = edit), 250, 600, sys))
        assertEquals(
            GestureSpec(DoubleTap.WAIT, 180, 900, LongMode.TAP, 50),
            gestureSpec(key(tap = ctrlC, doubleTap = edit, doubleMs = 180, longMs = 900), 250, 600, sys),
        )
        assertEquals(
            "修饰键：双击不等、长按按住，两个时长跟总的",
            GestureSpec(DoubleTap.EAGER, 250, 600, LongMode.FIRE, 50),
            gestureSpec(key(mod = KeyMod.CTRL, doubleMs = 180, longMs = 900), 250, 600, sys),
        )
    }

    @Test
    fun outputsMapToTheKeysTriggers() {
        val k = key(tap = ctrlC, doubleTap = edit, longPress = up)
        assertEquals(ctrlC, k.bindingFor(TAP))
        assertEquals(edit, k.bindingFor(DOUBLE))
        assertEquals(up, k.bindingFor(LONG))
        assertNull(k.bindingFor(LONG_END))
        assertEquals("长按没设时手势只出单击", ctrlC, key(tap = ctrlC).bindingFor(TAP))
    }
}
