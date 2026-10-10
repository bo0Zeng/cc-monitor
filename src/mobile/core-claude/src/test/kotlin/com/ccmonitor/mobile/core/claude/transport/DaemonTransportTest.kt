package com.ccmonitor.mobile.core.claude.transport
import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import kotlinx.coroutines.flow.filterIsInstance
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * [DaemonTransport] 解析 cc-monitor daemon wire（PROTO_VERSION=1）9 种帧中的 7 种（两个 tmux 帧为其专属、按未知 kind 跳过），
 * + currentOffset 追 max(byteOffset) + 未知/坏帧跳过 + wire 字段名映射（Line/TurnEnd `session_id`、其余 `sid`）。
 */
class DaemonTransportTest {
    private fun channelOf(vararg lines: String) =
        RemoteCommandChannel { flowOf((lines.joinToString("\n") + "\n").toByteArray()) }

    @Test fun parsesAllWireFramesAndSkipsUnknownAndGarbage() =
        runTest {
            val ch =
                channelOf(
                    """{"kind":"hello","v":1,"build_id":"p1l","host_arch":"x86_64","claude_dir":"/h/.claude","capabilities":["bg","tail-only"],"emits":["turn_end"]}""",
                    """{"kind":"line","session_id":"s1","path":"/a/x.jsonl","seq":0,"raw":"{\"type\":\"user\"}","byte_offset":40}""",
                    """{"kind":"line","session_id":"s1","path":"/a/x.jsonl","seq":1,"raw":"hi","byte_offset":95}""",
                    """{"kind":"session_added","sid":"s2","session_kind":"interactive","cwd":"/proj","name":"work","path":"/a/y.jsonl","lines":42,"status":"idle"}""",
                    """{"kind":"session_status","sid":"s2","status":"running","waiting_for":"tool"}""",
                    """{"kind":"session_removed","sid":"s2"}""",
                    """{"kind":"overflow","dropped":7}""",
                    """{"kind":"turn_end","session_id":"s1","uuid":"u-123"}""",
                    """{"kind":"future_frame","x":1}""", // 未知 kind → 跳
                    """not json at all""", // 坏 JSON → 跳
                )
            val frames = DaemonTransport(ch, "~/.aterm/bin/cc-monitor-remote").frames().toList()

            assertEquals("unknown + garbage 跳，余 8 帧", 8, frames.size)

            val hello = frames.filterIsInstance<JsonlFrame.Hello>().single()
            assertEquals("p1l", hello.buildId)
            assertEquals(listOf("bg", "tail-only"), hello.capabilities)
            assertEquals("emits 独立字段（帧发射声明，非 flag-strip）", listOf("turn_end"), hello.emits)

            val lines = frames.filterIsInstance<JsonlFrame.Line>()
            assertEquals(listOf(40L, 95L), lines.map { it.byteOffset })
            assertEquals("raw 转义还原", listOf("""{"type":"user"}""", "hi"), lines.map { it.raw })
            assertEquals("Line 用 session_id", "s1", lines[0].sessionId)

            val added = frames.filterIsInstance<JsonlFrame.SessionAdded>().single()
            assertEquals("SessionAdded 用 sid", "s2", added.sessionId)
            assertEquals("interactive", added.sessionKind)
            assertEquals(42, added.lines)
            assertNull("省略字段 → null", added.waitingFor)

            val status = frames.filterIsInstance<JsonlFrame.SessionStatus>().single()
            assertEquals("running", status.status)
            assertEquals("tool", status.waitingFor)

            assertEquals("s2", frames.filterIsInstance<JsonlFrame.SessionRemoved>().single().sessionId)
            assertEquals(7, frames.filterIsInstance<JsonlFrame.Overflow>().single().dropped)

            val turnEnd = frames.filterIsInstance<JsonlFrame.TurnEnd>().single()
            assertEquals("TurnEnd 用 session_id", "s1", turnEnd.sessionId)
            assertEquals("u-123", turnEnd.uuid)
        }

