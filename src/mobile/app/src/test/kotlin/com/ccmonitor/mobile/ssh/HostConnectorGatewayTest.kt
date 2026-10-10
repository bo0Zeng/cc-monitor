package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 连接的生与死都收进 `HostConnector`，别处只拿一个句柄。
 *
 * 「调用方有几个、在哪儿」是头注记不住的事实：`SshConnectionManager.connect` 多一个没人记得的调用方
 * （比如在主线程的 `LaunchedEffect` 里），失败路径的同步 `close()` 就成了主线程网络写，异常被外层
 * `runCatching` 吞掉，静默泄漏 socket、线程与远端会话。所以它必须是判据。
 *
 * ### 判别力边界
 *
 * | 绕过形态 | 本文件 |
 * |---|---|
 * | 网关之外新增 `manager.connect(` | 红 |
 * | 新文件注入/导入 `SshConnectionManager` | 红（不依赖它怎么命名接收者） |
 * | 网关新增/失联一支函数 | 红 |
 * | `app/…/di/` 里重新 `get<SshConnectionManager>()` | 红 |
 * | 网关的 `connection` / `disconnectAll` 被写成空壳（`= null` / `{}`） | 抓不到（源码扫描只答「谁写了哪个名字」），由 `HostConnectorLifecycleTest` 跑起来验 |
 * | 已可达 manager 的文件在自己文件里新加一行 `manager.disconnect(…)` | 抓不到：这里问的是「谁够得着」，不是「够着之后调了几支」 |
 * | 把 manager 的接收者改名（`manager` → `mgr`）后调 `connect` | 按接收者扫的那条会漏，但 [everyFileThatCanReachTheManagerBindsItAsManager] 会红 |
 * | 反射 / 字符串里拼名字 | 抓不到：扫描器把字符串字面整段删掉 |
 * | 调用点存在但走的是死分支 | 抓不到：源码扫描只答「有没有引用」 |
 *
 * 全仓源码扫描类判据一律住 `app`：`:app:testDebugUnitTest` 的运行时 classpath 含全部 core 模块的产物，
 * 任何模块的代码改动都会让它重跑。
 */
class HostConnectorGatewayTest {
    // ---- 生那一半 -------------------------------------------------------

    /**
     * `SshConnectionManager.connect` 在生产代码里只有网关一个调用方。
     *
     * 按接收者字面 `manager.connect(` 扫；其前提（「够得着 manager 的文件都把它叫 `manager`」）由
     * [everyFileThatCanReachTheManagerBindsItAsManager] 单独守着，不然这条会静默漏。
     */
    @Test
    fun theOnlyProductionCallerOfManagerConnectIsTheGateway() {
        val code = productionCode()
        val found = code.filterValues { it.contains("$MANAGER_RECEIVER.connect(") }.keys.toSortedSet()
        assertTrue(
            "全仓一个 `$MANAGER_RECEIVER.connect(` 都没扫到 —— 扫描器瞎了，这条判据会恒绿。",
            found.isNotEmpty(),
        )
        assertEquals(
            "`SshConnectionManager.connect` 的生产调用方变了。" +
                "建连必须经 `HostConnector`（它才知道认证/跳板/竞速/纪元守门怎么解析）；" +
                "多一个没人记得的调用方，就可能在主线程上泄漏连接。",
            setOf(GATEWAY),
            found.toSet(),
        )
    }

    // ---- 谁够得着这个 manager -------------------------------------------

    /**
     * 够得着 `SshConnectionManager` 的生产文件，一个不多一个不少。
     *
     * 这条比上一条硬：它不问「调了哪支」，问「谁拿得到那个句柄」，拿得到就能 connect、close、disconnect。
     * 表里的文件要的都不是「连接的生死与句柄」，而是通道工厂（`commandChannel` / `commandExecutor`）、
     * `sessions` / `activeCount` 状态流、或 `probeAndReconnectStale` 后台重连；逐条见 [MANAGER_REACHABLE]。
     *
     * 它拦得住「又多一个文件够得着」，拦不住「已经够得着的那些在自己文件里直调生死」。
     */
    @Test
    fun everyFileThatCanReachTheConnectionManagerIsPinned() {
        val code = productionCode()
        val found = code.filterValues { it.contains(MANAGER_TYPE) }.keys.toSortedSet()
        assertTrue("一个都没扫到 ⇒ 扫描器瞎了。", found.isNotEmpty())
        assertEquals(
            "够得着 `SshConnectionManager` 的生产文件变了。\n" +
                "  多出来的：它凭什么直接拿连接管理器？别处只拿一个句柄，请改经 `HostConnector`；" +
                "真拿不掉就把它加进这张表并在 `HostConnector` 的头注里登记成欠账（别只改表）。\n" +
                "  少掉的：好事，把它从表里删掉（那是一笔欠账被还了）。",
            MANAGER_REACHABLE,
            found.toSet(),
        )
    }

