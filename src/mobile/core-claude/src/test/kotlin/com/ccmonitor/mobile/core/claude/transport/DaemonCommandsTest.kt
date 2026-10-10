package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.lang.reflect.Method
import java.lang.reflect.Modifier

/**
 * [DaemonCommands]：命令串必须逐字符正确，每条断言背后都有一个具体的失败形态。
 *
 * 判据分两类，缺一不可：
 * 1. 逐字节定值（`EXPECTED_*`，手写在 companion 里，不从生产代码拼）：拼出来的话生产改形状时它跟着改，恒绿。
 * 2. 位置断言（`--` 必须是 `argv[1]`）：后端读的就是位置（`args.first() == "--"`）。把 `--` 挪到子命令后面，
 *    命令串里仍然有 `--`，任何 `contains("--")` 式的写法照样绿，而整行已经落到 claude 手里了。
 *
 * 注意：参数为空时这个二进制会起一个 claude，所以流模式必须显式发 `--stream`。
 */
class DaemonCommandsTest {
    private val path = "/home/u/.cc-monitor/bin/ccm"

    // ── 逐字节定值（手写，不从生产代码拼） ─────────────────────────────

    /**
     * 流模式命令逐字节等于 `<路径> -- --stream`。
     *
     * 都必须红的错法：
     * - 发裸路径 ⇒ 真起一个 claude；
     * - 丢掉 `--` 只发 `--stream` ⇒ `args.first()` 不是 `--` ⇒ 整行交给 claude。
     */
    @Test
    fun theStreamCommandIsByteForByteThePathTheGateAndTheExplicitStreamWord() {
        assertEquals(
            "流模式必须是「路径 + `--` + `--stream`」，裸路径的含义是「起一个 claude」",
            EXPECTED_STREAM,
            DaemonCommands.stream(path),
        )
    }

    /** 带 flag 的流：顺序逐字节钉死。 */
    @Test
    fun theStreamCommandWithFlagsIsByteForByteTheTemplate() {
        assertEquals(
            EXPECTED_STREAM_WITH_FLAGS,
            DaemonCommands.stream(path, listOf("bg", "tail-only"), listOf("bg", "tail-only")),
        )
    }

    // ── 不对应 capability token 的那个硬 flag ────────────────────

    /**
     * 硬 flag 绝不许被 `capabilities` 的交集吞掉。
     *
     * 后端的 `STREAM_CAPABILITIES` 是 `["bg", "tail-only"]`，里面没有 `with` 加 `-raw` 那个词；
     * 而 `STREAM_FLAGS` 里有它，且不对应能力 token（不认它的后端当未知旗标忽略、照常起流）。
     *
     * 把它放进 [DaemonCommands.FLAG_OF] 那张表 ＝ 进了 `if (token in want && token in capabilities)` 那一行
     * ＝ daemon 永远不声明它 ⇒ 它恒被静默丢掉，而拼出来的命令串看起来一切正常
     * （与 [AlphaUnavailable.NoTailOnly] 头注说的是同一个病）。
     *
     * 喂进去的 `capabilities` 是空集，一旦有人把那个词挪进交集，产出就会退化成 [EXPECTED_STREAM]，当场不等。
     */
    @Test
    fun theHardFlagIsNeverSwallowedByTheCapabilityIntersection() {
        val cmd =
            DaemonCommands.stream(
                path,
                capabilities = emptyList(),
                want = listOf(DaemonCommands.CAP_BG, DaemonCommands.CAP_TAIL_ONLY, DaemonCommands.WANT_RAW),
            )
        assertEquals(
            "能力 token 该被交集吃掉，而硬 flag 不对应任何 token ⇒ 必须照发",
            EXPECTED_STREAM_HARD_ONLY,
            cmd,
        )
    }