    /**
     * `overflow` 不止 `dropped`：丢的是什么也在帧上（`lost` / `lost_truncated`）。
     *
     * 不解析这两格的话，「某条对话的状态帧丢了」永远看不见：后端把 `session_status` 判为丢了补不回来，
     * 手上那个 `waiting` 可能是过期的最后一帧，屏上却继续说「在等」。
     */
    @Test fun overflowCarriesWhatWasLostNotOnlyHowMany() =
        runTest {
            val ch =
                channelOf(
                    """{"kind":"overflow","dropped":3,"lost":[{"kind":"session_status","subject":"s1"},{"kind":"line"}],"lost_truncated":true}""",
                    """{"kind":"overflow","dropped":1}""",
                    """{"kind":"overflow","dropped":2,"lost":[{"subject":"没有 kind 的坏项"},{"kind":"session_status","subject":"s9"}]}""",
                )
            val overflows = DaemonTransport(ch, "cmd").frames().filterIsInstance<JsonlFrame.Overflow>().toList()
            assertEquals("三条都要出来", listOf(3, 1, 2), overflows.map { it.dropped })

            assertEquals(
                "lost 的 kind/subject 都要带过来",
                listOf(JsonlFrame.Overflow.Lost("session_status", "s1"), JsonlFrame.Overflow.Lost("line", null)),
                overflows[0].lost,
            )
            assertTrue("lost_truncated 要认", overflows[0].lostTruncated)

            assertEquals("只发 dropped ⇒ 空表", emptyList<JsonlFrame.Overflow.Lost>(), overflows[1].lost)
            assertFalse("缺席 = 表是全的（不是 true）", overflows[1].lostTruncated)

            // 坏项（没有 kind）跳过、不整条丢：少认一项只是少作废一条等待态，丢掉整个 overflow 才是真的静默。
            assertEquals(
                "坏项跳过，好项留下",
                listOf(JsonlFrame.Overflow.Lost("session_status", "s9")),
                overflows[2].lost,
            )
        }

    @Test fun currentOffsetTracksMaxLineByteOffset() =
        runTest {
            val ch =
                channelOf(
                    """{"kind":"line","session_id":"s","path":"/p","seq":0,"raw":"a","byte_offset":40}""",
                    """{"kind":"overflow","dropped":1}""", // 非 Line 不推进
                    """{"kind":"line","session_id":"s","path":"/p","seq":1,"raw":"b","byte_offset":120}""",
                )
            val t = DaemonTransport(ch, "cmd")
            t.frames().toList()
            assertEquals(120L, t.currentOffset)
        }

    @Test fun currentOffsetNeverGoesBackwardOnOutOfOrderOffsets() =
        runTest {
            // 防御：帧乱序/回放时 currentOffset 只增（max）、不倒退——避免 resume 锚点回退重放。
            val ch =
                channelOf(
                    """{"kind":"line","session_id":"s","path":"/p","seq":0,"raw":"a","byte_offset":200}""",
                    """{"kind":"line","session_id":"s","path":"/p","seq":1,"raw":"b","byte_offset":50}""",
                )
            val t = DaemonTransport(ch, "cmd")
            t.frames().toList()
            assertEquals(200L, t.currentOffset)
        }

    @Test fun lineMissingByteOffsetOrRawIsDropped() =
        runTest {
            val ch =
                channelOf(
                    """{"kind":"line","session_id":"s","path":"/p","seq":0,"raw":"a"}""", // 无 byte_offset → 丢
                    """{"kind":"line","session_id":"s","path":"/p","seq":1,"byte_offset":10}""", // 无 raw → 丢
                    """{"kind":"line","session_id":"s","path":"/p","seq":2,"raw":"c","byte_offset":30}""",
                )
            val lines = DaemonTransport(ch, "cmd").frames().toList().filterIsInstance<JsonlFrame.Line>()
            assertEquals(listOf("c"), lines.map { it.raw })
            assertEquals(listOf(30L), lines.map { it.byteOffset })
        }

    // ---------- per-kind 字段的 additive 解析、缺省与映射 ----------

    @Test fun parsesPerKindWireFieldsCodex() =
        runTest {
            // Hello +codex_dir/+kinds；SessionAdded +agent_kind/+liveness_confidence；SessionStatus +liveness_confidence。
            val ch =
                channelOf(
                    """{"kind":"hello","v":1,"claude_dir":"/h/.claude","codex_dir":"/h/.codex","kinds":["claude","codex"]}""",
                    """{"kind":"session_added","sid":"cx1","path":"/s/r.jsonl","agent_kind":"codex","liveness_confidence":"heuristic"}""",
                    """{"kind":"session_status","sid":"cx1","status":"running","liveness_confidence":"heuristic"}""",
                )
            val frames = DaemonTransport(ch, "cmd").frames().toList()
            val hello = frames.filterIsInstance<JsonlFrame.Hello>().single()
            assertEquals("/h/.codex", hello.codexDir)
            assertEquals(listOf("claude", "codex"), hello.kinds)
            val added = frames.filterIsInstance<JsonlFrame.SessionAdded>().single()
            assertEquals("codex", added.agentKind)
            assertEquals("heuristic", added.livenessConfidence)
            assertEquals(AgentKind.Codex, agentKindFromWire(added.agentKind))
            assertEquals("heuristic → 判活非权威", false, livenessAuthoritative(added.livenessConfidence))
            assertEquals("heuristic", frames.filterIsInstance<JsonlFrame.SessionStatus>().single().livenessConfidence)
        }

