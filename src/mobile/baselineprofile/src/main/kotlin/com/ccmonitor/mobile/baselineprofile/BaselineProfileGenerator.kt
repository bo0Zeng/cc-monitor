package com.ccmonitor.mobile.baselineprofile

import androidx.benchmark.macro.junit4.BaselineProfileRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * 录 Baseline Profile：两条路（冷启到主屏 · 打开长会话往上滑）各走几遍，规则并进 app 的 `src/main/generated/baselineProfiles/`。
 * 跑法：`scripts/baseline-profile.sh gen`（`:app:generateMinifiedBaselineProfile`）。界面大改之后重录。
 */
@RunWith(AndroidJUnit4::class)
class BaselineProfileGenerator {
    @get:Rule
    val rule = BaselineProfileRule()

    @Test
    fun coldStartAndLongSession() =
        rule.collect(packageName = PACKAGE) {
            seed()
            coldStartToMainScreen()
            openLongSessionAndScrollUp()
        }
}
