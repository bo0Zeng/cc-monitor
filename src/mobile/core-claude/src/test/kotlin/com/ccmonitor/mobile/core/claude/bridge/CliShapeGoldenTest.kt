package com.ccmonitor.mobile.core.claude.bridge

import com.squareup.moshi.Moshi
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * CLI 原生 stream-json 的形状 golden。
 *
 * `bridge/vectors/` 下的 `.ndjson` 里，原始行是 SDK 对象转储（`__type__=StreamEvent`），
 * 而常驻管道产出的是 CLI 的 stream-json（`{"type":"stream_event",…}`）。
 * `cli-text-short.cli.ndjson` 是同一个 prompt 的 CLI 录制，这里钉两件事：
 * 1. 序列逐条对应，所以 SDK 侧的映射可以照搬。
 * 2. 信封不同：SDK 用 `__type__` 类名分派，CLI 用 `type` + `subtype`，映射必须按 CLI 的键分派。
 *
 * 注意：这段注释里别写 `vectors/` 紧跟星号，Kotlin 块注释可嵌套，会报 `Unclosed comment`。
 */
class CliShapeGoldenTest {
    private val moshi = Moshi.Builder().build()
    private val anyAdapter = moshi.adapter(Any::class.java)

    private fun vectorsDir(): File =
        File("../bridge/vectors").takeIf { it.isDirectory } ?: File("bridge/vectors")

    private fun rows(name: String): List<Map<*, *>> =
        File(vectorsDir(), name)
            .readLines()
            .mapNotNull { anyAdapter.fromJson(it) as? Map<*, *> }
            .filterNot { it.containsKey("__meta__") }

    /** CLI 侧每行的「帧型 + 子型」。 */
    private fun cliKinds(): List<Pair<String, String?>> =
        rows("cli-text-short.cli.ndjson").map { row ->
            val line = row["line"] as Map<*, *>
            val type = line["type"] as? String ?: "?"
            val sub = line["subtype"] as? String ?: ((line["event"] as? Map<*, *>)?.get("type") as? String)
            type to sub
        }

    /** SDK 侧每行的「类名 + 子型」。 */
    private fun sdkKinds(): List<Pair<String, String?>> =
        rows("text-short.ndjson").map { row ->
            val msg = row["msg"] as Map<*, *>
            val type = msg["__type__"] as? String ?: "?"
            val sub = msg["subtype"] as? String ?: ((msg["event"] as? Map<*, *>)?.get("type") as? String)
            type to sub
        }

    /**
     * 两侧帧型逐个对得上。
     *
     * 判据是序列同构：条数一致、每一位的语义类别一致。
     * 两侧类名不同（那正是下面那条测试的内容），所以这里按语义映射比。
     */
    @Test
    fun theCliStreamMatchesTheSdkStreamFrameForFrame() {
        val cli = cliKinds()
        val sdk = sdkKinds()
        assertTrue("前提：两份 golden 都要读到", cli.isNotEmpty() && sdk.isNotEmpty())
        assertEquals("条数必须一致（同一个 prompt）", sdk.size, cli.size)

        // SDK 类名 → CLI type 的语义对照
        val expectedCliType =
            mapOf(
                "HookEventMessage" to "system",
                "SystemMessage" to "system",
                "StreamEvent" to "stream_event",
                "AssistantMessage" to "assistant",
                "UserMessage" to "user",
                "RateLimitEvent" to "rate_limit_event",
                "ResultMessage" to "result",
            )
        sdk.forEachIndexed { i, (sdkType, sdkSub) ->
            val (cliType, cliSub) = cli[i]
            assertEquals("第 $i 位的帧型对不上（sdk=$sdkType）", expectedCliType[sdkType], cliType)
            assertEquals("第 $i 位的子型对不上（sdk=$sdkType/$sdkSub）", sdkSub, cliSub)
        }
    }

    /**
     * 信封不同：CLI 侧一个 `__type__` 都没有，它用 `type` + `subtype`。
     * 照 SDK 形状写的分派拿到 CLI 输出上会一条都认不出来。
     */
    @Test
    fun theEnvelopeDiffersSoTheMappingMustDispatchOnCliKeys() {
        val cliLines = rows("cli-text-short.cli.ndjson").map { it["line"] as Map<*, *> }
        assertTrue("CLI 侧没有 __type__", cliLines.none { it.containsKey("__type__") })
        assertTrue("CLI 侧每行都有 type", cliLines.all { it["type"] is String })

        val sdkMsgs = rows("text-short.ndjson").map { it["msg"] as Map<*, *> }
        assertTrue("SDK 侧每行都有 __type__", sdkMsgs.all { it["__type__"] is String })
    }

    /**
     * `init` 帧的 catalog 在 CLI 侧也在：命令选择器消费的就是它。
     */
    @Test
    fun theCatalogSurvivesInTheCliShape() {
        val init =
            rows("cli-text-short.cli.ndjson")
                .map { it["line"] as Map<*, *> }
                .first { it["subtype"] == "init" }
        for (key in listOf("tools", "mcp_servers", "slash_commands")) {
            assertNotNull("catalog 的 `$key` 在 CLI 侧也必须有", init[key])
        }
        assertTrue("cwd 要在（分组/定位要用）", init["cwd"] is String)
        assertTrue("session_id 要在", init["session_id"] is String)
    }

    /** 脱敏：golden 里不许有真实用户名/家目录。 */
    @Test
    fun theGoldenIsRedacted() {
        val raw = File(vectorsDir(), "cli-text-short.cli.ndjson").readText()
        val home = System.getProperty("user.home").orEmpty()
        val user = home.substringAfterLast('/')
        assertTrue("不许残留真实家目录", home.isEmpty() || !raw.contains(home))
        assertTrue("不许残留真实用户名", user.isEmpty() || !raw.contains(Regex("\\b${Regex.escape(user)}\\b")))
    }
}
