package com.ccmonitor.mobile.ui.chat

import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.StateRestorationTester
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onChildren
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.ccmonitor.mobile.core.claude.bridge.ChatTurnAssembler
import com.ccmonitor.mobile.core.claude.bridge.Pacing
import com.ccmonitor.mobile.core.claude.bridge.ReplayRow
import com.ccmonitor.mobile.core.claude.bridge.ReplayTransport
import com.ccmonitor.mobile.core.claude.bridge.ReplayVectors
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.ui.theme.AppTheme
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * 「vectors → 像素上屏」的机器验证。
 *
 * 约定：`junit4.v2`、断言前 `waitUntil`、需要权限时 `GrantPermissionRule`。
 * 本测不起 `MainActivity`（用 [createComposeRule] 而非 `createAndroidComposeRule`），
 * 所以不需要通知权限，那条约定只在拉起真 Activity 时才适用。
 *
 * 用 [Pacing.None]：`long-reply` 照原节奏要 ~15 秒（31 个块、间隔中位 472ms），androidTest 里 `delay` 是真睡，
 * 慢测试的下场是被人加 `@Ignore`。观感靠 `ChatReplayActivity` 用真机肉眼验，正确性靠这里快速验。
 */
@RunWith(AndroidJUnit4::class)
class ChatReplayUiTest {
    /** 测试用的最小状态构造（住测试源集，不进 release 包）。 */
    private fun chatStateOf(vararg units: RenderUnit) = ChatUiState(units = units.toList())

    @get:Rule
    val compose = createComposeRule()

    private fun rows(case: String): List<ReplayRow> =
        InstrumentationRegistry
            .getInstrumentation()
            .targetContext
            .assets
            .open("$case.frames.ndjson")
            .bufferedReader()
            .useLines { ReplayVectors.parse(it) }

    private fun unitsOf(case: String): List<RenderUnit> {
        val a = ChatTurnAssembler()
        runBlocking { ReplayTransport(rows(case), Pacing.None).frames().toList() }.forEach { a.feed(it) }
        return a.units()
    }

    /**
     * 真录制的 vector 渲染成卡片并显示在屏幕上。
     *
     * 断言的是「渲染出了几张卡」而不是「屏幕上有那段文字」：`AssistantMarkdownCard` 的正文是
     * Markwon 渲染进 `AndroidView` 里的 TextView（见 `MarkdownText.kt`），而 `onNodeWithText` 读的是
     * Compose 语义树，interop View 内部的文字不在里面。文字级断言留给用 Compose 原生 `Text` 的卡
     * （用户消息、工具卡），见下面两条。
     */
    @Test
    fun realGoldenVectorRendersCardsOnScreen() {
        val units = unitsOf("long-reply")
        assertTrue("前提：assembler 要从真 vector 产出单元", units.isNotEmpty())

        compose.setContent {
            AppTheme { ChatScreen(state = chatStateOf(*units.toTypedArray()), onSend = {}, onStop = {}) }
        }

        // v2 语义下必须先 waitUntil，直接断言就是 flaky
        compose.waitUntil(TIMEOUT_MS) {
            compose.onAllNodesWithTag(ChatTestTags.PANE).fetchSemanticsNodes().size > 0
        }
        compose.onNodeWithTag(ChatTestTags.PANE).assertIsDisplayed()
        compose.waitUntil(TIMEOUT_MS) {
            compose
                .onNodeWithTag(ChatTestTags.PANE)
                .onChildren()
                .fetchSemanticsNodes()
                .isNotEmpty()
        }
    }

    /**
     * 文字级验证：用户消息用的是 Compose 原生 `Text`（`UserTextCard`），能直接断言。
     *
     * 这条补上了上面那条因为 AndroidView 而做不到的事：证明「内容真的到了屏幕上」，不只是「有个容器渲染了」。
     */
    @Test
    fun userMessageTextActuallyReachesTheScreen() {
        compose.setContent {
            AppTheme {
                ChatScreen(
                    state = chatStateOf(RenderUnit.UserText("u#0", "帮我看看 total_value 为什么算错了", null)),
                    onSend = {},
                    onStop = {},
                )
            }
        }
        compose.waitUntil(TIMEOUT_MS) {
            compose.onAllNodesWithText("total_value", substring = true).fetchSemanticsNodes().size > 0
        }
        compose.onNodeWithText("total_value", substring = true).assertIsDisplayed()
    }