    /**
     * 按接收者字面扫的那条的前提自检：没有它，那条改个变量名就一个都扫不到、还全绿。
     *
     * 全仓凡是运行时够得着 manager 的文件，都必须把它绑成一个叫 `manager` 的标识符。
     * 不绑标识符的例外登记在 [MANAGER_REACHABLE_WITHOUT_RECEIVER] 里。
     */
    @Test
    fun everyFileThatCanReachTheManagerBindsItAsManager() {
        val code = productionCode()
        val reachable = code.filterValues { it.contains(MANAGER_TYPE) }.keys.toSortedSet()
        assertTrue("一个都没扫到 ⇒ 扫描器瞎了。", reachable.isNotEmpty())
        val withoutReceiver = reachable.filterNot { code.getValue(it).contains("$MANAGER_RECEIVER.") }.toSortedSet()
        assertEquals(
            "有文件够得着 `SshConnectionManager` 却没有把它叫 `manager`：按接收者字面扫的那条，" +
                "接收者一改名就静默漏（零命中 ≠ 守住了）。要么改回 `manager`，要么把这个文件连同理由加进" +
                "`MANAGER_REACHABLE_WITHOUT_RECEIVER` 并同时加强那条的扫法。",
            MANAGER_REACHABLE_WITHOUT_RECEIVER,
            withoutReceiver.toSet(),
        )
    }

    // ---- 网关自己 -------------------------------------------------------

    /**
     * 网关的函数一支不多一支不少（形状同 `TmuxGatewayCallSiteTest`）。
     *
     * 新长出一支就必须在 [PINNED_GATEWAY_CALLERS] 里表态一次，那一次表态就是「它有没有人用」被真正问过的证据。
     */
    @Test
    fun theGatewayDeclaresExactlyThePinnedFunctions() {
        val src = File(repoRoot(), GATEWAY)
        assertTrue("被守的网关源文件不在：${src.absolutePath}", src.isFile)
        val declared = FUN_DECL_RE.findAll(codeOnly(src.readText())).map { it.groupValues[1] }.toSortedSet()
        assertTrue("一支函数都没抠出来 ⇒ 抠取器瞎了、这条恒绿", declared.isNotEmpty())
        assertEquals(
            "网关的函数集变了。新增的那一支要写进 `PINNED_GATEWAY_CALLERS` 并点名谁在用；" +
                "接不上调用方的就别先加：零生产调用方的公开函数要么接上、要么删掉。",
            PINNED_GATEWAY_CALLERS.keys.toSortedSet(),
            declared,
        )
    }

    /**
     * 网关每一支的生产调用方住址定值钉，且非空。
     *
     * 非空那半是真有牙的：一支没人用的网关函数就是「已 build 零引用」。
     */
    @Test
    fun everyGatewayFunctionHasPinnedProductionCallers() {
        val code = productionCode().filterKeys { it != GATEWAY }
        val byFn = mutableMapOf<String, MutableSet<String>>()
        for ((path, text) in code) {
            for (m in CONNECTOR_CALL_RE.findAll(text)) {
                byFn.getOrPut(m.groupValues[1]) { sortedSetOf() }.add(path)
            }
        }
        for ((fn, pinned) in PINNED_GATEWAY_CALLERS) {
            val found = byFn[fn].orEmpty().toSortedSet()
            assertTrue(
                "`HostConnector.$fn` 零生产调用方：网关函数要么接上、要么删掉。",
                found.isNotEmpty(),
            )
            assertEquals("`HostConnector.$fn` 的生产调用方住址变了。", pinned, found.toSet())
        }
        // 反面：扫出来的函数名不许多出一支没表态的（接收者规则只认 `*onnector.`，但函数名是开放的）。
        assertEquals(
            "有人经 `HostConnector` 调了一支没在 `PINNED_GATEWAY_CALLERS` 里表态的函数。",
            PINNED_GATEWAY_CALLERS.keys.toSortedSet(),
            byFn.keys.toSortedSet(),
        )
        // 前提自检：凡是叫得出 `HostConnector` 的生产文件（网关自己除外），都得真的扫到至少一次调用 ——
        //   否则就是「接收者改名了 ⇒ 这条静默漏」。
        val namesGateway = code.filterValues { it.contains(GATEWAY_TYPE) }.keys.toSortedSet()
        val calledSomething = byFn.values.flatten().toSortedSet()
        assertEquals(
            "有文件叫得出 `HostConnector` 却一次调用都没扫到 —— 接收者多半改名了（本判据按 `*onnector.` 扫）。",
            namesGateway,
            calledSomething,
        )
    }

