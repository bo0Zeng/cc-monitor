package com.ccmonitor.mobile.ui.settings

import com.ccmonitor.mobile.core.data.db.Settings
import com.ccmonitor.mobile.core.data.db.SettingsDao
import com.ccmonitor.mobile.core.data.repo.SettingsRepository
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** 新界面开关与对话列表的来源路径。 */
class NewUiSettingsTest {
    /**
     * 「从没设过」必须判成开：新界面默认开，它不是一个可选皮肤而是产品本身。
     *
     * 单看这一条守不住任何东西：一个「无条件返回 true」的实现也全绿。
     * 分辨力在下面那条 [explicitlyDisabledMeansOff]，两条必须同时在。
     */
    @Test
    fun neverConfiguredMeansOn() {
        assertTrue("null（从没设过）⇒ 开", isOn(null))
        assertTrue("空串也算没设过 ⇒ 开", isOn(""))
    }

    /**
     * 显式关掉也必须判成关。
     *
     * 判据写成「有没有值」的话，存进去的 `"0"` 会被读成开 —— 用户关不掉这个开关，
     * 而且从界面上完全看不出哪里错了。
     */
    @Test
    fun explicitlyDisabledMeansOff() {
        assertFalse("存了关值仍然是关", isOn(SettingsRepository.FLAG_OFF))
        assertTrue("存了开值才是开", isOn(SettingsRepository.FLAG_ON))
    }

    /** 开/关两个值必须可区分，且都不是空串（空串会与「没设过」混同）。 */
    @Test
    fun theTwoFlagValuesAreDistinguishable() {
        assertNotEquals(SettingsRepository.FLAG_ON, SettingsRepository.FLAG_OFF)
        assertTrue(SettingsRepository.FLAG_ON.isNotEmpty())
        assertTrue(SettingsRepository.FLAG_OFF.isNotEmpty())
    }

    /**
     * 设置屏上没有「新界面（预览）」这一节。
     *
     * 一个躲在设置里、名字叫「预览」的开关会让产品退回「可选皮肤」的形态。
     * 把那一节写回去（标题、开关、或任何一条通向 `setNewUi` 的路）⇒ 本条当场红。
     *
     * 第三条断言（没有 `setNewUi`）才是承重的那条：只删标题和 tag、留一个照样接在 `vm::setNewUi` 上的开关，
     * 前两条全绿而开关还在。
     *
     * ### 判别力边界
     *
     * | 绕过形态 | 本条 |
     * |---|---|
     * | 标题/开关/`setNewUi` 任一写回 `SettingsScreen.kt` | 红 |
     * | 把开关搬进另一个文件的 composable，再从设置屏调过去 | 抓不到（本条只读这一个文件） |
     */
    @Test
    fun theSettingsScreenOffersNoPreviewToggle() {
        val code = codeOnly(settingsScreenSource())
        assertTrue("前提：得真读到 SettingsScreen.kt 的代码，否则本条判据恒绿", code.contains("fun SettingsScreen("))
        assertFalse("「$PREVIEW_TITLE」那一节不许有：它在就说明产品仍躲在一个预览项后面", code.contains("\"$PREVIEW_TITLE\""))
        assertFalse("那个开关的测试标签也不许有（留一个没人用的 tag = 下一个人以为开关还在）", code.contains(NEW_UI_TAG_NAME))
        assertFalse(
            "设置屏不许再给出任何一条通向 `setNewUi` 的路 —— 只删标题和 tag、" +
                "留一个照样接在 `vm::setNewUi` 上的开关的话，上面两条全绿而开关还在",
            code.contains("setNewUi"),
        )
    }

    /**
     * 剥注释、留字面量，走全仓唯一一份词法扫描器 [KotlinSourceScanner.codeOnlyKeepingLiterals]。
     *
     * 必须留字面：本条断的是 `code.contains("\"<标题>\"")` 这种带引号的文案字面在不在。
     * 「那一栏必须在」是条正向断言，换成「删字面」那支它会假红；而「开关不许有」那几条反向断言会变成恒绿。
     * 它看不见什么写在 [KotlinSourceScanner] 的头注里。
     */
    private fun codeOnly(text: String): String = KotlinSourceScanner.codeOnlyKeepingLiterals(text)

    private fun settingsScreenSource(): String {
        val f =
            java.io.File("src/main/kotlin/com/ccmonitor/mobile/ui/settings/SettingsScreen.kt").takeIf { it.isFile }
                ?: java.io.File("app/src/main/kotlin/com/ccmonitor/mobile/ui/settings/SettingsScreen.kt")
        return f.readText()
    }

    /**
     * 跑真的 [SettingsRepository.newUiEnabled]，不在测试里复述那条判据。
     *
     * 写成 `stored == SettingsRepository.FLAG_ON` 就是把同一份逻辑写了两遍：生产那条改成「有值即开」时，
     * 测试照样绿。喂一个假 DAO 给真 repo，变异才杀得掉。
     */
    private fun isOn(stored: String?): Boolean =
        runBlocking {
            SettingsRepository(
                object : SettingsDao {
                    override fun observe(key: String): Flow<String?> = flowOf(stored)

                    override suspend fun get(key: String): String? = stored

                    override suspend fun upsert(setting: Settings) = Unit
                },
            ).newUiEnabled().first()
        }

    companion object {
        /** 不许出现在设置屏上的节标题。 */
        private const val PREVIEW_TITLE = "新界面（预览）"

        /**
         * 那个开关的测试标签名字（不是值）。
         *
         * 刻意写成字符串而不是引用常量：那个常量不存在，引用它编译不过；「它不该存在」正是本条要断的事。
         */
        private const val NEW_UI_TAG_NAME = "TAG_NEW_UI_SWITCH"

        /** 设置屏上必须在的那一栏（它不是预览的一部分，是来源配置）。 */
    }
}
