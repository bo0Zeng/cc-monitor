package com.ccmonitor.mobile.core.claude.catalog

import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 历史分组：stat 命令/解析、按真实 cwd 分组（编码目录收养/兜底）、
 * historyByProject 渐进 Flow（分片探针 + 骨架先行 + stat 失败退 ls）。纯 JVM。
 */
class ClaudeSessionHistoryTest {
    private val rs = ClaudeSessionCatalog.CHUNK_MARKER

    // ---------- 纯函数 ----------

    @Test fun statTimesCommandShapeAndReservedSubstringsSafe() {
        val cmd = ClaudeSessionCatalog.statTimesCommand("\${CLAUDE_CONFIG_DIR:-\$HOME/.claude}", null)
        // 默认前缀双引号包裹（保留 $CLAUDE_CONFIG_DIR/$HOME 展开、防含空格展开值 word-split），glob 仍活。
        assertEquals(
            "stat -c '%Y %n' -- \"\${CLAUDE_CONFIG_DIR:-\$HOME/.claude}\"/projects/*/*.jsonl 2>/dev/null | sort -rn",
            cmd,
        )
        // 保留子串契约：不得与 app 测试 fake 的既有分派（ls/wc/tail/for/pidfile/awk）撞车。
        assertFalse(cmd.startsWith("ls "))
        assertFalse(cmd.startsWith("wc -c"))
        assertFalse(cmd.startsWith("for "))
        assertFalse(cmd.contains("tail -"))
        assertFalse(cmd.contains("sessions/*.json"))
        assertFalse(cmd.contains("LC_ALL=C awk"))
    }

    @Test fun statTimesCommandScopesToProjectWhenCwdGiven() {
        val cmd = ClaudeSessionCatalog.statTimesCommand("/h/.claude", "/home/u/proj")
        // 覆写前缀单引号包裹（含空格安全），项目编码目录 + glob 仍活。
        assertTrue("cwd 非空 → 该项目编码目录 glob：$cmd", cmd.contains("'/h/.claude'/projects/-home-u-proj/*.jsonl"))
    }

    // pidfiles（活会话检测）也 quote claudeDir 前缀：含空格覆写被 word-split 的话 for 循环落空、判活静默失效。
    @Test fun pidfilesCommandQuotesClaudeDirPrefix() {
        // 默认值 → 双引号（$CLAUDE_CONFIG_DIR/$HOME 仍展开）、glob 活。
        assertTrue(
            ClaudeSessionCatalog
                .pidfilesCommand("\${CLAUDE_CONFIG_DIR:-\$HOME/.claude}")
                .startsWith("for f in \"\${CLAUDE_CONFIG_DIR:-\$HOME/.claude}\"/sessions/*.json;"),
        )
        // 含空格覆写 → 单引号包裹（不被 word-split）。
        assertTrue(ClaudeSessionCatalog.pidfilesCommand("/srv/c d").startsWith("for f in '/srv/c d'/sessions/*.json;"))
    }

    @Test fun parseStatTimesTolerant() {
        val out =
            ClaudeSessionCatalog.parseStatTimes(
                """
                |1730000200 /h/.claude/projects/-a/x1.jsonl
                |1730000100 /h/.claude/projects/-a b/x 2.jsonl
                |garbage line
                |1730000050 /h/.claude/projects/-a/notes.txt
                |notanumber /h/.claude/projects/-a/x3.jsonl
                |
                """.trimMargin(),
            )
        assertEquals(2, out.size)
        assertEquals(TimedPath("/h/.claude/projects/-a/x1.jsonl", 1_730_000_200L), out[0])
        assertEquals("路径含空格也完整保留", "/h/.claude/projects/-a b/x 2.jsonl", out[1].path)
    }

    @Test fun encodedProjectDirTakesParentSegment() {
        assertEquals("-home-u-proj", ClaudeSessionCatalog.encodedProjectDir("/h/.claude/projects/-home-u-proj/sid.jsonl"))
        assertEquals("", ClaudeSessionCatalog.encodedProjectDir("sid.jsonl"))
    }

    @Test fun projectLabelPrefersRealCwdTailSegment() {
        assertEquals("android-terminal", ClaudeSessionCatalog.projectLabel("/home/u/project/android-terminal", "-x"))
        assertEquals("尾随斜杠归一", "proj", ClaudeSessionCatalog.projectLabel("/home/u/proj/", "-x"))
        assertEquals("/", ClaudeSessionCatalog.projectLabel("/", "-x"))
        assertEquals("cwd 未知 → 编码目录名原样（有损，仅展示）", "-home-u-proj", ClaudeSessionCatalog.projectLabel(null, "-home-u-proj"))
        assertEquals("(未知项目)", ClaudeSessionCatalog.projectLabel(null, ""))
    }