    /**
     * 反过来也要钉：加了硬 flag 这条路，不许把能力 token 的交集纪律弄松。
     *
     * 两件事必须同时成立：上一条，硬 flag 不受 `capabilities` 管；这条，能力 flag 仍然受 `capabilities` 管，
     * 且硬 flag 固定排在它们之后（后端分 flag 与位置无关，但判据要定值）。
     */
    @Test
    fun capabilityFlagsStayIntersectedAndTheHardFlagComesLast() {
        val cmd =
            DaemonCommands.stream(
                path,
                capabilities = listOf(DaemonCommands.CAP_BG, DaemonCommands.CAP_TAIL_ONLY),
                want = listOf(DaemonCommands.CAP_BG, DaemonCommands.CAP_TAIL_ONLY, DaemonCommands.WANT_RAW),
            )
        assertEquals(EXPECTED_STREAM_WITH_FLAGS_AND_HARD, cmd)
    }

    /**
     * 没人要它 ⇒ 一个字都不许多发。
     *
     * 硬 flag 走的是 `want`，不是「无条件追加」，否则探测那条裸流（[DaemonProbe] 的 `stream(path)`，
     * 它只读 `hello` 就被取消）也会带上它。与 `DaemonSessionSourceTest` 里那条生产面普查是一对：
     * 这条守「机制不乱发」，那条守「生产 `want` 里没有它」。
     */
    @Test
    fun theHardFlagIsOnlySentWhenActuallyAskedFor() {
        assertEquals("want 为空 ⇒ 产出就是只有门与 --stream 的那条", EXPECTED_STREAM, DaemonCommands.stream(path))
        assertEquals(
            "只要能力 token、没要硬 flag ⇒ 不许多出来",
            EXPECTED_STREAM_WITH_FLAGS,
            DaemonCommands.stream(
                path,
                capabilities = listOf(DaemonCommands.CAP_BG, DaemonCommands.CAP_TAIL_ONLY),
                want = listOf(DaemonCommands.CAP_BG, DaemonCommands.CAP_TAIL_ONLY),
            ),
        )
    }

    /**
     * `--` 与 `--stream` 的位置，不是「在不在」。
     *
     * 注意：不许直接比 `indexOf` 的大小：找不到时给 `-1`，`-1 < anything` 恒真。两侧都先护一道。
     */
    @Test
    fun inAStreamCommandTheGateIsArgv1AndTheStreamWordIsArgv2() {
        val words = DaemonCommands.stream(path, listOf("bg", "tail-only"), listOf("bg", "tail-only")).split(' ')
        val gate = words.indexOf(DaemonCommands.ARGV_END)
        val stream = words.indexOf(DaemonCommands.STREAM_FLAG)
        val bg = words.indexOf("--with-bg")
        assertTrue("前提：`--` 得真在命令里（找不到时 indexOf 给 -1，下面的比较会恒真）：$words", gate >= 0)
        assertTrue("前提：`--stream` 得真在命令里：$words", stream >= 0)
        assertTrue("前提：`--with-bg` 得真在命令里：$words", bg >= 0)
        assertEquals("`--` 必须紧跟路径：后端读的是 argv[1]：$words", 1, gate)
        assertEquals("`--stream` 必须紧跟 `--`：后端读的是 argv[2]：$words", 2, stream)
        assertTrue("能力 flag 必须排在 `--stream` 之后：$words", stream < bg)
    }

    /**
     * daemon 没声明的能力，一个 flag 都不发。
     *
     * `p1h-bg-badge` 的 `hello` 就没有 `capabilities`。给个「合理默认」的话就会发它不认识的 flag
     * ⇒ 剥不掉 ⇒ 落进一次性查询 ⇒ exit 2 ⇒ 永远不发 hello ⇒ 无限重连。
     * 「不发 flag」不等于「裸路径」：`-- --stream` 那两个词始终要发。
     */
    @Test
    fun capabilitiesAbsentMeansNoCapabilityFlagsButStillTheGateAndStreamWord() {
        val cmd =
            DaemonCommands.stream(
                path,
                capabilities = emptyList(), // hello 里没有这个字段
                want = listOf(DaemonCommands.CAP_BG, DaemonCommands.CAP_TAIL_ONLY),
            )
        assertEquals("缺 capabilities ⇒ 空集 ⇒ 不带能力 flag，但门与 --stream 照发", EXPECTED_STREAM, cmd)
        assertFalse("绝不能发 --with-bg", cmd.contains("--with-bg"))
        assertFalse("绝不能发 --tail-only", cmd.contains("--tail-only"))
    }