    @Test fun perKindFieldsAbsentDefaultToClaude() =
        runTest {
            // 不带 per-kind 字段的 daemon（skip_if_none/skip_if_empty 省略）→ 缺=claude/authoritative。
            val ch =
                channelOf(
                    """{"kind":"hello","v":1,"claude_dir":"/h/.claude"}""",
                    """{"kind":"session_added","sid":"s1","path":"/a/x.jsonl"}""",
                )
            val frames = DaemonTransport(ch, "cmd").frames().toList()
            val hello = frames.filterIsInstance<JsonlFrame.Hello>().single()
            assertNull("codex_dir 省 → null", hello.codexDir)
            assertEquals("kinds 省 → 空", emptyList<String>(), hello.kinds)
            val added = frames.filterIsInstance<JsonlFrame.SessionAdded>().single()
            assertNull("agent_kind 省 → null", added.agentKind)
            assertNull("liveness_confidence 省 → null", added.livenessConfidence)
            assertEquals("缺 agent_kind → ClaudeCode", AgentKind.ClaudeCode, agentKindFromWire(added.agentKind))
            assertEquals("缺 liveness → authoritative(true)", true, livenessAuthoritative(added.livenessConfidence))
        }

    @Test fun wireMappingHelpersDomainAndUnknownTolerant() {
        assertEquals(AgentKind.Codex, agentKindFromWire("codex"))
        assertEquals(AgentKind.ClaudeCode, agentKindFromWire("claude"))
        assertEquals(AgentKind.ClaudeCode, agentKindFromWire(null))
        assertEquals("未知 wire kind → ClaudeCode 兜底", AgentKind.ClaudeCode, agentKindFromWire("gemini"))
        assertEquals(false, livenessAuthoritative("heuristic"))
        assertEquals(true, livenessAuthoritative("authoritative"))
        assertEquals(true, livenessAuthoritative(null))
        assertEquals("未知 liveness 值 → 保守 authoritative(true)", true, livenessAuthoritative("unknown"))
    }

    @Test fun parsesTheBackendsExactSerializationOfPerKindFields() =
        runTest {
            // 后端序列化的精确字节：absent 形字段省略（skip_if_none/skip_if_empty），不是显式 []/"claude"
            // → 按「省=null→缺省 claude/authoritative」解析。
            val ch =
                channelOf(
                    // present（Codex 会话 / 支持 Codex 的 daemon）
                    """{"kind":"hello","v":1,"build_id":"b","host_arch":"x86_64","claude_dir":"/c","codex_dir":"/home/u/.codex","kinds":["claude","codex"]}""",
                    """{"kind":"session_added","sid":"s","agent_kind":"codex","liveness_confidence":"heuristic"}""",
                    """{"kind":"session_status","sid":"s","status":"busy","liveness_confidence":"heuristic"}""",
                    // absent（Claude 会话，字段省略）
                    """{"kind":"hello","v":1,"build_id":"b","host_arch":"x86_64","claude_dir":"/c"}""",
                    """{"kind":"session_added","sid":"s2"}""",
                    """{"kind":"session_status","sid":"s2","status":"idle"}""",
                )
            val frames = DaemonTransport(ch, "cmd").frames().toList()
            val hellos = frames.filterIsInstance<JsonlFrame.Hello>()
            val addeds = frames.filterIsInstance<JsonlFrame.SessionAdded>()
            val statuses = frames.filterIsInstance<JsonlFrame.SessionStatus>()

            // present 形
            assertEquals("/home/u/.codex", hellos[0].codexDir)
            assertEquals(listOf("claude", "codex"), hellos[0].kinds)
            assertEquals(AgentKind.Codex, agentKindFromWire(addeds[0].agentKind))
            assertEquals(false, livenessAuthoritative(addeds[0].livenessConfidence))
            assertEquals("heuristic", statuses[0].livenessConfidence)

            // absent 形（省略 → 缺=claude/authoritative）
            assertNull("codex_dir 省 → null", hellos[1].codexDir)
            assertEquals("kinds 省略(非空数组) → 空", emptyList<String>(), hellos[1].kinds)
            assertNull("agent_kind 省 → null", addeds[1].agentKind)
            assertEquals(AgentKind.ClaudeCode, agentKindFromWire(addeds[1].agentKind))
            assertEquals(true, livenessAuthoritative(addeds[1].livenessConfidence))
            assertNull("liveness_confidence 省 → null", statuses[1].livenessConfidence)
        }

