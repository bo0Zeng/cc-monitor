package com.ccmonitor.mobile.service

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * 「到点就停」这条判断，与「现在有没有活」无关。
 *
 * 本服务在清单里是 `dataSync` 型、`targetSdk = 36`（> 35），吃 Android 15 那条「24 小时内合计 6 小时」的额度。
 * 到点系统调 `Service.onTimeout(int, int)`，此时服务已不算前台服务，只有几秒可以 `stopSelf()`；
 * 不停 ⇒ `RemoteServiceException` ⇒ 崩。
 *
 * 最容易写错的不是「忘了实现 onTimeout」，而是「实现了、但顺手复用了既有的自停条件」。
 * 既有条件是「没会话也没在飞轮次才停」；一个跑满 6 小时的服务多半正有活，复用它就正好在该停的时候不停。
 * 这条判据钉的就是这一格：[KeepAlivePolicy.stopReasonFor] 对 `SystemTimeout` 必须不看 `hasWork`。
 *
 * 测纯函数而不是测 `Service`：`onTimeout` 是系统回调，`app` 模块没引 Robolectric、门禁不跑 `androidTest`。
 * 所以回调里唯一会写错的那段判断住在 [KeepAlivePolicy]（纯函数、无 Android 依赖），判据量它；
 * 「系统到底会不会在 6 小时调我」「`stopSelf` 到底停没停」由 `ForegroundServiceCrashGuardTest` 从结构上兜一半。
 *
 * ### 判别力边界
 *
 * | 这一格 | 本判据 |
 * |---|---|
 * | 「`SystemTimeout` 不看 `hasWork`」 | 测到（`hasWork = true` 也必须停） |
 * | 「`NoWork` 一支就是『没会话也没在飞轮次』」 | 测到（四种组合全覆） |
 * | 「版本门：API 31 之前不认那族异常」 | 测到（两头断） |
 * | 「真的 `ForegroundServiceStartNotAllowedException` 认得出」 | 测不到：JVM 单测里那个类是桩，构造器一调就抛，造不出实例。结构上由 `ForegroundServiceCrashGuardTest` 钉住「`isStartRefusal` 里引用了那个类型」，真要验得 Android 12+ 真机 |
 * | 「系统真的会在 6 小时调 `onTimeout`」 | 测不到，是读官方文档的推断。要验：Android 15+ 真机 ＋ `adb shell am compat enable FGS_INTRODUCE_TIME_LIMITS <pkg>` ＋ `adb shell device_config put activity_manager data_sync_fgs_timeout_duration <ms>` |
 * | 「`stopSelf()` 之后系统真的不再抛」 | 测不到，要真机 |
 */
class KeepAlivePolicyTest {
    @Before
    fun clearFact() {
        // `object` 是 JVM 级单例，判据之间不许串味。
        KeepAliveStopFact.reset()
    }

    /**
     * 系统说额度到点了 ⇒ 照停，哪怕正有活。
     *
     * 变异靶子：把 [KeepAlivePolicy.stopReasonFor] 里 `SystemTimeout` 那一支改成
     * `candidate.takeIf { !hasWork }`（= 顺手复用既有自停条件）⇒ 本条当场红。
     */
    @Test
    fun aSystemTimeoutStopsTheServiceEvenWhileWorkIsStillInFlight() {
        assertEquals(
            "额度到点正有活时也必须停：不停的话几秒后 RemoteServiceException。" +
                "这是本格唯一真正会写错的地方（顺手复用了「没活才停」）。",
            KeepAliveStopReason.SystemTimeout,
            KeepAlivePolicy.stopReasonFor(KeepAliveStopReason.SystemTimeout, hasWork = true),
        )
        assertEquals(
            "没活时当然也停（这一半本来就对，断它是为了说明上面那条不是碰巧绿）",
            KeepAliveStopReason.SystemTimeout,
            KeepAlivePolicy.stopReasonFor(KeepAliveStopReason.SystemTimeout, hasWork = false),
        )
    }

    /** 起不成前台 ⇒ 照停（留着一个非前台的服务，只会被系统收走，且没人知道为什么）。 */
    @Test
    fun aRefusedStartStopsTheServiceRegardlessOfWork() {
        for (hasWork in listOf(true, false)) {
            assertEquals(
                "startForeground 被拒 ⇒ 必须停（hasWork=$hasWork）",
                KeepAliveStopReason.StartRefused,
                KeepAlivePolicy.stopReasonFor(KeepAliveStopReason.StartRefused, hasWork = hasWork),
            )
        }
    }

    /** 划走任务 ⇒ 照停（划走 = 用完了）。 */
    @Test
    fun swipingTheTaskAwayStopsTheServiceRegardlessOfWork() {
        for (hasWork in listOf(true, false)) {
            assertEquals(
                "划走任务 ⇒ 必须停（hasWork=$hasWork）",
                KeepAliveStopReason.TaskRemoved,
                KeepAlivePolicy.stopReasonFor(KeepAliveStopReason.TaskRemoved, hasWork = hasWork),
            )
        }
    }

