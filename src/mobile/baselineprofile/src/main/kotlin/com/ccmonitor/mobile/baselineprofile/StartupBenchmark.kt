package com.ccmonitor.mobile.baselineprofile

import androidx.benchmark.macro.BaselineProfileMode
import androidx.benchmark.macro.CompilationMode
import androidx.benchmark.macro.FrameTimingMetric
import androidx.benchmark.macro.StartupMode
import androidx.benchmark.macro.StartupTimingMetric
import androidx.benchmark.macro.junit4.MacrobenchmarkRule
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.Parameterized

/**
 * 量两条路，每种编译方式各 [ITERATIONS] 趟：
 * - `None`：不预编译，全靠解释与 JIT（没有任何 profile 的样子）；
 * - `Partial(Require)`：照包里带的 profile 预编译（侧装的包经 profileinstaller 装上之后的样子；包里没有 profile 就红）。
 * 冷启读 `StartupTimingMetric`（timeToInitialDisplay），长会话往上滑读 `FrameTimingMetric`。
 * 跑法：`scripts/baseline-profile.sh bench`（`:baselineprofile:connectedBenchmarkMinifiedAndroidTest`）；模拟器上的数只作相对比较。
 */
@RunWith(Parameterized::class)
class StartupBenchmark(
    private val mode: CompilationMode,
) {
    @get:Rule
    val rule = MacrobenchmarkRule()

    @Test
    fun coldStartToMainScreen() =
        rule.measureRepeated(
            packageName = PACKAGE,
            metrics = listOf(StartupTimingMetric()),
            compilationMode = mode,
            startupMode = StartupMode.COLD,
            iterations = ITERATIONS,
            setupBlock = {
                seed()
                killProcess()
            },
        ) { coldStartToMainScreen() }

    @Test
    fun openLongSessionAndScrollUp() =
        rule.measureRepeated(
            packageName = PACKAGE,
            metrics = listOf(FrameTimingMetric()),
            compilationMode = mode,
            iterations = ITERATIONS,
            // 每一趟都从刚起的进程点进那条会话（第一次打开那一屏的头几秒才是要看的），起 App 那一段不计。
            setupBlock = {
                killProcess()
                seed()
                killProcess()
                coldStartToMainScreen()
            },
        ) { openLongSessionAndScrollUp() }

    companion object {
        private const val ITERATIONS = 5

        @JvmStatic
        @Parameterized.Parameters(name = "{0}")
        fun modes(): List<CompilationMode> = listOf(CompilationMode.None(), CompilationMode.Partial(BaselineProfileMode.Require))
    }
}