    /** 演示素材同样渲染得出：它带工具卡（`Bash`/`Read`/`Edit`），覆盖的卡片种类比 long-reply 多。 */
    @Test
    fun demoVectorWithToolCardsRenders() {
        val units = unitsOf("demo-debug")
        assertTrue("演示素材应含工具调用", units.any { it is RenderUnit.ToolCall })

        compose.setContent {
            AppTheme { ChatScreen(state = chatStateOf(*units.toTypedArray()), onSend = {}, onStop = {}) }
        }
        compose.waitUntil(TIMEOUT_MS) { compose.onAllNodesWithTag(ChatTestTags.PANE).fetchSemanticsNodes().size > 0 }
        compose.onNodeWithTag(ChatTestTags.PANE).assertIsDisplayed()
        // 工具卡的名字是 Compose 原生 Text ⇒ 可做文字级断言
        compose.waitUntil(TIMEOUT_MS) {
            compose.onAllNodesWithText("Bash", substring = true).fetchSemanticsNodes().isNotEmpty()
        }
    }

    /** 流式中显示「停止」而非「发送」：两个按钮不该同时在（会不知道点哪个）。 */
    @Test
    fun stopReplacesSendWhileStreaming() {
        compose.setContent {
            AppTheme {
                ChatScreen(state = ChatUiState(streaming = true), onSend = {}, onStop = {})
            }
        }
        compose.waitUntil(TIMEOUT_MS) { compose.onAllNodesWithTag(ChatTestTags.STOP).fetchSemanticsNodes().size > 0 }
        compose.onNodeWithTag(ChatTestTags.STOP).assertIsDisplayed()
        assertTrue("流式中不该同时出现发送键", compose.onAllNodesWithTag(ChatTestTags.SEND).fetchSemanticsNodes().size == 0)
    }

    /** 输入后点发送 → 回调拿到文本。 */
    @Test
    fun typingAndSendingInvokesCallback() {
        var sent: String? = null
        compose.setContent {
            AppTheme { ChatScreen(state = ChatUiState(), onSend = { sent = it }, onStop = {}) }
        }
        compose.waitUntil(TIMEOUT_MS) { compose.onAllNodesWithTag(ChatTestTags.INPUT).fetchSemanticsNodes().size > 0 }
        compose.onNodeWithTag(ChatTestTags.INPUT).performTextInput("你好")
        compose.waitUntil(TIMEOUT_MS) { compose.onAllNodesWithTag(ChatTestTags.SEND).fetchSemanticsNodes().size > 0 }
        compose.onNodeWithTag(ChatTestTags.SEND).performClick()
        compose.waitUntil(TIMEOUT_MS) { sent != null }
        assertTrue("发送回调应拿到输入的文本，实际=$sent", sent == "你好")
    }

    /** 中断的 turn 必须看得见。 */
    @Test
    fun abortedTurnShowsAVisibleReason() {
        compose.setContent {
            AppTheme {
                ChatScreen(
                    state = ChatUiState(streaming = false, failedWhy = "aborted_streaming"),
                    onSend = {},
                    onStop = {},
                )
            }
        }
        compose.waitUntil(TIMEOUT_MS) { compose.onAllNodesWithText("aborted_streaming", substring = true).fetchSemanticsNodes().size > 0 }
        compose.onNodeWithText("aborted_streaming", substring = true).assertIsDisplayed()
    }

    // ---- 上滑翻历史 --------------------------------------------------------