    @Test fun groupHistoryGroupsByRealCwdAndAdoptsEncSiblings() {
        val listed =
            listOf(
                TimedPath("/h/p/-home-u-alpha/s1.jsonl", 300L),
                TimedPath("/h/p/-home-u-beta/s2.jsonl", 200L),
                TimedPath("/h/p/-home-u-alpha/s3.jsonl", 100L), // 同 alpha 编码目录、探针未解出 cwd → 收养 s1 的（仅分组）
            )
        val parts =
            mapOf(
                "/h/p/-home-u-alpha/s1.jsonl" to TitleParts(null, "修复登录", null, "/home/u/alpha"),
                // s2/s3 无素材（探针没抓到 user 行）
            )
        val groups = ClaudeSessionCatalog.groupHistory(listed, parts)
        assertEquals(2, groups.size)
        assertEquals("组序=最近优先（首见序）", "alpha", groups[0].label)
        assertEquals("/home/u/alpha", groups[0].cwd)
        assertEquals("同编码目录未知者收养真实 cwd 作组键 → 并入同组", 2, groups[0].sessions.size)
        assertEquals(listOf("s1", "s3"), groups[0].sessions.map { it.sessionId })
        // 收养与 resume 解耦——收养只进组键/组展示，HistorySession.cwd 仅真解出时写。
        assertEquals("真解出 cwd 的会话带 cwd", "/home/u/alpha", groups[0].sessions[0].cwd)
        assertNull("收养的会话 cwd=null（收养仅供分组，不再假装真值）", groups[0].sessions[1].cwd)
        assertEquals("cwd 全未知的组 → 编码目录名兜底", "-home-u-beta", groups[1].label)
        assertNull(groups[1].cwd)
        assertNull("cwd 未知 → 字段为 null（resume 由远端 find-by-sid 兜底，不看本字段）", groups[1].sessions[0].cwd)
    }

    @Test fun groupHistoryFlagsLiveViaLiveSids() {
        val listed = listOf(TimedPath("/h/p/-a/s1.jsonl", 2L), TimedPath("/h/p/-a/s2.jsonl", 1L))
        val groups = ClaudeSessionCatalog.groupHistory(listed, emptyMap(), liveSids = setOf("s1"))
        val s = groups.single().sessions
        assertEquals("活会话打 live 标（UI 禁 resume）", true, s.first { it.sessionId == "s1" }.live)
        assertEquals(false, s.first { it.sessionId == "s2" }.live)
    }

    @Test fun groupHistoryIsolatesCraftedEncCwdKey() {
        // cwd 是 JSONL 可写文本，构造 cwd="enc:-home-u-beta" 不得与真编码兜底组撞键（前缀隔离）。
        val listed =
            listOf(
                TimedPath("/h/p/-home-u-beta/s1.jsonl", 2L), // 无素材 → enc 兜底组
                TimedPath("/h/p/-x/s2.jsonl", 1L), // cwd 素材 = "enc:-home-u-beta"（构造）
            )
        val parts = mapOf("/h/p/-x/s2.jsonl" to TitleParts(null, null, null, "enc:-home-u-beta"))
        val groups = ClaudeSessionCatalog.groupHistory(listed, parts)
        assertEquals("构造 cwd 与真编码兜底组隔离 → 两组不撞", 2, groups.size)
    }

    @Test fun groupHistoryTitleFallbackAndMtimeCarried() {
        val listed = listOf(TimedPath("/h/p/-a/abcdef12-9999.jsonl", 42L))
        val skeleton = ClaudeSessionCatalog.groupHistory(listed, emptyMap())
        assertEquals("素材未到 → sid8 占位", "abcdef12", skeleton[0].sessions[0].title)
        assertEquals(42L, skeleton[0].sessions[0].mtimeEpochSec)
        val labeled =
            ClaudeSessionCatalog.groupHistory(
                listed,
                mapOf("/h/p/-a/abcdef12-9999.jsonl" to TitleParts("我改的标题", "ai标题", "首条输入", "/a")),
            )
        assertEquals("custom 最优先", "我改的标题", labeled[0].sessions[0].title)
    }

    // ---------- historyByProject 端到端（fake channel） ----------

    private fun historyChannel(
        commands: MutableList<String>,
        statOut: String,
        lsOut: String = "",
        titleOutByProbe: (probeIndex: Int) -> String = { "" },
    ): RemoteCommandChannel {
        var probeIdx = 0
        return RemoteCommandChannel { cmd ->
            commands.add(cmd)
            when {
                cmd.startsWith("stat -c") -> flowOf(statOut.toByteArray())
                cmd.startsWith("ls ") -> flowOf(lsOut.toByteArray())
                // pidfiles（判活）前缀 quote → 覆写 claudeDir 也以 `for f in '` 起头，须先按 sessions/*.json
                // 分出（否则被下面的标题探针分支误吞、偷走一次探针）。本组测试无活会话 → 空。
                cmd.contains("sessions/*.json") -> emptyFlow()
                cmd.startsWith("for f in '") -> flowOf(titleOutByProbe(probeIdx++).toByteArray())
                else -> emptyFlow()
            }
        }
    }