    @Test fun passesStreamCommandVerbatimToChannel() =
        runTest {
            var captured = ""
            val ch =
                RemoteCommandChannel { cmd ->
                    captured = cmd
                    flowOf(ByteArray(0))
                }
            DaemonTransport(ch, "acd --with-bg --tail-only").frames().toList()
            assertEquals("acd --with-bg --tail-only", captured)
        }

    // ---- 两个 additive 字段 ------------------------

    /**
     * `attachable` 只认真布尔，且缺席 = 可 attach。
     *
     * 后端写的是真布尔。字符串 `"false"` 之类必须落到 null = 缺席 = true：宽松解析会让「不可 attach」
     * 被当成「可以」，而那正是这个字段存在的理由。
     */
    @Test
    fun attachableIsParsedOnlyAsARealBooleanAndDefaultsToAttachable() =
        runTest {
            val frames =
                DaemonTransport(
                    channelOf(
                        """{"kind":"session_added","sid":"absent","path":"/p.jsonl"}""",
                        """{"kind":"session_added","sid":"real","path":"/p.jsonl","attachable":false}""",
                        """{"kind":"session_added","sid":"stringy","path":"/p.jsonl","attachable":"false"}""",
                    ),
                    "acd",
                ).frames().toList().filterIsInstance<JsonlFrame.SessionAdded>()
            assertEquals("前提：三条都要解析出来", 3, frames.size)

            assertEquals("缺席 ⇒ null（消费方按「可 attach」处理）", null, frames[0].attachable)
            assertEquals("真布尔 false 必须读出来", false, frames[1].attachable)
            assertEquals("字符串不是布尔：按缺席处理，不做宽松解析", null, frames[2].attachable)
        }

    /**
     * `SessionRemoved.cause`：`"superseded"`（`/branch`、`/clear` 原地换 sid）不是「会话死了」。
     *
     * 丢掉这个轴，UI 分不清「退出了」和「执行了 /clear」。
     */
    @Test
    fun sessionRemovedCarriesItsCauseSoSupersededIsNotMistakenForGone() =
        runTest {
            val frames =
                DaemonTransport(
                    channelOf(
                        """{"kind":"session_removed","sid":"gone"}""",
                        """{"kind":"session_removed","sid":"branched","cause":"superseded"}""",
                    ),
                    "acd",
                ).frames().toList().filterIsInstance<JsonlFrame.SessionRemoved>()
            assertEquals("前提：两条都要解析出来", 2, frames.size)

            assertEquals("缺席 ⇒ null（后端的默认值 gone 不序列化）", null, frames[0].cause)
            assertEquals("superseded 必须读出来", "superseded", frames[1].cause)
        }

    /**
     * `session_added` 上的 `activity` 与 `background` 也必须收。
     *
     * `session_added` 是「连上之前就在等」的唯一到达路径：后端首次是同步全扫，已经在等的会话只走这一支，
     * 而这正是手机上最常见的形状（合上手机、过一小时回来、app 重连）。这一帧上没有 `status`，
     * 漏了 `activity` 这条路径就恒哑，而且一条断言都不会红。
     *
     * `background` 是两态：后端只在 true 时序列化 ⇒ 缺席 ≡ false ≡ 交互会话。
     * 字符串 `"true"` 一律落 false（同 `attachable` 那条只认真布尔）。
     */
    @Test
    fun sessionAddedCarriesTheCookedActivityAndTheBackgroundFlag() =
        runTest {
            val ch =
                channelOf(
                    """{"kind":"hello","v":1,"build_id":"p9s-record-arg-line"}""",
                    // 没有 status，只有 activity
                    """{"kind":"session_added","sid":"s1","activity":"needs_you","waiting_for":"permission prompt","background":true}""",
                    """{"kind":"session_added","sid":"s2"}""",
                    """{"kind":"session_added","sid":"s3","background":"true"}""",
                )
            val got = DaemonTransport(ch, "ccm").frames().toList().filterIsInstance<JsonlFrame.SessionAdded>()
            assertEquals("前提：三条都要解析出来", 3, got.size)
            assertEquals("宣告时的活动档必须收下：这一帧上没有 status", "needs_you", got[0].activity)
            assertEquals("等什么那一格照旧", "permission prompt", got[0].waitingFor)
            assertTrue("后台会话那一格要收下", got[0].background)
            assertNull("缺席 = 对端说「说不清」，不许编一个", got[1].activity)
            assertFalse("缺席 ≡ false ≡ 交互会话（对端只在 true 时上线）", got[1].background)
            assertFalse("只认真布尔：字符串一律落 false（同 attachable 那条口径）", got[2].background)
        }