    // ---- DI 那句「单一入口」是判据 --------------------------------

    /**
     * `app/…/di/` 这一层只叫得出网关，叫不出连接管理器。
     *
     * 网关头注写着「单一入口」；DI 这头若自己 `get<SshConnectionManager>()` 再转手，一条断言都不会红。
     *
     * 两头都断（零命中 + 必须有命中）：只断「零命中」的话，扫描面写错路径（目录名改了、源根挪了）会让它恒绿。
     */
    @Test
    fun diLayerNamesOnlyTheGatewayNotTheManager() {
        val code = productionCode().filterKeys { it.startsWith(DI_DIR) }
        assertTrue("`$DI_DIR` 下一个源文件都没扫到 ⇒ 扫描面写错了，这条会恒绿。", code.isNotEmpty())
        assertEquals(
            "DI 这一层又自己去拿了一份 `SshConnectionManager`。连接的生与死都经 `HostConnector`，" +
                "DI 只许把 manager 交给网关（`single { HostConnector(get(), …) }` 那种按类型推断的 `get()`），" +
                "不许用名字自己要一份转手给别人。",
            emptySet<String>(),
            code
                .filterValues { it.contains(MANAGER_TYPE) }
                .keys
                .toSortedSet()
                .toSet(),
        )
        assertTrue(
            "DI 这一层一次都没提 `HostConnector` —— 那上面那条「零命中」就什么都没证明。",
            code.values.any { it.contains(GATEWAY_TYPE) },
        )
    }

    // ---- 扫描器自检 -----------------------------------------------------------

    /**
     * 扫描器自检：上面全部判据的判别力都押在 [codeOnly] 上。
     *
     * 两个坑：
     * ① 正则版剥注释会被形如「引号-星-斜杠-星-引号」的字符串字面骗到（`SftpScreen.kt` 上传用的
     *    MIME 通配串就是这个形状），把随后的代码当注释吃掉。
     * ② Kotlin 的块注释是嵌套的：KDoc 里写一个 `/` 紧跟 ``（想表达某个目录通配）会真的开一层嵌套注释，
     *    编译器报 `Unclosed comment`。
     */
    @Test
    fun theCommentStripperSurvivesSlashStarInsideStringLiterals() {
        val sample =
            """
            val a = launch("*/*")
            val b = manager.connect(x)
            /* 注释里的 manager.connect( 不算 */
            // 行注释里的 manager.connect( 也不算
            val c = "字符串里的 manager.connect( 同样不算"
            """.trimIndent()
        val got = codeOnly(sample)
        assertEquals("`manager.connect(` 只该剩一处（注释/行注释/字符串里的三处都该没了）", 1, got.windowed(16).count { it == "manager.connect(" })
        assertTrue("`\"*/*\"` 之后的代码被当注释吃掉了：正则版 codeOnly 就是这个坑", got.contains("val b"))
    }

    /** 抠取器自检：网关函数集那条的判别力全押在 [FUN_DECL_RE] 上。 */
    @Test
    fun theFunctionExtractorFindsDeclarationsAndOnlyDeclarations() {
        val sample =
            "class X {\n" +
                "    suspend fun realOne(a: String): String = a\n" +
                "    private fun hiddenOne(): Int = 1\n" +
                "    val notAFun = listOf(1).map { fun2(it) }\n" +
                "}\n"
        val got = FUN_DECL_RE.findAll(codeOnly(sample)).map { it.groupValues[1] }.toSortedSet()
        assertEquals("只该认出两支真声明：$got", sortedSetOf("hiddenOne", "realOne"), got)
    }

