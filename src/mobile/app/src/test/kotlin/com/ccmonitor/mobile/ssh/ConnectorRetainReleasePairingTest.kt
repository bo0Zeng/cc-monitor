package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 凡是经网关 `retain` 的生产文件，必须也经网关 `release`。
 *
 * `SshConnectionManager.retain` / `release` 是引用计数：`retain` 往 `holders` 里 `add`，
 * `release` 先 `remove`，集合空了才真 `disconnectLocked`。
 * ⇒ 一个永不 `remove` 的 token 会让那台主机的持有者集永远非空
 * ⇒ 那条连接此后再也不会被引用计数断掉（只剩 force-disconnect 一条路能收它）。
 *
 * 「谁 retain 了、谁放手了」是头注记不住的事实，所以它必须是判据。
 *
 * ### 两头断，别只断「零命中」
 *
 * 扫描面写错路径会让判据恒绿。所以除了断「不该有的命中为空」，还断：
 * ① 真扫到了 `retain` 调用点（否则 needle 写错 ⇒ 空集减空集恒等于空集）；
 * ② 真扫到了 `release` 调用点（否则 release 那个 needle 写错 ⇒ 全员不配对、红在错误的理由上）；
 * ③ 靶子还在：`HostLink` 仍在 retain（本条判据就是为它写的）；
 * ④ 分辨得出来：一个真配对的文件（`SessionViewModel`）必须同时出现在两头
 *    —— 否则本条就是「全都算不配对」的全红，不是能分辨。
 *
 * ### 判别力边界
 *
 * | 绕过形态 | 本条 |
 * |---|---|
 * | 某个文件只 `retain` 不 `release` | 真红并点名文件 |
 * | 把 `release` 写进一个永远走不到的分支 | 抓不到 —— 本条只答「这个文件里两个词都出现过」，不答「每条路径都配对」 |
 * | `release` 的时机错了（在屏上就放、重试时误放） | 抓不到 —— 那是 Compose 行为。本仓 `app` 没引 Robolectric、门禁只跑 `testDebugUnitTest`，Compose 行为住 `androidTest` 而门禁不跑它 |
 * | `release` 传的 token 与 `retain` 的不是同一个串 | 抓不到 —— 源码扫描只认调用名，不认实参 |
 * | 持有被另一个类放手（`HostLink` 的 `false` 路：token 与 `ChatController.sessionFor` 铸的是同一个串，由 `ChatController.release` 覆盖） | 抓不到 —— 跨文件配对不在本条射程里，那由 `ChatController` 自己的判据守 |
 * | 接收者不叫 `*onnector`（如 `ChatController` 的 `connections?.retain(`） | 看不见 —— 那是 `ConnectionHolder` 端口不是网关。前提「凡是叫得出 `HostConnector` 的生产文件都把接收者命名成 `*onnector`」由 `HostConnectorGatewayTest.everyGatewayFunctionHasPinnedProductionCallers` 末尾那条自检守着 |
 * | 反射 / 把调用名拼在字符串里 | 看不见 —— 本文件走 [KotlinSourceScanner.codeOnlyDroppingLiterals]，字面整段删掉 |
 *
 * ### 为什么住 `app`
 *
 * 同 `HostConnectorGatewayTest`：全仓源码扫描类判据一律住 `app`
 * —— `:app:testDebugUnitTest` 的运行时 classpath 含全部 core 模块产物，任何模块改动都会让它重跑。
 */
class ConnectorRetainReleasePairingTest {
    /**
     * retain 与 release 在生产文件这一层必须成对。
     *
     * 四头断的理由见类头注「两头断」那一节。
     */
    @Test
    fun everyProductionFileThatRetainsThroughTheGatewayAlsoReleases() {
        val code = productionCode()
        val retainers = code.filterValues { RETAIN_RE.containsMatchIn(it) }.keys.toSortedSet()
        val releasers = code.filterValues { RELEASE_RE.containsMatchIn(it) }.keys.toSortedSet()

        assertTrue(
            "全仓一个 `*onnector.retain(` 都没扫到 —— 扫描面或 needle 写错了，本条会恒绿。",
            retainers.isNotEmpty(),
        )
        assertTrue(
            "全仓一个 `*onnector.release(` 都没扫到 —— release 那个 needle 写错了，" +
                "那样下面那条会把所有 retain 方一起打红，红在错误的理由上。",
            releasers.isNotEmpty(),
        )
        assertTrue(
            "靶子没了：`$HOST_LINK` 不再 retain —— 本条判据正是为它写的。" +
                "真要去掉那处 retain，请连本条一起重新想（或换一个靶子），别让它变成没有靶子的恒绿。",
            HOST_LINK in retainers,
        )
        assertTrue(
            "分辨不出来了：`$SESSION_VM` 是真配对的那一侧，" +
                "它必须同时出现在两头 —— 否则本条就是「全都算不配对」的全红，而不是能分辨。",
            SESSION_VM in retainers && SESSION_VM in releasers,
        )

        assertEquals(
            "有生产文件经 `HostConnector` 只 retain 不 release。\n" +
                "  `SshConnectionManager` 的持有者集是引用计数：永不 remove 的 token ⇒ 那台主机的持有者集永远非空\n" +
                "  ⇒ 连接此后再也不会被引用计数断掉（只剩 force-disconnect 能收）。\n" +
                "  Compose 里的形状是 `DisposableEffect(…) { retain(); onDispose { release() } }`；\n" +
                "  ViewModel 里是 `onCleared`。\n" +
                "  真的由别的更长命的东西负责放手（如 `ChatController` 拿同一个 token），\n" +
                "  那就把那条路写进该文件的头注并在这里登记成例外 —— 别只改判据。",
            emptySet<String>(),
            retainers.minus(releasers).toSet(),
        )
    }