    /**
     * 缺 `raw` 的 `line` 帧不上抛，而且绝不推进 resume baseline。
     *
     * 后端的 `line` 帧只在客户端发了 `--with-raw` 时才带 `raw`，而生产的 `want` 里没有这个词，这一形真的会到达。
     *
     * 这条同时挡住两种看似可行的处理，各自都是一次静默失效：
     * - 把帧改成 `raw = ""` 上抛 ⇒ 消费方会去 parse 一个空串，得到一条垃圾记录；
     * - 推进 `currentOffset` ⇒ 下次 resume 从它往后拉 ⇒ 那几条记录永远丢。补齐前不推进 baseline。
     *
     * drop 的代价是贵（`currentOffset` 停在已交付的那一行 ⇒ resume 从头重下，中位 1.60 MB、最大 296 MB），
     * 但不丢数据。见 [DaemonTransport] 的 `parseLine` KDoc。
     */
    @Test
    fun lineWithoutRawIsDroppedAndNeverAdvancesTheResumeBaseline() =
        runTest {
            val ch =
                channelOf(
                    """{"kind":"hello","v":1,"build_id":"p9s-record-arg-line","emits":["line"]}""",
                    """{"kind":"line","session_id":"s1","seq":0,"raw":"{}","byte_offset":100}""",
                    // 不发 `--with-raw` 时后端省掉的正是这一格
                    """{"kind":"line","session_id":"s1","seq":1,"byte_offset":500}""",
                )
            val t = DaemonTransport(ch, "ccm")
            val lines = t.frames().toList().filterIsInstance<JsonlFrame.Line>()
            assertEquals("缺 raw 的那一帧不许上抛：没有正文可给消费方", 1, lines.size)
            assertEquals("前提：带 raw 的那一帧照常上抛", 100L, lines.single().byteOffset)
            assertEquals(
                "baseline 绝不许越过一条没交付给消费方的记录：越过去 ⇒ 下次 resume 静默跳过它。" +
                    "补齐前不推进 baseline",
                100L,
                t.currentOffset,
            )
        }

    /**
     * `activity` 与 `status` 两格都要收，不许折叠成一格。
     *
     * 后端的 `session_added` / `session_status` 可能只带 `activity`、不带 `status`，只认 `status` 的话红绿灯整片熄掉。
     * 反过来也不许只认 `activity`：两者不是一套值。`status` 有 `shell`（「在跑别的命令」），
     * 而后端把它并进了 `Idle`（`"idle" | "shell" => Idle`）⇒ 只读 `activity` 会丢掉那一档。
     */
    @Test
    fun sessionStatusKeepsBothTheRawStatusAndTheCookedActivity() =
        runTest {
            val ch =
                channelOf(
                    """{"kind":"hello","v":1,"build_id":"p1l","capabilities":["bg"],"emits":["turn_end"]}""",
                    // 两格都给
                    """{"kind":"session_status","sid":"s1","status":"shell","activity":"idle"}""",
                    // 只给 `activity`
                    """{"kind":"session_status","sid":"s1","activity":"needs_you"}""",
                )
            val got = DaemonTransport(ch, "ccm").frames().toList().filterIsInstance<JsonlFrame.SessionStatus>()
            assertEquals(2, got.size)
            assertEquals("原值那一格不许丢（shell 只在它身上）", "shell", got[0].status)
            assertEquals("成品那一格要收下", "idle", got[0].activity)
            assertNull("只给 activity 时，status 缺席是正常的", got[1].status)
            assertEquals("needs_you", got[1].activity)
        }
}
