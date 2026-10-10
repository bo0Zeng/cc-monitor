package com.ccmonitor.mobile.ui.settings

import com.ccmonitor.mobile.core.data.db.Settings
import com.ccmonitor.mobile.core.data.db.SettingsDao
import com.ccmonitor.mobile.core.data.repo.SettingsRepository
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import com.ccmonitor.mobile.ui.overview.DEFAULT_SOURCE_PATH
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
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
     * 来源留空 = 用默认值，不是「没有来源」。
     *
     * 默认值必须非空：空的话查询会静默返回「一个对话都没有」，而那与「真的没有对话」不可辨。
     */
    @Test
    fun anEmptySourceFallsBackToAVisibleDefault() {
        assertTrue("默认值不许是空串", DEFAULT_SOURCE_PATH.isNotEmpty())
        assertEquals("留空 ⇒ 回退到默认", DEFAULT_SOURCE_PATH, (null as String?) ?: DEFAULT_SOURCE_PATH)
    }

    /**
     * 默认来源不许是会起会话的启动器。
     *
     * 这个值一路传成 `daemonPath`，而 `DaemonProbe.probe()` 发的是裸命令
     * （`DaemonCommands.stream` 在 capabilities 为空时不带任何 flag）。`ccm` 是后端的 bash 启动器：
     * 裸跑它 = `action=new`，每次总览面探测都可能在用户机器上多起一条 Claude 会话。
     * 这不是「读不到」，是有副作用；上面那条只量「非空」的断言对它恒绿。
     *
     * 判别力边界：它是一份黑名单，只挡已知的三个启动器名；后端换个启动器名字，它照样绿。
     * 挡「换个没人看过的名字悄悄溜过去」的是下面 [theDefaultSourceIsPinnedSoAnyChangeGetsReVetted]。
     */
    @Test
    fun theDefaultSourceIsNotASessionStartingLauncher() {
        // ccm / cc / cc-iso 都是后端侧的启动器，裸跑会起会话
        val launchers = setOf("ccm", "cc", "cc-iso")
        assertTrue(
            "默认来源 '$DEFAULT_SOURCE_PATH' 是启动器不是 daemon —— " +
                "裸跑它会在用户机器上起一条新会话（DaemonProbe.probe 发的就是裸命令）",
            DEFAULT_SOURCE_PATH.substringAfterLast('/') !in launchers,
        )
    }

    /**
     * 这个值一变，就必须有人重新过一遍「它裸跑会不会起会话」。
     *
     * 上面那条黑名单只挡三个已知名字，换个名字就漏。这条把值本身钉死：它不证明这个值是安全的
     * （那要真跑），它证明的是「这个值没有在没人注意的时候变过」。改动者必须停下来，走一遍下面那份清单。
     *
     * 不走「让探测带一个 flag」那条路：`DaemonProbe.kt` 的裸命令是刻意的，发对端不认识的 flag ⇒
     * daemon 落进一次性查询模式 ⇒ exit 2 ⇒ 永远不发 hello ⇒ 客户端无限重连且日志里什么都没有。
     * 那条性质由 `DaemonProbeTest.probeAlwaysUsesBareCommandBecauseCapabilitiesAreNotKnownYet` 钉着。
     *
     * 判别力边界：
     * - 它分不出「换了个安全的名字」和「换了个危险的名字」，两种都红；真阳率取决于改动者看不看清单。
     * - 它对后端二进制改名恒哑：红的时机是我们自己改这个常量。运行期吸收改名的是
     *   `DaemonLocator.candidates` 那张有序候选表（判据在 `DaemonLocatorTest.theNewNameWinsWhenBothAreInstalled`）。
     *   所以它不许被引作「改名不会打到我们」的证据。
     *
     * 这个常量指向 `DaemonLocator.UNSET_PLACEHOLDER`，好让 `core-claude` 那侧认得出「这是占位名不是用户的选择」。
     */
    @Test
    fun theDefaultSourceIsPinnedSoAnyChangeGetsReVetted() {
        assertEquals(
            "改 DEFAULT_SOURCE_PATH 之前先过这四条，过完了再把新值抄到这里：\n" +
                "  ① 它是 daemon 本体还是一个 bash 启动器？（启动器裸跑 = 起一条新会话）\n" +
                "  ② 在真机上裸跑一次这个名字，会话数有没有多一条？\n" +
                "     ——`DaemonProbe.probe()` 发的就是裸命令，且那是刻意的\n" +
                "  ③ 上面那条黑名单要不要一起加上新的启动器名？\n" +
                "  ④ 这次改动和后端二进制改名（cc-monitor-backend）是什么关系？\n" +
                "     ⇒ 若是为了兼容改名，改错地方了：那由 DaemonLocator.candidates 那张有序候选表吸收，\n" +
                "       而不是由这个常量吸收。这个值只是「占位名 + 一个『用户没填』的信号」。",
            "cc-monitor-remote",
            DEFAULT_SOURCE_PATH,
        )
    }

    /**
     * 设置屏那两个测试标签必须互不相同且非空。
     *
     * 撞了的话 UI 测试会定位到错的控件，而它照样绿，那种测试比没有更糟。
     */
    @Test
    fun theTestTagsAreDistinct() {
        assertNotEquals(TAG_SOURCE_FIELD, TAG_PERMISSION_MODE)
        assertTrue(TAG_SOURCE_FIELD.isNotEmpty() && TAG_PERMISSION_MODE.isNotEmpty())
    }

    /**
     * 设置屏上没有「新界面（预览）」这一节，但「对话列表的来源」那一栏在。
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
     * | 「来源」那一栏被误删 | 红（后三条） |
     * | 「来源」那一栏在源码里但画不出来 / 滚不到 | 抓不到，要真机 |
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
        // 「对话列表的来源」不是预览的一部分，是来源配置，它必须在
        assertTrue("「$SOURCE_LABEL」那一栏是来源配置、不是预览的一部分，必须在", code.contains("\"$SOURCE_LABEL\""))
        assertTrue("那一栏得真能存下去（保存动作还在）", code.contains("saveOverviewSourcePath"))
        assertTrue("那一栏的测试标签在", code.contains("TAG_SOURCE_FIELD"))
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
        private const val SOURCE_LABEL = "对话列表的来源"
    }
}