    /** 只发 want ∩ capabilities 的交集。 */
    @Test
    fun onlyIntersectionOfWantAndCapabilitiesBecomesFlags() {
        assertEquals(
            "$EXPECTED_STREAM --with-bg",
            DaemonCommands.stream(
                path,
                capabilities = listOf("bg", "tail-only"), // daemon 两个都支持
                want = listOf("bg"), // 但我们只要一个
            ),
        )
        assertEquals(
            "对端只支持 bg 时，要 tail-only 也不该发",
            EXPECTED_STREAM,
            DaemonCommands.stream(path, capabilities = listOf("bg"), want = listOf("tail-only")),
        )
    }

    /** flag 顺序固定，与集合迭代序无关：命令串要可测、可 diff。 */
    @Test
    fun flagOrderIsStableRegardlessOfInputOrder() {
        val a = DaemonCommands.stream(path, listOf("tail-only", "bg"), listOf("tail-only", "bg"))
        val b = DaemonCommands.stream(path, listOf("bg", "tail-only"), listOf("bg", "tail-only"))
        assertEquals(a, b)
        assertEquals(EXPECTED_STREAM_WITH_FLAGS, a)
    }

    /**
     * 每一条构造出来的命令都要过那道门，而 `--stream` 只许出现在流模式那一类：
     * 撒进一次性查询里的话，`--stream` 就不是要的那个 argv[2] 了。
     */
    @Test
    fun everyCommandGoesThroughTheGateAndOnlyTheStreamOneCarriesTheStreamWord() {
        val streams =
            listOf(
                DaemonCommands.stream(path),
                DaemonCommands.stream(path, listOf("bg", "tail-only"), listOf("bg", "tail-only")),
            )
        val queries =
            listOf(
                DaemonCommands.listProjects(path),
                DaemonCommands.query(path, "--list-sessions", "a dir"),
                DaemonCommands.query(path, "--read-session-from-offset", "/a/s.jsonl", "42"),
            )
        for (c in streams + queries) {
            assertTrue("命令没过 `--` 那道门 ⇒ 整行会被交给 claude：$c", c.startsWith("'$path' ${DaemonCommands.ARGV_END} "))
            assertFalse("`--` 不许被 quote（`'--'` 是一个普通参数，不是分派门）：$c", c.contains("'${DaemonCommands.ARGV_END}'"))
        }
        for (c in streams) assertTrue("流模式必须显式带 ${DaemonCommands.STREAM_FLAG}：$c", c.contains(DaemonCommands.STREAM_FLAG))
        for (c in queries) assertFalse("一次性查询里不许混进 ${DaemonCommands.STREAM_FLAG}：$c", c.contains(DaemonCommands.STREAM_FLAG))
    }

    /** 含空格 / 单引号的部署路径必须被正确引用，否则 word-split 后是完全另一条命令。 */
    @Test
    fun pathWithSpacesAndQuotesIsQuoted() {
        assertEquals("'/opt/my daemon/ccm' -- --stream", DaemonCommands.stream("/opt/my daemon/ccm"))
        assertEquals("""'/opt/it'\''s/ccm' -- --stream""", DaemonCommands.stream("""/opt/it's/ccm"""))
    }

    /**
     * 一次性查询逐字节等于 `<路径> -- <子命令> && printf 标记`。
     *
     * 每条查询恒带正向成功标记（`&& printf '\n<marker>\n'`）：`exec` 拿不到退出码，
     * 「空 stdout」既可能是「真的没有」也可能是「命令不存在」。
     * 连标记那一截一起逐字节比：「`--` 没了或挪了位」在「前缀相等 + contains 标记」下也能躲过去。
     */
    @Test
    fun aOneShotQueryIsByteForByteTheTemplateIncludingTheMarker() {
        assertEquals(EXPECTED_LIST_PROJECTS, DaemonCommands.listProjects(path))
    }