    /** 「已是最早」必须说出来：不说的话会一直往上拽，以为卡了。 */
    @Test
    fun reachingTheOldestShowsAnExplicitTerminalMessage() {
        compose.setContent {
            AppTheme {
                ChatScreen(
                    state =
                        ChatUiState(
                            units = listOf(RenderUnit.UserText("u#0", "第一句", null)),
                            // 必须显式接了翻页才谈得上「到顶」；只写 canLoadOlder=false
                            //   钉住的是「没接翻页的屏常驻假横幅」那个错误行为
                            historyPagingAttached = true,
                            canLoadOlder = false,
                        ),
                    onSend = {},
                    onStop = {},
                )
            }
        }
        compose.waitUntil(TIMEOUT_MS) {
            compose.onAllNodesWithTag(ChatTestTags.NO_MORE_HISTORY).fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithTag(ChatTestTags.NO_MORE_HISTORY).assertIsDisplayed()
    }

    /** 正在载入与「已是最早」是两个不同的态，不许混成一个。 */
    @Test
    fun loadingOlderAndExhaustedAreDistinctVisibleStates() {
        compose.setContent {
            AppTheme {
                ChatScreen(
                    state =
                        ChatUiState(
                            units = listOf(RenderUnit.UserText("u#0", "第一句", null)),
                            historyPagingAttached = true,
                            canLoadOlder = true,
                            loadingOlder = true,
                        ),
                    onSend = {},
                    onStop = {},
                )
            }
        }
        compose.waitUntil(TIMEOUT_MS) {
            compose.onAllNodesWithTag(ChatTestTags.LOADING_OLDER).fetchSemanticsNodes().isNotEmpty()
        }
        assertTrue(
            "正在载入时不该同时显示「已是最早」",
            compose.onAllNodesWithTag(ChatTestTags.NO_MORE_HISTORY).fetchSemanticsNodes().isEmpty(),
        )
    }

    /** 翻页失败要看得见，且提示可以重试。 */
    @Test
    fun historyLoadFailureIsVisible() {
        compose.setContent {
            AppTheme {
                ChatScreen(
                    state = ChatUiState(units = listOf(RenderUnit.UserText("u#0", "x", null)), historyError = "连接超时"),
                    onSend = {},
                    onStop = {},
                )
            }
        }
        compose.waitUntil(TIMEOUT_MS) {
            compose.onAllNodesWithText("连接超时", substring = true).fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithTag(ChatTestTags.HISTORY_ERROR).assertIsDisplayed()
    }

    /**
     * prepend 之后滚动位置要稳住。
     *
     * 造一屏放不下的列表，滚到顶触发翻页，往前插一批；断言翻页前看着的那条内容还在视野里。
     * 若位置补偿失效，视图会停在原下标上，也就是内容整体往下窜了一屏。
     */
    @Test
    fun prependingOlderUnitsKeepsTheAnchorItemVisible() {
        val anchorText = "这是插入前排在最上面的那条"
        var loaded = false
        val newer = (0 until 20).map { RenderUnit.UserText("new#$it", if (it == 0) anchorText else "新内容 $it", null) }
        val older = (0 until 20).map { RenderUnit.UserText("old#$it", "更早内容 $it", null) }

        compose.setContent {
            AppTheme {
                val units = remember { mutableStateOf<List<RenderUnit>>(newer) }
                ChatScreen(
                    state = ChatUiState(units = units.value, canLoadOlder = !loaded),
                    onSend = {},
                    onStop = {},
                    onLoadOlder = {
                        if (!loaded) {
                            loaded = true
                            units.value = older + newer
                        }
                    },
                )
            }
        }

        compose.waitUntil(TIMEOUT_MS) {
            compose.onAllNodesWithText(anchorText, substring = true).fetchSemanticsNodes().isNotEmpty()
        }
        // 起始就在顶部 ⇒ 触发翻页
        compose.waitUntil(TIMEOUT_MS) { loaded }
        compose.waitForIdle()

        // 锚点内容必须仍在视野里；若补偿失效，视野会被更早的 20 条顶掉
        compose.onNodeWithText(anchorText, substring = true).assertIsDisplayed()
        // 能区分的那半条：补进来的最老一条不该在视野里。
        //   只断言「锚点可见」是不够的：若视图被顶到列表最前面，锚点也可能恰好还在屏内。
        //   这条一红就说明位置没稳住（视野整个被更早的内容占了）。
        assertTrue(
            "prepend 之后视野不该跳到最老的那条",
            compose.onAllNodesWithText("更早内容 0", substring = true).fetchSemanticsNodes().isEmpty(),
        )
    }

    // ---- 停止键与草稿保活 -------------------------------------------

    /**
     * 有消息在飞时显示「停止」而不是「发送」。
     *
     * 判据是 `streaming || sending`：只看 `streaming`（轮次态，由 `res` 决定）的话，`res` 到达之后再发消息
     * 界面上根本没有停止键。这条钉住后半截：轮次已结束但消息在飞。
     */
    @Test
    fun aMessageInFlightShowsStopEvenAfterTheTurnEnded() {
        compose.setContent {
            AppTheme {
                ChatScreen(
                    // 前提：轮次已结束（streaming=false），只有这样才测得到 `sending` 那一半
                    state = ChatUiState(streaming = false, sending = true),
                    onSend = {},
                    onStop = {},
                )
            }
        }
        compose.waitUntil(TIMEOUT_MS) {
            compose.onAllNodesWithTag(ChatTestTags.STOP).fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithTag(ChatTestTags.STOP).assertIsDisplayed()
        assertTrue(
            "在飞时不该同时出现发送键",
            compose.onAllNodesWithTag(ChatTestTags.SEND).fetchSemanticsNodes().isEmpty(),
        )
    }

    /**
     * 草稿保活：转屏（Activity 重建）之后草稿还在。
     *
     * 宿主 `ChatReplayActivity` 没有 `configChanges`，转屏必然重建；用 `remember` 的话一段长 prompt 转个屏就没了。
     * 这里用 `StateRestorationTester` 模拟保存/恢复，它走的正是 `rememberSaveable` 的那条 `SavedStateRegistry` 路径。
     */
    @Test
    fun theDraftSurvivesActivityRecreation() {
        val restorationTester = StateRestorationTester(compose)
        restorationTester.setContent {
            AppTheme { ChatScreen(state = ChatUiState(), onSend = {}, onStop = {}) }
        }

        compose.waitUntil(TIMEOUT_MS) {
            compose.onAllNodesWithTag(ChatTestTags.INPUT).fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithTag(ChatTestTags.INPUT).performTextInput("一段写了很久的长 prompt")
        compose.waitUntil(TIMEOUT_MS) {
            compose.onAllNodesWithText("一段写了很久", substring = true).fetchSemanticsNodes().isNotEmpty()
        }

        restorationTester.emulateSavedInstanceStateRestore()

        compose.waitUntil(TIMEOUT_MS) {
            compose.onAllNodesWithText("一段写了很久", substring = true).fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithText("一段写了很久", substring = true).assertIsDisplayed()
    }

    private companion object {
        const val TIMEOUT_MS = 5_000L
        const val PROBE_CHARS = 12
    }

    /** 没接翻页的屏不许显示「已是最早」：那是把「没这功能」讲成了「已到顶」。 */
    @Test
    fun aScreenWithoutHistoryPagingShowsNoTerminalBanner() {
        compose.setContent {
            AppTheme {
                ChatScreen(
                    state = ChatUiState(units = listOf(RenderUnit.UserText("u#0", "只有这一句", null))),
                    onSend = {},
                    onStop = {},
                )
            }
        }
        compose.waitUntil(TIMEOUT_MS) {
            compose.onAllNodesWithText("只有这一句", substring = true).fetchSemanticsNodes().isNotEmpty()
        }
        assertTrue(
            "没接翻页就不该有「已是最早」",
            compose.onAllNodesWithTag(ChatTestTags.NO_MORE_HISTORY).fetchSemanticsNodes().isEmpty(),
        )
    }

    /**
     * 内容从「不满一屏」长到「超过一屏」之后，上滑必须能触发翻页。
     *
     * `scrollable` 必须在 `LaunchedEffect` 的 key 里，不能只在效果体里：首帧只有 1 条内容、不满一屏 ⇒
     * 判定「不加载」；之后内容长到十几条，key 一个都没变 ⇒ 效果再也不重跑，上滑永远触发不了。
     *
     * `prependingOlderUnitsKeepsTheAnchorItemVisible` 抓不到它：那条一开始就给了 20 条内容（已经超过一屏）。
     */
    @Test
    fun paginationTriggersAfterContentGrowsBeyondOneScreen() {
        var requested = false
        val grown = (0 until 30).map { RenderUnit.UserText("g#$it", "内容 $it", null) }

        compose.setContent {
            AppTheme {
                val units = remember { mutableStateOf<List<RenderUnit>>(listOf(grown.first())) }
                ChatScreen(
                    state = ChatUiState(units = units.value, historyPagingAttached = true, canLoadOlder = true),
                    onSend = {},
                    onStop = {},
                    onLoadOlder = { requested = true },
                )
                LaunchedEffect(Unit) { units.value = grown }
            }
        }

        // 内容长起来之后（超过一屏、且仍在顶部）必须请求更早的一段
        compose.waitUntil(TIMEOUT_MS) { requested }
        assertTrue("内容超过一屏后上滑到顶必须触发翻页", requested)
    }
}