    /**
     * 本条自己的探测器自检 —— 没有它，上面那条就可能是「零命中 = 守住了」。
     *
     * 正反都断：只 retain 的样本必须被认出来；成对的样本必须不被认成；
     * 而只在注释/字符串里提过那两个调用名的样本两头都不算（证明共享扫描器真的在链路上）。
     */
    @Test
    fun theDetectorFlagsRetainOnlyAndNotAPairedFile() {
        val retainOnly = "fun a() { connector.retain(h, t) }"
        val paired = "fun a() { hostConnector.retain(h, t) }\nfun b() { hostConnector.release(h, t) }"
        val onlyInProse =
            "// connector.retain( 写在行注释里不算\n" +
                "val s = \"connector.retain( 写在字符串里也不算\"\n" +
                "fun c() = 1\n"

        assertTrue("认不出「只 retain」的样本 ⇒ 上面那条是恒绿的零命中", isUnpaired(retainOnly))
        assertTrue("成对的样本被认成了不配对 ⇒ 那是全红，不是能分辨", !isUnpaired(paired))
        assertTrue(
            "注释/字符串里的调用名被当成了真调用 —— 共享扫描器没在链路上（或被绕过了）",
            !RETAIN_RE.containsMatchIn(codeOnly(onlyInProse)),
        )
        assertTrue("剥完之后真代码得还在（扫描器把代码吃掉了）", codeOnly(onlyInProse).contains("fun c()"))
    }

    /** 样本里「只 retain 不 release」成不成立 —— 与上面那条判据同一套 needle。 */
    private fun isUnpaired(sample: String): Boolean {
        val stripped = codeOnly(sample)
        return RETAIN_RE.containsMatchIn(stripped) && !RELEASE_RE.containsMatchIn(stripped)
    }

    /**
     * 剥注释并删掉字符串字面，走全仓唯一一份词法扫描器。
     *
     * 选「删字面」那一支的理由同 `HostConnectorGatewayTest`：本文件问的是
     * 「代码里有没有这个调用」，字面里提一嘴不是调用。代价（反射/按名拼接一概看不见）写在类头注里。
     */
    private fun codeOnly(text: String): String = KotlinSourceScanner.codeOnlyDroppingLiterals(text)

    private fun repoRoot(): File =
        generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }
            .take(MAX_WALK_UP)
            .firstOrNull { File(it, "settings.gradle.kts").isFile }
            ?: error("找不到仓根（往上 $MAX_WALK_UP 层都没有 settings.gradle.kts）：${File(".").absolutePath}")

    /** 全部生产源（`app` + `core-*` 的 `src/main/kotlin`），值是剥过注释与字符串的代码。 */
    private fun productionCode(): Map<String, String> {
        val root = repoRoot()
        val roots =
            (listOf(File(root, "app")) + (root.listFiles()?.filter { it.isDirectory && it.name.startsWith("core-") } ?: emptyList()))
                .map { File(it, "src/main/kotlin") }
                .filter { it.isDirectory }
                .sortedBy { it.path }
        assertTrue("前提：得找到扫描面（app + core-*），实得 ${roots.size} 个源根", roots.size >= 2)
        val files = roots.flatMap { r -> r.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList() }
        assertTrue("前提：得真扫到源文件，实得 ${files.size} 个", files.size > MIN_PRODUCTION_FILES)
        return files.associate { it.relativeTo(root).invariantSeparatorsPath to codeOnly(it.readText()) }
    }

    companion object {
        private const val MAX_WALK_UP = 6
        private const val MIN_PRODUCTION_FILES = 150

        private const val HOST_LINK = "app/src/main/kotlin/com/ccmonitor/mobile/ui/common/HostLink.kt"
        private const val SESSION_VM = "app/src/main/kotlin/com/ccmonitor/mobile/ui/session/SessionViewModel.kt"

        /**
         * 经 `HostConnector` 的 `retain` / `release` —— 按接收者名认（`connector.` / `hostConnector.`），
         * 与 `HostConnectorGatewayTest.CONNECTOR_CALL_RE` 同一族。
         * 其前提（叫得出 `HostConnector` 的文件都这么命名接收者）由那个文件末尾的自检守着。
         */
        private val RETAIN_RE = Regex("""\w*[Cc]onnector\.retain\(""")
        private val RELEASE_RE = Regex("""\w*[Cc]onnector\.release\(""")
    }
}