    /**
     * `--` 必须是第一个词（`argv[1]`），不是「串里有就行」。
     *
     * 把 `--` 挪到子命令后面之后，命令串里仍然有 `--`、仍然带着标记、仍然 quote 得好好的，
     * 任何 `contains("--")` 式的写法全绿，而后端 `args.first()` 读到的是 `--list-projects` ⇒ 整行交给 claude。
     * 用下标比对，不用 `indexOf` 比大小。
     */
    @Test
    fun theGateIsTheFirstWordAfterThePathNotJustSomewhereInTheString() {
        val words = DaemonCommands.listProjects(path).substringBefore(" &&").split(' ')
        assertTrue("前提：至少要有「路径 + 门 + 子命令」三个词：$words", words.size >= 3)
        assertEquals("argv[0] 必须是 quote 过的路径：$words", "'$path'", words[0])
        assertEquals("argv[1] 必须逐字是 `--`：$words", DaemonCommands.ARGV_END, words[1])
        assertEquals("argv[2] 必须是子命令（后端拿它判是不是自己的子命令）：$words", "--list-projects", words[2])
    }

    /** 查询参数值必须 quote（它们可能来自用户输入/路径），而门与子命令名不 quote。 */
    @Test
    fun queryArgumentsAreQuoted() {
        assertEquals(
            "'$path' -- --read-session '/a b/s.jsonl'",
            DaemonCommands.query(path, "--read-session", "/a b/s.jsonl").substringBefore(" &&"),
        )
    }

    // ── 普查（不是名单） ─────────────────────────────────────────────────

    /**
     * 每一支吐命令串的函数都必须过那道门：人群从声明来，不从手写名单来。
     *
     * [everyCommandGoesThroughTheGateAndOnlyTheStreamOneCarriesTheStreamWord] 手写枚举现有的几条命令；
     * 新加一支绕过 [DaemonCommands.query] 的命令构造，比如
     * ```
     * fun listSubagents(daemonPath: String): String = guarded(shellQuote(daemonPath) + " --list-subagents")
     * ```
     * 它照样全绿。而「新加一条一次性命令、忘了带 `--`」恰恰最可能发生，后果又特别坏：
     * 那条命令不会响亮地失败，它会默默去起一个 claude。
     *
     * 人群从 `DaemonCommands` 的声明现推：所有 public、非合成、返回 `String` 的成员函数，新加一支自动进人群。
     * 然后逐支真的调用一次，拿产出的串做位置断言，期望不从生产代码取形状。
     *
     * 三道前提自检：人群非空；人群里必须有已知的那三支（防签名一变就被过滤器吃掉）；扣掉豁免之后仍非空。
     * 另有一道反向自检：豁免表里不许有不存在的名字，否则「先登记豁免、再加同名函数」就能绕过。
     * 判别器本身会不会空转，由 [theCensusWouldCatchANewCommandThatBypassesTheGate] 两头证。
     */
    @Test
    fun everyCommandProducingFunctionGoesThroughTheArgvGate() {
        val population =
            DaemonCommands::class.java.declaredMethods
                .filter { !it.isSynthetic && !it.isBridge }
                .filter { Modifier.isPublic(it.modifiers) }
                .filter { it.returnType == String::class.java }
                .sortedBy { it.name }
        val names = population.map { it.name }

        assertTrue("前提①：反射一支都没扫到 ⇒ 普查对任何新命令都恒绿", population.isNotEmpty())
        assertTrue(
            "前提②：已知的三支必须都在人群里，少了说明过滤器把它吃掉了（实得 $names）",
            names.containsAll(listOf("stream", "listProjects", "query")),
        )
        assertTrue(
            "反向自检：豁免表里有不存在的名字：「先登记豁免再加同名函数」就能绕过普查。" +
                "多出来的：${GATE_EXEMPT.keys - names.toSet()}",
            names.containsAll(GATE_EXEMPT.keys),
        )

        val checked = population.filter { it.name !in GATE_EXEMPT }
        assertTrue("前提③：豁免表把人群吃光了 ⇒ 普查恒绿（人群 $names，豁免 ${GATE_EXEMPT.keys}）", checked.isNotEmpty())

        val violations =
            checked.mapNotNull { m ->
                gateViolation(m.name, m.invoke(DaemonCommands, *argsFor(m)) as String)
            }
        assertEquals(
            "有函数绕过了 `--` 那道门。要么让它走 DaemonCommands.query / stream，" +
                "要么在 GATE_EXEMPT 里连理由一起登记（天生不调那个二进制的才许豁免）。",
            emptyList<String>(),
            violations,
        )
    }

