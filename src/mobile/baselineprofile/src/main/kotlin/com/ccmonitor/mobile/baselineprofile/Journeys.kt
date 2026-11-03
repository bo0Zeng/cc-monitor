package com.ccmonitor.mobile.baselineprofile

import androidx.benchmark.macro.MacrobenchmarkScope
import androidx.test.uiautomator.By
import androidx.test.uiautomator.BySelector
import androidx.test.uiautomator.Direction
import androidx.test.uiautomator.UiObject2
import androidx.test.uiautomator.Until

// 录 profile 与量冷启共用的两条路。造的数（一台不可达的机器 ＋ 一条长会话）由 app 量测那两族里的 `BenchSeedActivity` 写，
// 不连机器、不登账号。屏上要认的字：抽屉的两处取自文案表（BuildConfig，见 build.gradle.kts）；会话名与正文是造数里的 ASCII。

const val PACKAGE = "com.ccmonitor.mobile"

/** 造数里那条长会话在列表上的名字（`BenchSeedActivity.TITLE`）。 */
private const val LONG_SESSION = "bench-long-session"

/** 造数里每一条答都有的那一句（标题那一行）。 */
private const val ANSWER_MARK = "decoder path"

/** 往上滑几屏。 */
private const val SCREENS = 6

private const val WAIT_MS = 15_000L

/** 每一组之前：授通知权限（API 33+ 首启弹的框会抢前台）、写造的数（同一份写几遍结果一样）。 */
fun MacrobenchmarkScope.seed() {
    device.executeShellCommand("pm grant $PACKAGE android.permission.POST_NOTIFICATIONS")
    device.executeShellCommand("am start -W -n $PACKAGE/.bench.BenchSeedActivity")
}

/** 冷启到主屏：起 App，等落地那一屏（上次那台的新建会话）的抽屉按钮出来。 */
fun MacrobenchmarkScope.coldStartToMainScreen() {
    pressHome()
    startActivityAndWait()
    waitFor(By.desc(BuildConfig.DRAWER_OPEN))
}

/** 打开长会话往上滑：抽屉 → 会话 → 点那条长会话 → 正文照缓存上屏 → 往上滑 [SCREENS] 屏。 */
fun MacrobenchmarkScope.openLongSessionAndScrollUp() {
    waitFor(By.desc(BuildConfig.DRAWER_OPEN)).click()
    waitFor(By.text(BuildConfig.DRAWER_SESSIONS))
    // 抽屉盖在那一屏上，同一个字底下那一屏也可能有：点画在最上面的那一个（节点排在最后）。
    device.findObjects(By.text(BuildConfig.DRAWER_SESSIONS)).last().click()
    waitFor(By.text(LONG_SESSION)).click()
    waitFor(By.textContains(ANSWER_MARK))
    val list = device.findObjects(By.scrollable(true)).maxBy { it.visibleBounds.height() }
    list.setGestureMargin(device.displayWidth / 5)
    repeat(SCREENS) {
        list.fling(Direction.UP)
        device.waitForIdle()
    }
}

private fun MacrobenchmarkScope.waitFor(selector: BySelector): UiObject2 =
    device.wait(Until.findObject(selector), WAIT_MS) ?: error("not on screen within $WAIT_MS ms: $selector")
