package com.ccmonitor.mobile.core.ui.feedback

import androidx.compose.material3.SnackbarHostState
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test

/** 撤得回的动作给的撤销窗全 App 一个数（8 秒）：到点提示自己收起、不再撤；窗里点撤销照做。假时钟，不按墙钟。 */
@OptIn(ExperimentalCoroutinesApi::class)
class UndoWindowTest {
    private fun TestScope.controller(host: SnackbarHostState) = AppSnackbarController(host, backgroundScope) {}

    @Test
    fun theUndoWindowIsEightSecondsAndThenCloses() =
        runTest {
            val host = SnackbarHostState()
            var undone = 0
            controller(host).showUndo("m", "u") { undone++ }
            runCurrent()
            advanceTimeBy(UNDO_WINDOW_MS - 1)
            runCurrent()
            assertNotNull("窗没到不许收", host.currentSnackbarData)
            advanceTimeBy(2)
            runCurrent()
            assertNull("到点收起", host.currentSnackbarData)
            assertEquals(0, undone)
            assertEquals(8_000L, UNDO_WINDOW_MS)
        }

    @Test
    fun undoInsideTheWindowRuns() =
        runTest {
            val host = SnackbarHostState()
            var undone = 0
            controller(host).showUndo("m", "u") { undone++ }
            runCurrent()
            advanceTimeBy(7_000)
            runCurrent()
            host.currentSnackbarData!!.performAction()
            runCurrent()
            assertEquals(1, undone)
        }
}