    /**
     * 判别器两头自检。
     *
     * 普查的判别力全压在 [gateViolation] 上：认不出「绕过门」的话，普查对任何新命令恒绿；
     * 把合规的命令也判成违规的话，普查会开始误红。两种都当场证不会；喂进去的是那条绕过门的构造的逐字产出形状。
     */
    @Test
    fun theCensusWouldCatchANewCommandThatBypassesTheGate() {
        // `guarded(shellQuote(daemonPath) + " --list-subagents")` 会产出这个串
        val bypassing = "'$path' --list-subagents && printf '\\n${DaemonCommands.QUERY_OK_MARKER}\\n'"
        assertNotNull(
            "判别器认不出「绕过门」⇒ 上面那条普查是空的：$bypassing",
            gateViolation("listSubagents", bypassing),
        )
        assertNull(
            "判别器把合规命令误判成违规 ⇒ 普查会开始误红",
            gateViolation("listProjects", DaemonCommands.listProjects(path)),
        )
        assertNull(
            "流模式（没有 `&&` 标记那一截）也不许被误判",
            gateViolation("stream", DaemonCommands.stream(path)),
        )
    }

    /**
     * 判一条命令串有没有过门；过了 ⇒ `null`，没过 ⇒ 说清是哪一支、串长什么样。
     *
     * 位置用下标比，不用 `indexOf` 比大小（找不到时 `indexOf` 给 -1，比较会恒真）。
     */
    private fun gateViolation(
        who: String,
        cmd: String,
    ): String? {
        val words = cmd.substringBefore(" &&").split(' ')
        if (words.size < 2) return "$who：命令串连「路径 + 门」两个词都不够 ⇒ $cmd"
        if (words[0] != "'$path'") return "$who：argv[0] 不是普查喂进去的那条路径（$path）⇒ $cmd"
        if (words[1] != DaemonCommands.ARGV_END) {
            return "$who：argv[1] 是 `${words[1]}` 不是 `${DaemonCommands.ARGV_END}` ⇒ " +
                "这条命令会被整行交给 claude（零参数时甚至直接起一个）⇒ $cmd"
        }
        return null
    }

    /**
     * 按参数类型给普查合成一组实参。
     *
     * 认不出的类型要响亮地炸，不许悄悄跳过那一支：
     * 「跳过」等于把一支没被检查的函数算成合格，正是本条要防的东西。
     */
    private fun argsFor(m: Method): Array<Any?> {
        var pathTaken = false
        return m.parameterTypes
            .map { t ->
                when {
                    t == String::class.java && !pathTaken -> {
                        pathTaken = true
                        path
                    }
                    t == String::class.java -> CENSUS_SUBCOMMAND
                    t == Array<String>::class.java -> emptyArray<String>()
                    Collection::class.java.isAssignableFrom(t) -> emptyList<String>()
                    t == Int::class.javaPrimitiveType -> 0
                    t == Boolean::class.javaPrimitiveType -> false
                    else -> error("普查不认识参数类型 $t（来自 ${m.name}）——加一条映射，别把这支悄悄跳过")
                }
            }.toTypedArray()
    }

    // ── 后端不提供 `--usage` ────────────────────────────────────────────────