    @Test fun historyChunksTitleProbeAndEmitsProgressively() =
        runTest {
            // 20 个文件（> HISTORY_TITLE_CHUNK=15）→ 探针分 2 片；发射 = 1 骨架 + 2 片 = 3 版。
            val statOut = (20 downTo 1).joinToString("") { i -> "${1000 + i} /h/p/-proj/sid-%02d.jsonl\n".format(i) }
            val commands = mutableListOf<String>()
            val channel =
                historyChannel(
                    commands,
                    statOut = statOut,
                    titleOutByProbe = { idx ->
                        if (idx == 0) {
                            "$rs/h/p/-proj/sid-20.jsonl\n" +
                                """{"type":"user","cwd":"/home/u/proj","message":{"role":"user","content":"最近的活"}}""" + "\n"
                        } else {
                            ""
                        }
                    },
                )
            val emissions = ClaudeSessionCatalog(channel, "/h").historyByProject().toList()

            assertEquals("骨架 + 每片一版", 3, emissions.size)
            val probes = commands.filter { it.startsWith("for f in '") && !it.contains("sessions/*.json") } // 排除 pidfiles
            assertEquals("20 文件按 15/片 → 2 片", 2, probes.size)
            assertTrue("每片 ≤ CHUNK 个文件", probes.all { p -> p.split(".jsonl'").size - 1 <= ClaudeSessionCatalog.HISTORY_TITLE_CHUNK })
            // 骨架版：无标题素材 → 编码目录兜底组 + sid8 标题。
            assertEquals("-proj", emissions[0].single().label)
            assertEquals("sid-20", emissions[0].single().sessions[0].title)
            // 第一片到货：sid-20 解出 cwd + 摘要标题；同编码目录其余会话收养 cwd。
            val final = emissions.last().single()
            assertEquals("proj", final.label)
            assertEquals("/home/u/proj", final.cwd)
            assertEquals(20, final.sessions.size)
            assertEquals("最近的活", final.sessions[0].title)
            assertEquals("mtime 序：最新在前", "sid-20", final.sessions[0].sessionId)
            assertEquals(1020L, final.sessions[0].mtimeEpochSec)
            // 注入安全：探针路径逐个单引号 quote。
            assertTrue(probes.all { it.contains("'/h/p/-proj/sid-20.jsonl'") || it.contains(".jsonl'") })
        }

    @Test fun historyFlagsLiveSessionsViaPidfiles() =
        runTest {
            // historyByProject 并入 pidfile 判活——live-01 有活进程 → live=true，dead-02 → false。
            val statOut = "1002 /h/p/-proj/live-01.jsonl\n1001 /h/p/-proj/dead-02.jsonl\n"
            val channel =
                RemoteCommandChannel { cmd ->
                    when {
                        cmd.startsWith("stat -c") -> flowOf(statOut.toByteArray())
                        cmd.startsWith("for f in") && cmd.contains("sessions/*.json") ->
                            flowOf(("$rs/h/sessions/500.json\n" + """{"pid":500,"sessionId":"live-01","cwd":"/home/u/proj"}""" + "\n").toByteArray())
                        cmd.startsWith("for p in") -> flowOf("500\n".toByteArray()) // pid 500 活
                        cmd.startsWith("for f in '") -> flowOf("".toByteArray())
                        else -> emptyFlow()
                    }
                }
            val emissions = ClaudeSessionCatalog(channel, "/h").historyByProject().toList()
            val sessions = emissions.last().flatMap { it.sessions }
            assertEquals("活会话打标", true, sessions.first { it.sessionId == "live-01" }.live)
            assertEquals("死会话不打标", false, sessions.first { it.sessionId == "dead-02" }.live)
        }

    @Test fun historyFallsBackToLsWhenStatUnavailable() =
        runTest {
            val commands = mutableListOf<String>()
            val channel =
                historyChannel(
                    commands,
                    statOut = "", // BSD stat / 无 stat → 空输出
                    lsOut = "/h/p/-a/s1.jsonl\n/h/p/-b/s2.jsonl\n",
                )
            val final = ClaudeSessionCatalog(channel, "/h").historyByProject().toList().last()
            assertEquals("退 ls：两个编码目录兜底组", 2, final.size)
            assertNull("无 stat → 无 mtime（相对时间隐藏）", final[0].sessions[0].mtimeEpochSec)
            assertTrue("先试 stat 再退 ls", commands.first().startsWith("stat -c"))
        }

    @Test fun historyRespectsLimit() =
        runTest {
            val statOut = (1..6).joinToString("") { i -> "${100 - i} /h/p/-a/s$i.jsonl\n" }
            val channel = historyChannel(mutableListOf(), statOut = statOut)
            val final = ClaudeSessionCatalog(channel, "/h").historyByProject(limit = 2).toList().last()
            assertEquals("封顶 limit 条（最近优先）", 2, final.single().sessions.size)
            assertEquals(listOf("s1", "s2"), final.single().sessions.map { it.sessionId })
        }

    @Test fun historyEmptyEmitsEmptyOnce() =
        runTest {
            val channel = historyChannel(mutableListOf(), statOut = "", lsOut = "")
            val emissions = ClaudeSessionCatalog(channel, "/h").historyByProject().toList()
            assertEquals("空历史只发一版空列表（UI 显『无历史』）", listOf(emptyList<ProjectHistory>()), emissions)
        }
}