    /**
     * `NoWork` 一支就是 `n == 0 && !inFlight`（没会话也没在飞轮次）。
     *
     * 四种 `(n, inFlight)` 组合全覆。
     */
    @Test
    fun theNoWorkBranchIsExactlyNoSessionsAndNothingInFlight() {
        for (n in listOf(0, 1, 7)) {
            for (inFlight in listOf(true, false)) {
                val before = n == 0 && !inFlight // 「没会话也没在飞轮次」那句条件
                val after = KeepAlivePolicy.stopReasonFor(KeepAliveStopReason.NoWork, hasWork = n > 0 || inFlight) != null
                assertEquals("自停条件变了（n=$n inFlight=$inFlight）", before, after)
            }
        }
    }

    /** 有活时 `NoWork` 不停（否则会把正在用的保活服务停掉，反方向的 bug）。 */
    @Test
    fun theNoWorkBranchDoesNotStopWhileThereIsWork() {
        assertNull(
            "有活还停 ⇒ 正在用的保活被停掉",
            KeepAlivePolicy.stopReasonFor(KeepAliveStopReason.NoWork, hasWork = true),
        )
        assertNotNull(
            "没活必须停（否则服务永远不自停）",
            KeepAlivePolicy.stopReasonFor(KeepAliveStopReason.NoWork, hasWork = false),
        )
    }

    /**
     * 版本门两头断：API 31 之前压根没有那族异常。
     *
     * 这族异常 API 31 起才有（`api-versions.xml`：`ServiceStartNotAllowedException since=31`）。
     * `minSdk = 26`，所以必须有版本门，否则低版本上会去碰一个不存在的类（`NoClassDefFoundError`）。
     *
     * 注意：断 `isStartRefusal(普通 ISE, sdkInt = 26..30) == false` 量不到门：`Api31.isRefusal` 对普通 ISE 恒 `false`，
     * 把 `sdkInt >= S` 整个摘掉判据照绿。所以版本门是独立的纯函数，这里直接断它；
     * 「[KeepAlivePolicy.isStartRefusal] 真的还在问它」由
     * `ForegroundServiceCrashGuardTest.theRefusalClassifierStillConsultsTheApiGate` 从结构上钉。
     */
    @Test
    fun theApiGateItselfSaysNoBelowApi31AndYesFrom31Up() {
        for (sdkInt in listOf(26, 28, 30)) {
            assertFalse(
                "API $sdkInt 上没有 ServiceStartNotAllowedException 这一族 ⇒ 版本门必须说「不可能」",
                KeepAlivePolicy.startRefusalIsPossible(sdkInt),
            )
        }
        for (sdkInt in listOf(31, 34, 35, 36)) {
            assertTrue(
                "API $sdkInt 上这族异常是存在的 ⇒ 门说「不可能」的话，真被拒时就认不出、于是崩",
                KeepAlivePolicy.startRefusalIsPossible(sdkInt),
            )
        }
    }

    /**
     * 另一头：就算在够新的系统上，一个普通的 `IllegalStateException` 也不许被认成「系统不许起」。
     *
     * 认成了就会被 `onStartCommand` 的 catch 吞掉，真 bug 变成一条 `Log.w`。
     * 本条钉的是「不吞我们不认识的错」那条纪律的判断面。
     */
    @Test
    fun aPlainIllegalStateExceptionIsNeverMistakenForASystemRefusal() {
        val notARefusal = IllegalStateException("通知建错了之类的真 bug")
        for (sdkInt in listOf(31, 34, 35, 36)) {
            assertFalse(
                "API $sdkInt：普通 ISE 不许认成「系统不许起」（认了就会被吞，真 bug 再也看不见）",
                KeepAlivePolicy.isStartRefusal(notARefusal, sdkInt),
            )
        }
    }

    /**
     * 「为什么停了」这个事实真的留下来了（界面要靠它说人话）。
     *
     * 它只答「因由」，不答文案。
     */
    @Test
    fun theStopReasonIsRecordedAsAReadableFact() {
        assertNull("前提：清过之后该是「没停过」", KeepAliveStopFact.last)
        KeepAliveStopFact.record(KeepAliveStopReason.SystemTimeout)
        assertEquals(
            "记不下因由的话，用户只看到「保活没了」而没有任何地方能说为什么",
            KeepAliveStopReason.SystemTimeout,
            KeepAliveStopFact.last,
        )
        KeepAliveStopFact.record(KeepAliveStopReason.NoWork)
        assertEquals("后来的因由该覆盖前一个（它答的是「最近一次」）", KeepAliveStopReason.NoWork, KeepAliveStopFact.last)
    }

    /**
     * 因由表是穷尽的：每一档都得有判决，没有一档会掉进缺省分支。
     *
     * [KeepAlivePolicy.stopReasonFor] 的 `when` 不带 `else` ⇒ 加一档当场编译不过。
     * 本条从运行期再断一遍：现有每一档都拿得到非空判决（`NoWork` 在没活时）。
     */
    @Test
    fun everyDeclaredReasonHasADecisionAndNoneFallsThrough() {
        assertEquals("前提：因由档数变了 ⇒ 请回去看 stopReasonFor 的 when 是否也跟着决定过", 4, KeepAliveStopReason.entries.size)
        for (reason in KeepAliveStopReason.entries) {
            assertEquals(
                "每一档在「没活」时都该判成停（含 NoWork 自己）：有档掉进缺省分支的话这里会是 null",
                reason,
                KeepAlivePolicy.stopReasonFor(reason, hasWork = false),
            )
        }
    }
}
