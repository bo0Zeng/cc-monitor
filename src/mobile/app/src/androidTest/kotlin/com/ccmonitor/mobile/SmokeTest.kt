package com.ccmonitor.mobile

import android.Manifest
import android.os.Build
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.rule.GrantPermissionRule
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TestRule
import org.junit.runner.RunWith

/**
 * app 模块的冒烟 instrumented 测，也是 Compose UI 测试的样板。
 *
 * 只做冒烟：App 起得来、根屏渲染得出、不崩。不测业务逻辑（那是 JVM 单测的活）。
 *
 * 它比「验了渲染」值钱：`createAndroidComposeRule<MainActivity>()` 会真正拉起 `AtermApp`，于是顺带覆盖了
 * Koin 全模块装配（`startKoin { modules(dataModule, sshModule, appModule) }`）以及根屏 ViewModel 的解析。
 * 往 `AppModule.kt` 加东西时 DI 装配破损是高发故障，会被本测当场抓住。别当 hello-world 删掉。
 *
 * ### UI 测试照抄的三条
 *
 * 1. 用 `junit4.v2` 而非 `junit4`：后者在 compose BOM 2026.05.01 上已 deprecated。
 *    v2 用 `StandardTestDispatcher`（排队）而非 `UnconfinedTestDispatcher`（立即执行）。
 * 2. 必须 `GrantPermissionRule` 预授权 `POST_NOTIFICATIONS`（见下方 rule 注释），否则 API33+ 上
 *    系统权限弹窗抢前台，任何 Compose 测都拿不到 compose 树。
 * 3. 断言前先 `waitUntil`（见测试体注释）：v2 语义下不加就是 flaky。
 */
@RunWith(AndroidJUnit4::class)
class SmokeTest {
    /**
     * 预授权通知权限，必须排在 compose rule 之前（`order = 0`）。
     *
     * `MainActivity.onCreate` 会调 `requestNotificationPermissionIfNeeded()`，在 API33+ 且未授予时
     * 拉起系统 `GrantPermissionsActivity`。它抢走前台 → `MainActivity` 的 compose 树不在顶层 →
     * 测试抛 `No compose hierarchies found in the app`。
     *
     * CI 的 `api-level: 26`（= minSdk）上不会触发，而 `POST_NOTIFICATIONS` 是 API33+ 才有的权限，
     * 所以这里按 SDK 版本门控：低于 33 时退化成 no-op rule（该权限不存在，硬授会报错）。
     */
    @get:Rule(order = 0)
    val permissionRule: TestRule =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            GrantPermissionRule.grant(Manifest.permission.POST_NOTIFICATIONS)
        } else {
            TestRule { base, _ -> base }
        }

    @get:Rule(order = 1)
    val composeRule = createAndroidComposeRule<MainActivity>()

    /**
     * App 冷启动 → `MainActivity` 起来 → Compose 树渲染出根屏。
     *
     * 导航大改时改断言、别删这条测：它的价值正是告诉人根屏还渲染不渲染得出来。
     */
    @Test
    fun appLaunchesAndRendersRootScreen() {
        // 断言前先 waitUntil。v2 用 StandardTestDispatcher，`setContent` 的组合是排队执行的；
        // 断言链里的隐式 waitForIdle 只等「已注册的 compose root 变空闲」，此刻注册表里可能一个 root 都还没有
        // → 它认为无事可等、立即返回 → 断言扑空。
        // 用 runCatching 包住是因为 root 尚未注册时 fetchSemanticsNodes() 是抛异常而非返回空列表。
        composeRule.waitUntil(timeoutMillis = 15_000) {
            runCatching { composeRule.onAllNodesWithText("Hosts").fetchSemanticsNodes().isNotEmpty() }
                .getOrDefault(false)
        }

        // 锚点是 HostListScreen 的 `title` 参数默认值 "Hosts"：由 HostListHeader 首次组合就无条件发出
        // （不经 collectAsState/LaunchedEffect、不依赖 DB 里有没有 host）。
        // 注意：同名文案出现在两处时 onNodeWithText 会抛 "Expected exactly '1' node but found '2'"，那时改用 testTag。
        composeRule.onNodeWithText("Hosts").assertIsDisplayed()
    }
}