    /**
     * `--usage` 在生产代码里零命中：后端没有这个子命令，拼出来只能拿到 exit 2。
     *
     * 扫源码而不是只看 `DaemonCommands`：别处直接 `query(path, "--usage")` 拼一条会绕过这个网关。
     * 扫的是剥掉注释之后的代码。自检在先：扫描器必须先证明自己扫得到一个活着的子命令，
     * 否则「零命中」可能只是因为它什么都没扫到。
     */
    @Test
    fun theUsageSubcommandHasZeroHitsInProductionCode() {
        val root = repoRoot()
        val sources = productionKotlinSources(root)
        assertTrue("前提：得真扫到源文件，否则本条恒绿（实得 ${sources.size} 个）", sources.size > 50)

        val alive = sources.filter { LIVE_SUBCOMMAND in codeOnly(it.readText()) }
        assertTrue(
            "扫描器自检：`$LIVE_SUBCOMMAND` 是后端有的子命令，扫不到它 ⇒ 扫描器瞎了 ⇒ 下面那条恒绿",
            alive.isNotEmpty(),
        )

        val hits = sources.filter { RETIRED_USAGE in codeOnly(it.readText()) }.map { it.relativeTo(root).invariantSeparatorsPath }
        assertEquals(
            "后端没有 `$RETIRED_USAGE` 这个子命令。" +
                "要接得先确认后端有这个子命令，且命令构造与读侧一起加。",
            emptyList<String>(),
            hits,
        )
    }

    /**
     * 查询输出的解析：只收 `{` 开头的行。
     *
     * daemon 对不认识的子命令是 stdout 空 / stderr 报错 / exit=2，
     * 但这里仍然守一道：万一有实现把错误文本写到 stdout，不能把它当 JSON 喂下去。
     */
    @Test
    fun parseQueryLinesKeepsOnlyJsonObjects() {
        val out =
            """
            {"dirName":"a","sessionCount":1}

            cc-monitor-remote query error: unknown argument: --usage
            {"dirName":"b","sessionCount":2}
            """.trimIndent()
        // 见到正向标记才认结果，所以这份样本要带上它才代表「查询成功了」
        val lines = DaemonCommands.parseQueryLines(out + "\n" + DaemonCommands.QUERY_OK_MARKER + "\n")!!
        assertEquals(2, lines.size)
        assertTrue(lines.all { it.startsWith("{") })
        assertTrue("错误文本必须被滤掉", lines.none { it.contains("unknown argument") })
        // 同一份输出没有标记时是「查询失败」，不是「查到了 2 条」。
        assertNull("失败要返回 null，不许交出半份结果", DaemonCommands.parseQueryLines(out))
    }

    /**
     * 构造出的流模式命令，喂真录制的 wire 能跑通：与 golden 对齐的一致性检查。
     *
     * 光测命令串是「我以为对」；这条确认命令形与 [DaemonTransport] 的期待是同一套。
     */
    @Test
    fun streamCommandShapeMatchesWhatDaemonTransportExpects() {
        // DaemonTransport 原样透传 streamCommand，不做任何加工，所以形状责任全在这里。
        val cmd = DaemonCommands.stream(path, listOf("bg"), listOf("bg"))
        assertTrue("必须以 quote 过的路径开头", cmd.startsWith("'$path'"))
        assertTrue(
            "门与 flag 之外不许有别的东西（多一个 token 就掉进一次性查询模式）",
            cmd
                .removePrefix("'$path'")
                .trim()
                .split(" ")
                .filter { it.isNotBlank() }
                .all { it.startsWith("--") },
        )
    }

    // ── 扫描器 ──────────────────────────────────────────────────────

    /**
     * 剥注释、留字面量，走词法扫描器 [KotlinSourceScanner.codeOnlyKeepingLiterals]。
     *
     * 注意：必须留字面。`--list-projects` 这类子命令是字面：连字面一起删的话，「活着的那一支必须扫得到」
     * 会假红，「`--usage` 零命中」会恒绿。扫描器看不见的情形写在 [KotlinSourceScanner] 的头注里。
     */
    private fun codeOnly(text: String): String = KotlinSourceScanner.codeOnlyKeepingLiterals(text)

