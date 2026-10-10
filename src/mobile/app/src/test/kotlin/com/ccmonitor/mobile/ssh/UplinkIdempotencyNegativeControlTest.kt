package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.claude.bridge.PipeSession
import com.ccmonitor.mobile.core.claude.bridge.SendOutcome
import com.ccmonitor.mobile.core.claude.bridge.SendRequest
import com.ccmonitor.mobile.core.claude.bridge.UplinkIdentity
import com.ccmonitor.mobile.core.claude.bridge.UplinkSink
import com.ccmonitor.mobile.core.claude.model.DeliveryState
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.ui.chat.ChatSession
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

/**
 * 阴性对照：那两条「看起来在验重复」的判据，在远端真的重复时全绿。
 *
 * `ChatSessionTest.retryingUsesTheSameLocalIdSoNoDuplicateAppears` 与
 * `ChatSessionTest.anEchoPathRetrySuccessDoesNotDuplicateOnceTheEchoArrives`
 * 守的是本地那一侧：同一个 `localId` ⇒ 同一个 `LazyColumn` key ⇒ 列表里不会多出第二条。
 * 它们与远端有几行毫无关系。本条把这件事钉成常绿判据，防下一个人拿那两条当幂等证据。
 *
 * 两半都要断：前半「远端读数 = 2」，后半「那两条仍然绿」（屏上仍然 1 条）。
 * 只断前半就成了幂等判据的重复；只断后半就没有判别力。
 *
 * 本条自己的失效形态：将来有人把那两条改成真的读远端，本条的后半会红。
 * 那时正确的动作是删掉本条（删之前先证明它恒绿），不是放宽它。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class UplinkIdempotencyNegativeControlTest {
    @get:Rule
    val tmp = TemporaryFolder()

    private val dispatcher = StandardTestDispatcher()

    @Before fun setMain() = Dispatchers.setMain(dispatcher)

    @After fun clearMain() = Dispatchers.resetMain()

    /**
     * 造一次「远端真的收到两遍」，然后两侧各读一次数。
     *
     * 最后那条断言是「不是失败态」，不是「`delivery == null`」：`SendOutcome.Accepted` 的默认落点是
     * [DeliveryState.SENT_UNCONFIRMED]（「已发出，还没看到对面的反应」），因为 SSH 上 `send-keys` 退 0
     * 分不清送没送达。本条要钉的性质是「重试成功之后屏上看不出任何异常」：不再是失败态、也不再有重试按钮。
     * 判据 `delivery !in 两个失败态` 会放过 `SENDING`，所以下面补了一条：重试成功后不许还停在 `SENDING`。
     */
    @Test
    fun theTwoExistingDuplicateJudgesStayGreenWhileTheRemoteReallyGotItTwice() =
        runTest(dispatcher) {
            val remote = File(tmp.root, "in.ndjson")
            // 替身：每次 send 都真的往远端追加一行（也就是把检查拿掉）。
            //   第一次答「失败」（模拟「发出去了但应答丢了」，会被画成「没送到」），
            //   于是屏上出现重试按钮；第二次答成功。
            val sink = BlindlyAppendingSink(remote, mutableListOf(SendOutcome.Rejected("应答丢了"), SendOutcome.Accepted))

            val vm = ChatSession(sink)
            vm.start(flowOf(), smooth = false)
            vm.send("同一句话")
            advanceUntilIdle()

            val failed =
                vm.state.value.units
                    .filterIsInstance<RenderUnit.UserText>()
                    .single()
            assertEquals("前提：第一次必须真的被画成失败（这就是重试按钮的来源）", DeliveryState.FAILED, failed.delivery)

            vm.retry(failed.key)
            advanceUntilIdle()

            // ---- 前半：新判据（远端字节读数）在这里必然红 -------------------------------
            val id = sink.wireIds.first()
            assertEquals("前提：两次上行用的是同一个幂等标识", 1, sink.wireIds.toSet().size)
            assertEquals(
                "远端读数 = 2：同一句话真的进对话两遍。远端字节那条判据在这一格必然红",
                2,
                occurrences(remote, PipeSession.uplinkIdNeedle(id)),
            )
            val distinct =
                remote
                    .readLines()
                    .filter { it.isNotBlank() }
                    .toSet()
            assertEquals("两行逐字节相同", 1, distinct.size)

            // ---- 后半：那两条判据守的东西，在同一格里全绿 -------------------------
            val after =
                vm.state.value.units
                    .filterIsInstance<RenderUnit.UserText>()
            assertEquals(
                "`retryingUsesTheSameLocalIdSoNoDuplicateAppears` 那条断言：重试不许多出一条：它在这里绿",
                1,
                after.size,
            )
            val localIds =
                sink.requests
                    .map { it.localId }
                    .toSet()
            assertEquals("它的第二条断言：两次请求用同一个 localId：也绿", 1, localIds.size)
            // 重试成功后不再是失败态：屏上看不出任何异常（没有重试按钮、没有红字）。
            //    不比 `== null`：`Accepted` 落 `SENT_UNCONFIRMED`（SSH 上分不清送没送达，说「已送达」是假话）。
            val delivery = after.single().delivery
            assertFalse(
                "重试成功后不再是失败态：屏上看不出任何异常，实得：$delivery",
                delivery in setOf(DeliveryState.FAILED, DeliveryState.FAILED_PERMANENT),
            )
            assertNotEquals(
                "也不许还停在「正在送」：那同样是个屏上没结束的状态",
                DeliveryState.SENDING,
                delivery,
            )

            // ⇒ 结论（本条要钉的那个事实）：那两条判据守的是本地 LazyColumn key，
            //   远端有几行它们一个字都不知道。拿它们当幂等证据是错的。
        }

    /**
     * 幂等标识是机器码，不许上屏。
     *
     * 判法是零命中守卫：整个 `ui/` 目录里不许出现标识的键名或铸造它的类，
     * 上屏必然要先在某个 `ui/` 文件里拿到它。
     *
     * 注意：只扫键名的值（`"aterm_id"`）与类名是不够的。`val k = PipeSession.UPLINK_ID_KEY` 那行源码里
     * 根本没有 `aterm_id` 这几个字节，只有常量的名字；而生产代码明令用具名常量，走常量名恰恰是最可能的上屏形态。
     * 所以针是 [FORBIDDEN_IN_UI]：值与名两侧都护。
     *
     * `UplinkIdentity.PREFIX`（值 `aterm-`）不能当针：`ui/` 下有合法的 `aterm-key-` / `aterm-att-`，拿它当针会常红。
     */
    @Test
    fun theUplinkIdNeverReachesTheUi() {
        val ui = uiSourceRoot()
        val offenders =
            ui
                .walkTopDown()
                .filter { it.isFile && it.extension == "kt" }
                .mapNotNull { f ->
                    val bad =
                        f.readLines().withIndex().filter { (_, line) ->
                            FORBIDDEN_IN_UI.any { line.contains(it) }
                        }
                    if (bad.isEmpty()) null else "${f.path}: ${bad.map { it.index + 1 }}"
                }.toList()
        assertTrue("幂等标识不许出现在 ui/ 下（机器码不给用户看）：$offenders", offenders.isEmpty())

        // 前提：这个扫描真的在扫东西，否则目录写错时它是空真
        val scanned = ui.walkTopDown().count { it.isFile && it.extension == "kt" }
        assertTrue("前提：ui/ 下要真的有 Kotlin 文件被扫到，实得 $scanned", scanned > MIN_UI_FILES)

        // 另一侧：失败原因里也不许夹带标识（那是最容易漏的上屏口）
        val reason = SendOutcome.Rejected("没写进去，也没说为什么").reason
        assertFalse("失败文案里不许有标识", reason.contains(UplinkIdentity.PREFIX))
    }

    /**
     * 替身 sink：真的往一个本地文件追加，且不做任何检查（盲追加）。
     *
     * 它铸标识用的是生产那套 [UplinkIdentity]，所以「两次上行同一个标识」这件事
     * 不是本文件假设出来的，是量出来的。
     */
    private class BlindlyAppendingSink(
        private val remote: File,
        private val outcomes: MutableList<SendOutcome>,
    ) : UplinkSink {
        private val identity = UplinkIdentity()
        val requests = mutableListOf<SendRequest>()
        val wireIds = mutableListOf<String>()

        override val echoesBack: Boolean = false

        override suspend fun send(request: SendRequest): SendOutcome {
            requests += request
            val id = identity.idFor(request)
            wireIds += id
            remote.appendText(PipeSession.userLineJson(request.text, id) + "\n")
            return outcomes.removeFirstOrNull() ?: SendOutcome.Accepted
        }

        override suspend fun interrupt(): SendOutcome = SendOutcome.Rejected("本替身不测中断", retryable = false)
    }

    private companion object {
        /** `ui/` 下的 Kotlin 文件数下限：只为证明扫描不是空转。 */
        const val MIN_UI_FILES = 10

        /**
         * 上屏守卫的针：值与名两侧都护。
         *
         * 拿到标识只有这几条门：手写键名字面量、引用那个具名常量、引用铸它的类、
         * 或者去拿远端 grep 的那根针。四条都得堵，缺任何一条这条守卫就是假绿。
         */
        val FORBIDDEN_IN_UI: List<String> =
            listOf(
                // 值：`"aterm_id"`。有人手写字面量时命中。
                PipeSession.UPLINK_ID_KEY,
                // 名：生产代码明令用具名常量，这才是最可能的形态。
                "UPLINK_ID_KEY",
                // 铸它的类（连带 `UplinkIdentity.PREFIX` 这种拼法）。
                "UplinkIdentity",
                // 远端 grep 的那根针：拿到它等于拿到了标识。
                "uplinkIdNeedle",
            )

        fun uiSourceRoot(): File {
            val fromModule = File("src/main/kotlin/com/ccmonitor/mobile/ui")
            if (fromModule.isDirectory) return fromModule
            val fromRepo = File("app/src/main/kotlin/com/ccmonitor/mobile/ui")
            require(fromRepo.isDirectory) { "找不到 ui 源码根，cwd=${File(".").absolutePath}" }
            return fromRepo
        }

        fun occurrences(
            file: File,
            needle: String,
        ): Int {
            val text = file.takeIf { it.exists() }?.readText() ?: return 0
            var n = 0
            var at = text.indexOf(needle)
            while (at >= 0) {
                n++
                at = text.indexOf(needle, at + needle.length)
            }
            return n
        }
    }
}