    // ---- 扫描器 ---------------------------------------------------------------

    /**
     * 剥注释并删掉字符串字面，走全仓唯一一份词法扫描器 [KotlinSourceScanner.codeOnlyDroppingLiterals]。
     *
     * 选「删字面」：本文件问的是「代码里有没有这个调用」，字面里提一嘴不是调用
     * （[theCommentStripperSurvivesSlashStarInsideStringLiterals] 钉着「字符串里的 `manager.connect(` 不算」）。
     * 代价：藏在字符串里的东西（反射、按名拼接）一概看不见；完整的「看不见什么」在 [KotlinSourceScanner] 头注里。
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
        assertTrue("前提：得真扫到源文件，实得 ${files.size} 个", files.size > 150)
        return files.associate { it.relativeTo(root).invariantSeparatorsPath to codeOnly(it.readText()) }
    }

    companion object {
        private const val MAX_WALK_UP = 6

        private const val MANAGER_TYPE = "SshConnectionManager"
        private const val GATEWAY_TYPE = "HostConnector"

        /** 按接收者字面扫；其前提由 [everyFileThatCanReachTheManagerBindsItAsManager] 守着。 */
        private const val MANAGER_RECEIVER = "manager"

        private const val GATEWAY = "app/src/main/kotlin/com/ccmonitor/mobile/ssh/HostConnector.kt"
        private const val DI_DIR = "app/src/main/kotlin/com/ccmonitor/mobile/di/"

        private const val APP_MODULE = "app/src/main/kotlin/com/ccmonitor/mobile/di/AppModule.kt"
        private const val SESSION_VM = "app/src/main/kotlin/com/ccmonitor/mobile/ui/session/SessionViewModel.kt"
        private const val SFTP_VM = "app/src/main/kotlin/com/ccmonitor/mobile/ui/sftp/SftpViewModel.kt"
        private const val HOST_LINK = "app/src/main/kotlin/com/ccmonitor/mobile/ui/common/HostLink.kt"
        private const val ATTACHMENT_PICKER = "app/src/main/kotlin/com/ccmonitor/mobile/ui/chat/AttachmentPicker.kt"
        private const val KEEP_ALIVE = "app/src/main/kotlin/com/ccmonitor/mobile/service/SshKeepAliveService.kt"

        /** 顶层/成员函数声明（含 `private` / `suspend`）。不认调用 —— `fun` 后必须紧跟名字与 `(`。 */
        private val FUN_DECL_RE = Regex("""(?m)^\s*(?:private\s+|internal\s+)?(?:suspend\s+)?fun\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(""")

        /**
         * 经 `HostConnector` 的调用点 —— 按接收者名认（`connector.` / `hostConnector.`）。
         * 前提「凡是叫得出 `HostConnector` 的文件都这么命名接收者」由 [everyGatewayFunctionHasPinnedProductionCallers]
         * 末尾那条自检守着。
         */
        private val CONNECTOR_CALL_RE = Regex("""\w*[Cc]onnector\.([A-Za-z_][A-Za-z0-9_]*)\(""")

        /**
         * 定值钉：网关每一支函数今天的生产调用方。
         *
         * | 函数 | 谁在用 |
         * |---|---|
         * | `connect` | `SessionViewModel` · `SftpViewModel` · `HostLink` |
         * | `retain` | `AppModule`（`ChatController` 的 `ConnectionHolder` 端口）+ 三个屏 |
         * | `release` | `AppModule`（同上）+ 两个 VM + `HostLink` |
         * | `connection` | `SessionViewModel` · `SftpViewModel` · `HostLink` · `AttachmentPicker` |
         * | `disconnectAll` | `SshKeepAliveService.onTaskRemoved` |
         *
         * 这里只问「住址集变了没」；「凡调 retain 的生产文件必须也调 release」由
         * `ConnectorRetainReleasePairingTest` 管。网关没有 `disconnect(id)` 那一支：全仓零生产调用方。
         */
        private val PINNED_GATEWAY_CALLERS =
            linkedMapOf(
                "connect" to setOf(SESSION_VM, SFTP_VM, HOST_LINK),
                "retain" to setOf(APP_MODULE, SESSION_VM, SFTP_VM, HOST_LINK),
                "release" to setOf(APP_MODULE, SESSION_VM, SFTP_VM, HOST_LINK),
                "connection" to setOf(SESSION_VM, SFTP_VM, HOST_LINK, ATTACHMENT_PICKER),
                "disconnectAll" to setOf(KEEP_ALIVE),
            )