    private fun repoRoot(): File =
        generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }
            .take(MAX_WALK_UP)
            .firstOrNull { File(it, "settings.gradle.kts").isFile }
            ?: error("找不到仓根（往上 $MAX_WALK_UP 层都没有 settings.gradle.kts）：${File(".").absolutePath}")

    private fun productionKotlinSources(root: File): List<File> {
        val roots =
            (listOf(File(root, "app")) + (root.listFiles()?.filter { it.isDirectory && it.name.startsWith("core-") } ?: emptyList()))
                .map { File(it, "src/main/kotlin") }
                .filter { it.isDirectory }
                .sortedBy { it.path }
        assertTrue("前提：得找到扫描面（app + core-*），实得 ${roots.size} 个源根", roots.size >= 2)
        return roots.flatMap { r -> r.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList() }
    }

    private companion object {
        private const val MAX_WALK_UP = 6

        /** 后端有的一条子命令：扫描器的自检靶子。 */
        private const val LIVE_SUBCOMMAND = "--list-projects"

        /** 普查给「还要一个子命令名」的函数（如 [DaemonCommands.query]）喂的占位值。 */
        private const val CENSUS_SUBCOMMAND = "--census-probe"

        /**
         * 普查的豁免表，逐条登记，每条带理由。
         *
         * 判准只有一句：这支函数产出的串，会不会被交给那个二进制去执行？
         * 会 ⇒ 必须过 `--` 那道门，不许豁免。不会（纯 shell、或者读的是输出）⇒ 没有 argv 分派这回事。
         *
         * 不许用「≥ 某个下界」的松写法代替这张表：新加一支绕过门的函数时它照样绿。
         * 表里的名字必须真的存在（普查有一条反向自检钉它）。
         */
        private val GATE_EXEMPT: Map<String, String> =
            mapOf(
                "guarded" to
                    "收到的是已经拼好的命令串，只负责在后面补正向标记；门该在传进来之前就带上了。它唯一的生产调用方 `query` 本身在普查里。",
                "presence" to
                    "纯 shell 存在性判定（`[ -x X ]` / `command -v -- X`），一个候选都不执行，没有 argv 分派这回事。",
                "locate" to
                    "把若干条 `presence` 用 `;` 串起来再加标记，同理：整条命令里一次都没有调用那个二进制。",
                "bodyOrNullIfFailed" to
                    "读输出的解析器（从 `guarded` 的产出里剥正文），根本不吐命令串。",
            )

        /** 后端没有的那一条。 */
        private const val RETIRED_USAGE = "--usage"

        /**
         * 流模式命令逐字节，手写在这里，不从生产代码拼：拼出来的话生产改形状时它跟着改，判据恒绿。
         */
        private const val EXPECTED_STREAM: String = "'/home/u/.cc-monitor/bin/ccm' -- --stream"

        /** 带 flag 的流，同样手写。 */
        private const val EXPECTED_STREAM_WITH_FLAGS: String = "'/home/u/.cc-monitor/bin/ccm' -- --stream --with-bg --tail-only"

        /**
         * daemon 一个能力 token 都不声明、而三个词都要时的逐字节产出。
         * 能力 flag 被交集吃掉，而那个硬 flag 必须还在。
         */
        private const val EXPECTED_STREAM_HARD_ONLY: String = "'/home/u/.cc-monitor/bin/ccm' -- --stream --with" + "-raw"

        /** 协商到两个能力 ＋ 又要了硬 flag 时的逐字节产出（硬 flag 固定排在能力 flag 之后）。 */
        private const val EXPECTED_STREAM_WITH_FLAGS_AND_HARD: String =
            "'/home/u/.cc-monitor/bin/ccm' -- --stream --with-bg --tail-only --with" + "-raw"

        /** 一次性查询逐字节，连 `guarded` 那一截一起。 */
        private const val EXPECTED_LIST_PROJECTS: String =
            "'/home/u/.cc-monitor/bin/ccm' -- --list-projects && printf '\\nATERM_Q_OK\\n'"
    }
}
