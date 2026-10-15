package com.ccmonitor.mobile.ui.session.keys

import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import com.ccmonitor.mobile.core.ui.feedback.showFor
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test

/** 删一颗 · 删一组的撤销窗是 8 秒：到点提示自己收起；窗里点撤销回 ActionPerformed。假时钟，不按墙钟。 */
@OptIn(ExperimentalCoroutinesApi::class)
class UndoWindowTest {
    @Test
    fun theUndoWindowClosesAfterEightSeconds() =
        runTest {
            val host = SnackbarHostState()
            var r: SnackbarResult? = null
            launch { r = host.showFor(UNDO_WINDOW_MS, "m", "u") }
            runCurrent()
            advanceTimeBy(UNDO_WINDOW_MS - 1)
            runCurrent()
            assertNotNull("窗没到不许收", host.currentSnackbarData)
            assertNull(r)
            advanceTimeBy(2)
            runCurrent()
            assertNull("到点收起", host.currentSnackbarData)
            assertEquals(SnackbarResult.Dismissed, r)
            assertEquals(8_000L, UNDO_WINDOW_MS)
        }

    @Test
    fun undoInsideTheWindowIsReported() =
        runTest {
            val host = SnackbarHostState()
            var r: SnackbarResult? = null
            launch { r = host.showFor(UNDO_WINDOW_MS, "m", "u") }
            runCurrent()
            advanceTimeBy(7_000)
            runCurrent()
            host.currentSnackbarData!!.performAction()
            runCurrent()
            assertEquals(SnackbarResult.ActionPerformed, r)
        }
}
