package com.ccmonitor.mobile.core.claude.transport

import com.squareup.moshi.Moshi
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * [DaemonTransport] 吃真录制的 daemon wire（`bridge/vectors/daemon-wire.ndjson`，逐字节录自真 daemon）。
 *
 * [DaemonTransportTest] 的 JSON 是手写的，覆盖边界与坏帧；手写的跟着对 wire 的理解走，理解错了照样绿。
 * 这一组保证「理解的 wire == daemon 真发的 wire」。
 *
 * fixture 录了两版 daemon：
 * - `p1t-removal-cause`：`hello` 有 `capabilities:['bg','tail-only']` + `emits`（8 种）
 * - `p1h-bg-badge`：两者全无
 *
 * 缺 `capabilities` ⇒ 空集 ⇒ 一个 flag 都不发。给它加个「合理默认」的话，就会对不认识该 flag 的 daemon
 * 发 flag → 落进一次性查询模式 → `exit 2` → 永远不发 hello → 重连死循环。
 */
class DaemonWireGoldenTest {
    private val moshi = Moshi.Builder().build()
    private val anyAdapter = moshi.adapter(Any::class.java)

    /** 录制样本里的一行。`ver` 区分两版 daemon：p1t / p1h。 */
    private data class WireRow(
        val ver: String?,
        val cmd: String?,
        val stream: String?,
        val raw: String,
    )

    private fun rows(): List<WireRow> {
        // JVM 测试的 CWD 是模块目录（core-claude/），仓根在其上一级。
        val f =
            File("../bridge/vectors/daemon-wire.ndjson").takeIf { it.isFile }
                ?: File("bridge/vectors/daemon-wire.ndjson")
        assertTrue(
            "daemon wire golden 缺失：${f.absolutePath}。它必须随仓提交（只放在仓外的产物会丢）",
            f.isFile,
        )
        return f
            .readLines()
            .filter { it.isNotBlank() }
            .mapNotNull { line ->
                @Suppress("UNCHECKED_CAST")
                val m = anyAdapter.fromJson(line) as? Map<String, Any?> ?: return@mapNotNull null
                val raw = m["raw"] as? String ?: return@mapNotNull null // 跳过 __meta__ 与统计行
                WireRow(m["ver"] as? String, m["cmd"] as? String, m["stream"] as? String, raw)
            }
    }

    /**
     * `p1t` 的 `hello` 必须逐字段解析出来：「理解的 wire == daemon 真发的 wire」的直接证明。
     */
    @Test
    fun theP1tHelloParsesFieldByField() =
        runTest {
            val row = rows().first { it.ver == "p1t" && it.cmd == "(流模式首帧)" }
            val frames =
                DaemonTransport({ flowOf((row.raw + "\n").toByteArray()) }, streamCommand = "x").frames().toList()

            val hello = frames.filterIsInstance<JsonlFrame.Hello>().singleOrNull()
            assertNotNull("p1t 的首帧必须解析成 Hello（解析不出 = 重连死循环的起点）", hello)
            hello!!
            assertEquals("协议版本", 1, hello.v)
            assertEquals("build_id", "p1t-removal-cause", hello.buildId)
            assertTrue("host_arch 应有值", !hello.hostArch.isNullOrBlank())
            assertTrue("claude_dir 应有值", !hello.claudeDir.isNullOrBlank())
            // 能力协商的两个数据源
            assertEquals("capabilities 录制值", listOf("bg", "tail-only"), hello.capabilities)
            assertTrue(
                "emits 录到 8 种（line/session_added/session_status/session_removed/overflow/turn_end/tmux_sessions/tmux_session_closed）",
                hello.emits.size == 8,
            )
            assertTrue("emits 应含 line", hello.emits.contains("line"))
            assertTrue("emits 应含 turn_end", hello.emits.contains("turn_end"))
        }

    /**
     * 后端往 `hello` 里加字段时，解析不许因此死掉。
     *
     * 拿真录制的那帧 `hello`，在 `claude_dir` 与 `capabilities` 之间注入一个不认识的 `homes` 数组，
     * 要求：仍然解析成 `Hello`；已知字段一个都不许错位或丢失。
     *
     * 只证明「多一个未知字段不会打死解析」：不证明 `homes` 被理解了，
     * 也不覆盖「已有字段改了类型或语义」那类破坏。
     */
    @Test
    fun anUnknownFieldAddedByTheOtherSideDoesNotBreakOurHelloParsing() =
        runTest {
            val row = rows().first { it.ver == "p1t" && it.cmd == "(流模式首帧)" }
            // 插在 `claude_dir` 之后、`capabilities` 之前
            val injected =
                row.raw.replace(
                    "\"capabilities\"",
                    "\"homes\":[{\"agent_kind\":\"claudecode\",\"path\":\"/home/u/.claude\"}],\"capabilities\"",
                )
            assertTrue("前提：注入真的发生了，否则下面是空真断言", injected != row.raw)

            val frames =
                DaemonTransport({ flowOf((injected + "\n").toByteArray()) }, streamCommand = "x").frames().toList()
            val hello = frames.filterIsInstance<JsonlFrame.Hello>().singleOrNull()
            assertNotNull("多一个未知字段就解析不出 hello = 重连死循环的起点", hello)
            hello!!
            // 已知字段不许因为多了个邻居就错位/丢失
            assertEquals("协议版本", 1, hello.v)
            assertEquals("build_id", "p1t-removal-cause", hello.buildId)
            assertTrue("claude_dir 不许被挤掉", !hello.claudeDir.isNullOrBlank())
            assertEquals("capabilities 不许被挤掉", listOf("bg", "tail-only"), hello.capabilities)
            assertEquals("emits 不许被挤掉", 8, hello.emits.size)
        }