        /**
         * 定值钉：今天够得着 `SshConnectionManager` 的全部生产文件。
         *
         * | 住址 | 它要的是什么 |
         * |---|---|
         * | `core-ssh/…/SshConnectionManager.kt` | 它自己 |
         * | `core-ssh/…/di/SshModule.kt` | 它的 Koin 定义处（`single { SshConnectionManager(get()) }`） |
         * | `core-ssh/…/CommandChannel.kt` | `commandChannel` / `commandExecutor` 两支扩展函数（接收者隐式，按 `manager.` 扫看不见它） |
         * | `app/…/ssh/HostConnector.kt` | 网关 |
         * | `app/…/AtermApp.kt` | `probeAndReconnectStale` 接线（前台跟踪 ⇒ 后台重连，不是建/断） |
         * | `app/…/service/SshKeepAliveService.kt` | `sessions` / `activeCount` / 通道工厂 / `probeAndReconnectStale` |
         * | `app/…/ui/session/SessionViewModel.kt` | 通道工厂 + `sessions` |
         * | `app/…/ui/chat/AttachmentPicker.kt` | `commandExecutor` |
         * | `app/…/ui/chat/ChatRoute.kt` · `app/…/ui/overview/ConversationsRoute.kt` | `commandChannel` 接线点 |
         * | `app/…/ui/sftp/SftpScreen.kt` | 只有一行 import，供 KDoc 链接用；代码里不碰 |
         *
         * 没有一条在拿「连接的生死与句柄」。
         */
        private val MANAGER_REACHABLE =
            setOf(
                "app/src/main/kotlin/com/ccmonitor/mobile/AtermApp.kt",
                KEEP_ALIVE,
                GATEWAY,
                ATTACHMENT_PICKER,
                "app/src/main/kotlin/com/ccmonitor/mobile/ui/chat/ChatRoute.kt",
                "app/src/main/kotlin/com/ccmonitor/mobile/ui/overview/ConversationsRoute.kt",
                SESSION_VM,
                "app/src/main/kotlin/com/ccmonitor/mobile/ui/sftp/SftpScreen.kt",
                "core-ssh/src/main/kotlin/com/ccmonitor/mobile/core/ssh/CommandChannel.kt",
                "core-ssh/src/main/kotlin/com/ccmonitor/mobile/core/ssh/SshConnectionManager.kt",
                "core-ssh/src/main/kotlin/com/ccmonitor/mobile/core/ssh/di/SshModule.kt",
            )

        /**
         * 够得着 manager 却不把它绑成名叫 `manager` 的标识符的例外。
         *
         * | 住址 | 为什么 |
         * |---|---|
         * | `SshConnectionManager.kt` | 它自己，没有接收者 |
         * | `di/SshModule.kt` | `single { SshConnectionManager(get()) }`，构造而非调用 |
         * | `CommandChannel.kt` | 两支扩展函数，接收者隐式（`connection(id)` 裸调），按 `manager.` 扫天然看不见。注意：这是个真盲区，谁都可以写一支 `fun SshConnectionManager.xxx()` 在里面裸调 `connect(…)` |
         * | `ui/sftp/SftpScreen.kt` | 只有一行 import 供 KDoc 链接用 |
         */
        private val MANAGER_REACHABLE_WITHOUT_RECEIVER =
            setOf(
                "app/src/main/kotlin/com/ccmonitor/mobile/ui/sftp/SftpScreen.kt",
                "core-ssh/src/main/kotlin/com/ccmonitor/mobile/core/ssh/CommandChannel.kt",
                "core-ssh/src/main/kotlin/com/ccmonitor/mobile/core/ssh/SshConnectionManager.kt",
                "core-ssh/src/main/kotlin/com/ccmonitor/mobile/core/ssh/di/SshModule.kt",
            )
    }
}