    /**
     * `hello` 缺 `capabilities`/`emits` 时必须降级为空集：不是默认值、不是崩。
     *
     * `p1h-bg-badge` 的 hello 只有 `{kind, v, build_id, host_arch, claude_dir}`。
     * 给 `capabilities` 加个「合理默认」（如 `?: listOf("bg")`）的话，就会对这种 daemon 发它不认识的 flag
     * → daemon 剥不掉 → 剩余参数非空 ⇒ 落进一次性查询模式 ⇒ `unknown argument` ⇒ exit 2、永远不发 hello
     * ⇒ 重连死循环。表现是「一直转圈重连，日志里什么都没有」。
     */
    @Test
    fun aHelloWithoutCapabilitiesDegradesToEmptySets() =
        runTest {
            val row = rows().first { it.ver == "p1h" && it.cmd == "(流模式首帧)" }
            val frames =
                DaemonTransport({ flowOf((row.raw + "\n").toByteArray()) }, streamCommand = "x").frames().toList()

            val hello = frames.filterIsInstance<JsonlFrame.Hello>().singleOrNull()
            assertNotNull("旧 daemon 的 hello 也必须能解析（不能因为缺字段就丢帧）", hello)
            hello!!
            assertEquals("build_id 仍应解析出来——它是判断对端版本的唯一依据", "p1h-bg-badge", hello.buildId)
            assertEquals("协议版本仍是 1", 1, hello.v)
            // 缺 = 空集，绝不是默认值
            assertEquals("缺 capabilities ⇒ 空集 ⇒ 一个 flag 都不发", emptyList<String>(), hello.capabilities)
            assertEquals("缺 emits ⇒ 空集", emptyList<String>(), hello.emits)
            // 其它可选字段同理（additive/skip_if_none）
            assertEquals("缺 codex_dir ⇒ null", null, hello.codexDir)
            assertEquals("缺 kinds ⇒ 空集", emptyList<String>(), hello.kinds)
        }

    /**
     * 两版的 `hello` 都解析得出 `build_id`：版本协商的唯一锚点。版本只能问 `hello.build_id`，不能靠路径猜。
     */
    @Test
    fun bothVersionsExposeBuildIdAsTheOnlyVersionAnchor() =
        runTest {
            val ids =
                rows()
                    .filter { it.cmd == "(流模式首帧)" }
                    .map { row ->
                        DaemonTransport({ flowOf((row.raw + "\n").toByteArray()) }, streamCommand = "x")
                            .frames()
                            .toList()
                            .filterIsInstance<JsonlFrame.Hello>()
                            .single()
                            .buildId
                    }
            assertEquals("fixture 应含新旧两版", 2, ids.size)
            assertTrue("每一版都必须自报 build_id", ids.all { !it.isNullOrBlank() })
            assertTrue("两版 build_id 必须不同（否则对照样本失去意义）", ids.toSet().size == 2)
        }

    /**
     * 一次性查询模式的输出不是 wire 帧，不能喂给 [DaemonTransport]。
     *
     * `--list-projects` 每行是 `{"dirName":…,"projectPath":…,"sessionCount":…}`，没有 `kind` 字段。
     * 喂进流解析器的结果是「未知 kind → 静默跳过」，一行都没有、也不报错。
     * 查询模式必须走独立的解析路径。
     */
    @Test
    fun oneShotQueryOutputIsNotWireFramesAndMustNotGoThroughStreamParser() =
        runTest {
            val queryRows = rows().filter { it.cmd == "--list-projects" }
            assertTrue("fixture 应含 --list-projects 样本", queryRows.isNotEmpty())

            val frames =
                DaemonTransport(
                    { flowOf((queryRows.joinToString("\n") { it.raw } + "\n").toByteArray()) },
                    streamCommand = "x",
                ).frames().toList()

            assertEquals(
                "查询输出没有 kind 字段 ⇒ 流解析器只会静默跳过（0 帧）。" +
                    "这正是危险之处：不报错、不崩、什么都没有。 查询模式必须走独立解析路径。",
                0,
                frames.size,
            )
        }

    /**
     * daemon 对不认识的子命令：stdout 空、错误走 stderr、exit=2。
     * 调用方按「stdout 有 JSON 才算成功」判定即可。
     *
     * 子命令名从那条 stderr 记录里读出来，不手抄；先断言它非空，否则 `none {}` 对空集恒真。
     */
    @Test
    fun theDaemonRejectsUnknownSubcommandOnStderrNotStdout() {
        val err = rows().firstOrNull { it.ver == "p1h" && it.stream == "stderr" }
        assertNotNull("fixture 应录到 p1h 的错误响应", err)
        assertTrue(
            "错误文本应指明是未知参数，实际：${err!!.raw}",
            err.raw.contains("unknown argument"),
        )
        val rejected = err.cmd
        assertTrue(
            "前提：那条错误记录必须说清是哪条子命令被拒的；说不清 ⇒ 下面那条对空集恒绿",
            !rejected.isNullOrBlank(),
        )
        assertTrue(
            "daemon 拒绝子命令（$rejected）时 stdout 必须为空——否则调用方会把错误文本当 JSON 解析",
            rows().none { it.ver == "p1h" && it.stream == "stdout" && it.cmd == rejected },
        )
    }
}
