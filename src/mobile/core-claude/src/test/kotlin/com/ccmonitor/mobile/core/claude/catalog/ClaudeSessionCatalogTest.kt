package com.ccmonitor.mobile.core.claude.catalog

import com.ccmonitor.mobile.core.claude.model.JsonlParser
import com.ccmonitor.mobile.core.remote.ConnectionDeadException
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.IOException

/**
 * ClaudeSessionCatalog 纯函数 + 端到端（fake channel）JVM 测试。
 * 覆盖：pidfile 解析容错 / 存活命令注入安全 / 标题回退链 / firstUserExcerpt 真实性判定 / 兜底路径。
 */
class ClaudeSessionCatalogTest {
    private val rs = ClaudeSessionCatalog.CHUNK_MARKER

    // ---------- pidfile 解析 ----------

    @Test fun parsePidfilesMultipleAndTolerant() {
        val raw =
            "$rs/h/.claude/sessions/100.json\n" +
                """{"pid":100,"sessionId":"aaaa-1111","cwd":"/home/u/proj","name":"修复登录","procStart":"777"}""" + "\n" +
                "$rs/h/.claude/sessions/bad.json\n{not json}\n" + // 坏 JSON → 跳过
                "$rs/h/.claude/sessions/nosid.json\n" + """{"pid":7}""" + "\n" + // 缺 sessionId → 跳过
                "$rs/h/.claude/sessions/negpid.json\n" + """{"pid":-3,"sessionId":"x"}""" + "\n" + // 非法 pid → 跳过
                "$rs/h/.claude/sessions/200.json\n" + """{"pid":200,"sessionId":"bbbb-2222"}""" + "\n"
        val parsed = ClaudeSessionCatalog.parsePidfiles(raw)
        assertEquals(2, parsed.size)
        assertEquals(SessionPidfile(100, "aaaa-1111", "/home/u/proj", "修复登录", "777"), parsed[0])
        assertEquals(SessionPidfile(200, "bbbb-2222", null, null, null), parsed[1])
    }

    @Test fun parsePidfilesStatusKindFields() {
        // 红绿灯：验 status/kind/waitingFor/statusUpdatedAt 的 moshi key 真解析，防 key 写错（纯逻辑测全绿也发现不了）。
        val raw =
            "$rs/h/.claude/sessions/300.json\n" +
                """{"pid":300,"sessionId":"cccc-3333","kind":"interactive","status":"busy",""" +
                """"statusUpdatedAt":"12345","waitingFor":"permission"}""" + "\n"
        val p = ClaudeSessionCatalog.parsePidfiles(raw).single()
        assertEquals("busy", p.status)
        assertEquals("interactive", p.kind)
        assertEquals("permission", p.waitingFor)
        assertEquals("12345", p.statusUpdatedAt)
    }

    @Test fun parsePidfilesEmptyOrGarbage() {
        assertTrue(ClaudeSessionCatalog.parsePidfiles("").isEmpty())
        assertTrue(ClaudeSessionCatalog.parsePidfiles("random noise\nno marker").isEmpty())
    }

    // ---------- 存活探测 ----------

    @Test fun livenessProbeCommandShapeAndSafety() {
        val cmd = ClaudeSessionCatalog.livenessProbeCommand(listOf(100L, 200L, 100L))!!
        assertTrue("去重后 pid 列表", cmd.contains("for p in 100 200; do"))
        assertTrue("先探 /proc（规避 kill -0 EPERM 误判）", cmd.contains("/proc/\$p"))
        assertTrue("kill -0 兜底", cmd.contains("kill -0"))
        assertTrue("读 starttime（PID 复用防护）", cmd.contains("/proc/\$p/stat"))
        assertTrue("吐 pid + starttime", cmd.contains("echo \"\$p \$st\""))
        assertFalse("不得含 tail（fake channel 按子串分派）", cmd.contains("tail"))
        assertNull("非法 pid 全滤掉 → null", ClaudeSessionCatalog.livenessProbeCommand(listOf(0L, -5L)))
    }

    @Test fun parseProbeParsesPidAndStarttime() {
        val probe = ClaudeSessionCatalog.parseProbe("100 777\nkill: no such process\n200\n")
        assertEquals("777", probe[100L]) // pid + starttime
        assertEquals("", probe[200L]) // pid 存在但 starttime 读失败（空段）
        assertEquals(2, probe.size) // 垃圾行（无数字 pid）忽略
    }

    @Test fun filterAliveByIdentityGuardsPidReuse() {
        val live = SessionPidfile(100, "live", "/p", null, "777") // starttime 匹配 → 活
        val reused = SessionPidfile(200, "stale", "/p", null, "888") // pidfile procStart 888，但 200 现被别的进程占（starttime 999）→ 死
        val old = SessionPidfile(300, "old", "/p", null, null) // 无 procStart → 仅存在性
        val emptyStat = SessionPidfile(400, "e", "/p", null, "111") // pid 存在但 stat 读不到（空）+ 有 procStart → 判死
        val probe = mapOf(100L to "777", 200L to "999", 300L to "555", 400L to "", 999L to "x")
        val alive = ClaudeSessionCatalog.filterAliveByIdentity(listOf(live, reused, old, emptyStat), probe)
        assertEquals("只有 starttime 一致的 live + 无 procStart 的 old 存活", listOf(live, old), alive)
    }

    @Test fun filterAliveByIdentityExcludesDeadPid() {
        val pf = SessionPidfile(100, "s", "/p", null, "777")
        assertTrue("pid 不在存活集 → 死", ClaudeSessionCatalog.filterAliveByIdentity(listOf(pf), emptyMap()).isEmpty())
    }

    // ---------- 标题回退链 ----------

    @Test fun titlePartsCustomBeatsAiAndLastWins() {
        val records =
            listOf(
                """{"type":"ai-title","aiTitle":"旧 AI 标题","sessionId":"s"}""",
                """{"type":"ai-title","aiTitle":"新 AI 标题","sessionId":"s"}""",
                """{"type":"custom-title","customTitle":"手设标题","sessionId":"s"}""",
            ).map { JsonlParser.parse(it) }
        val parts = ClaudeSessionCatalog.titleParts(records)
        assertEquals("手设标题", parts.custom)
        assertEquals("新 AI 标题", parts.ai) // 同类取最后一条
        assertEquals("手设标题", ClaudeSessionCatalog.readableTitle(parts, null, "abcdef12-3456"))
    }

    @Test fun readableTitleFallbackChain() {
        val sid = "abcdef12-3456-7890"
        assertEquals("C", ClaudeSessionCatalog.readableTitle(TitleParts("C", "A", "E"), "N", sid))
        assertEquals("A", ClaudeSessionCatalog.readableTitle(TitleParts(null, "A", "E"), "N", sid))
        assertEquals("N", ClaudeSessionCatalog.readableTitle(TitleParts(null, null, "E"), "N", sid)) // pidfile name 作 ai 同位补充
        assertEquals("E", ClaudeSessionCatalog.readableTitle(TitleParts(null, null, "E"), null, sid))
        assertEquals("abcdef12", ClaudeSessionCatalog.readableTitle(TitleParts(null, null, null), "  ", sid))
        assertEquals("abcdef12", ClaudeSessionCatalog.readableTitle(null, null, sid)) // 探针整体失败 → UUID8
    }

    // ---------- firstUserExcerpt ----------

    @Test fun firstUserExcerptStripsWrapperAndSkipsMeta() {
        val records =
            listOf(
                // meta → 跳过
                """{"type":"user","isMeta":true,"message":{"role":"user","content":"Caveat: ..."}}""",
                // 整条都是包装噪音 → 跳过
                """{"type":"user","message":{"role":"user","content":"<system-reminder>注入的上下文</system-reminder>"}}""",
                // 真实输入（带包装前缀）→ 命中且剥净
                """{"type":"user","message":{"role":"user","content":"<system-reminder>x</system-reminder>全面查看理解一下这个项目"}}""",
            ).map { JsonlParser.parse(it) }
        assertEquals("全面查看理解一下这个项目", ClaudeSessionCatalog.firstUserExcerpt(records))
    }

    @Test fun firstUserExcerptSkipsToolResultAndCompact() {
        val records =
            listOf(
                // tool_result 也是 user 记录，但无 Text 块 → 跳过
                """{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"大量输出"}]}}""",
                // /compact 续接摘要 → 跳过
                """{"type":"user","message":{"role":"user","content":"This session is being continued from a previous conversation..."}}""",
                """{"type":"user","message":{"role":"user","content":"修一下登录 bug"}}""",
            ).map { JsonlParser.parse(it) }
        assertEquals("修一下登录 bug", ClaudeSessionCatalog.firstUserExcerpt(records))
    }

    @Test fun firstUserExcerptRendersSlashAndBashInput() {
        val slash =
            JsonlParser.parse(
                """{"type":"user","message":{"role":"user","content":"<command-name>/model</command-name><command-message>model</command-message><command-args>opus</command-args>"}}""",
            )
        assertEquals("/model opus", ClaudeSessionCatalog.firstUserExcerpt(listOf(slash)))
        val bash = JsonlParser.parse("""{"type":"user","message":{"role":"user","content":"<bash-input>ls -la</bash-input>"}}""")
        assertEquals("!ls -la", ClaudeSessionCatalog.firstUserExcerpt(listOf(bash)))
        val bashOut = JsonlParser.parse("""{"type":"user","message":{"role":"user","content":"<bash-stdout>out</bash-stdout>"}}""")
        assertNull("bash 输出回显非用户输入", ClaudeSessionCatalog.firstUserExcerpt(listOf(bashOut)))
    }

    @Test fun firstUserExcerptCollapsesWhitespaceAndTruncates() {
        val long = "长".repeat(130)
        val rec = JsonlParser.parse("""{"type":"user","message":{"role":"user","content":"第一行\n  第二行\t$long"}}""")
        val excerpt = ClaudeSessionCatalog.firstUserExcerpt(listOf(rec))!!
        assertTrue("换行/tab 折叠成单空格", excerpt.startsWith("第一行 第二行 长"))
        assertEquals(ClaudeSessionCatalog.EXCERPT_MAX_CHARS + 1, excerpt.length) // 120 + '…'
        assertTrue(excerpt.endsWith("…"))
    }

    @Test fun truncateCharsSurrogateSafe() {
        assertEquals("短的不动", ClaudeSessionCatalog.truncateChars("短的不动", 120))
        val s = "a".repeat(119) + "😀延伸" // 😀 = 代理对，横跨切点 119..120
        val cut = ClaudeSessionCatalog.truncateChars(s, 120)
        assertEquals("a".repeat(119) + "…", cut) // 高代理项不劈半 → 回退一位
    }

    // ---------- cwd 匹配 / sessionId ----------

    @Test fun cwdMatchesNormalizesViaProjectDirName() {
        assertTrue(ClaudeSessionCatalog.cwdMatches("/home/u/proj", "/home/u/proj"))
        assertFalse(ClaudeSessionCatalog.cwdMatches("/home/u/other", "/home/u/proj"))
        assertTrue("pidfile 缺 cwd → 放行（项目 glob 兜底过滤）", ClaudeSessionCatalog.cwdMatches(null, "/home/u/proj"))
        assertTrue("阅读面无 cwd（全局）→ 放行", ClaudeSessionCatalog.cwdMatches("/home/u/proj", null))
    }

    @Test fun sessionIdFromPath() {
        assertEquals("aaaa-1111", ClaudeSessionCatalog.sessionIdOf("/h/.claude/projects/-home-u-proj/aaaa-1111.jsonl"))
    }

    // ---------- 端到端（fake channel） ----------

    private val projDir = "/h/.claude/projects/-home-u-proj"

    private fun fakeChannel(
        commands: MutableList<String>,
        lsOut: String,
        pidOut: String = "",
        aliveOut: String = "",
        titleOut: String = "",
    ) = RemoteCommandChannel { cmd ->
        commands.add(cmd)
        when {
            cmd.startsWith("ls ") -> flowOf(lsOut.toByteArray())
            cmd.startsWith("for f in") && cmd.contains("sessions/*.json") -> flowOf(pidOut.toByteArray())
            cmd.startsWith("for p in") -> flowOf(aliveOut.toByteArray())
            cmd.startsWith("for f in '") -> flowOf(titleOut.toByteArray())
            else -> emptyFlow()
        }
    }

    @Test fun liveSessionsFiltersToAlivePidfileSession() =
        runTest {
            val commands = mutableListOf<String>()
            val channel =
                fakeChannel(
                    commands,
                    lsOut = "$projDir/bbbb-2222.jsonl\n$projDir/aaaa-1111.jsonl\n$projDir/cccc-3333.jsonl\n",
                    pidOut =
                        "$rs/h/.claude/sessions/100.json\n" +
                            """{"pid":100,"sessionId":"aaaa-1111","cwd":"/home/u/proj"}""" + "\n" +
                            "$rs/h/.claude/sessions/200.json\n" +
                            """{"pid":200,"sessionId":"bbbb-2222","cwd":"/home/u/proj"}""" + "\n",
                    aliveOut = "100\n", // 只有 100 活 → 只有 aaaa-1111 是活动会话
                    titleOut =
                        "$rs$projDir/aaaa-1111.jsonl\n" +
                            """{"type":"ai-title","aiTitle":"修复登录","sessionId":"aaaa-1111"}""" + "\n",
                )
            val sessions = ClaudeSessionCatalog(channel, "\${CLAUDE_CONFIG_DIR:-\$HOME/.claude}").liveSessions("/home/u/proj")

            assertEquals("只列活动会话（非最新 mtime 的 bbbb）", 1, sessions.size)
            assertEquals("$projDir/aaaa-1111.jsonl", sessions[0].path)
            assertEquals("修复登录", sessions[0].title)
            assertTrue(sessions[0].live)
            // 目录命令不得与 tail 管线（TailTransport `tail -`/remoteSize `wc -c`/SkeletonScan `LC_ALL=C awk`）撞子串——
            // 否则真实 VM 无碍（走真 SSH 通道），但测试 fake 的子串分派会误路由。存活探测虽用 `awk -F')'`，形态不同、不撞。
            assertTrue("目录命令不与 tail 管线子串冲突", commands.none { it.contains("tail -") || it.startsWith("wc -c") || it.contains("LC_ALL=C awk") })
        }

    @Test fun fallbackToMtimeListWhenNoPidfiles() =
        runTest {
            val commands = mutableListOf<String>()
            val channel =
                fakeChannel(
                    commands,
                    lsOut = "$projDir/bbbb-2222.jsonl\n$projDir/aaaa-1111.jsonl\n",
                    pidOut = "", // 不写 pidfile 的 Claude
                    titleOut =
                        "$rs$projDir/bbbb-2222.jsonl\n" +
                            """{"type":"user","message":{"role":"user","content":"帮我写周报"}}""" + "\n" +
                            "$rs$projDir/aaaa-1111.jsonl\n", // 无任何素材 → UUID8
                )
            val sessions = ClaudeSessionCatalog(channel, "/h/.claude").liveSessions("/home/u/proj")

            assertEquals("兜底 = 全列表（mtime 序）", 2, sessions.size)
            assertEquals("帮我写周报", sessions[0].title) // 可读标题无条件生效
            assertEquals("aaaa-111", sessions[1].title) // UUID 前 8 位兜底
            assertFalse(sessions[0].live)
            assertFalse("无 pidfile → 不发存活探测", commands.any { it.startsWith("for p in") })
        }

    // 活动集无上限（兜底才截 15）：>15 个活会话时标题探针必须按 HISTORY_TITLE_CHUNK 分片
    // （单命令探针只在 ≤15 个文件时安全），不得一条超长命令带全部 20 个路径。
    @Test fun liveTitleProbeChunksWhenLiveSessionsExceedChunkSize() =
        runTest {
            val n = 20
            val lsOut = (1..n).joinToString("") { "$projDir/sid-%02d.jsonl\n".format(it) }
            val pidOut =
                (1..n).joinToString("") {
                    "$rs/h/.claude/sessions/${100 + it}.json\n" +
                        """{"pid":${100 + it},"sessionId":"sid-%02d","cwd":"/home/u/proj"}""".format(it) + "\n"
                }
            val aliveOut = (1..n).joinToString("") { "${100 + it}\n" }
            val commands = mutableListOf<String>()
            val channel = fakeChannel(commands, lsOut = lsOut, pidOut = pidOut, aliveOut = aliveOut)
            val sessions = ClaudeSessionCatalog(channel, "/h/.claude").liveSessions("/home/u/proj")

            assertEquals("20 个活会话全部列出（活动集不截断）", n, sessions.size)
            assertTrue("全部标 live", sessions.all { it.live })
            val probes = commands.filter { it.startsWith("for f in '") && !it.contains("sessions/*.json") } // 排除 pidfiles
            assertEquals("标题探针分 2 片（15+5）", 2, probes.size)
            probes.forEach { cmd ->
                val paths = cmd.split(".jsonl'").size - 1
                assertTrue("单片 ≤${ClaudeSessionCatalog.HISTORY_TITLE_CHUNK} 文件（实际 $paths）", paths <= ClaudeSessionCatalog.HISTORY_TITLE_CHUNK)
            }
            assertEquals("片间不重不漏（总路径数=20）", n, probes.sumOf { it.split(".jsonl'").size - 1 })
        }

    @Test fun fallbackWhenPidfilesAllDeadAndListCapped() =
        runTest {
            val paths = (1..20).joinToString("") { "$projDir/sid-%02d.jsonl\n".format(it) }
            val channel =
                fakeChannel(
                    mutableListOf(),
                    lsOut = paths,
                    pidOut = "$rs/h/.claude/sessions/1.json\n" + """{"pid":1,"sessionId":"sid-99"}""" + "\n",
                    aliveOut = "", // pidfile 存在但全是尸体（Claude 没在跑的常态）
                )
            val sessions = ClaudeSessionCatalog(channel, "/h/.claude").liveSessions("/home/u/proj")
            assertEquals("兜底列表截前 N 条", ClaudeSessionCatalog.FALLBACK_MAX_SESSIONS, sessions.size)
            assertEquals("$projDir/sid-01.jsonl", sessions[0].path) // mtime 序保持
        }

    @Test fun liveSessionInOtherProjectFilteredByCwd() =
        runTest {
            val channel =
                fakeChannel(
                    mutableListOf(),
                    lsOut = "$projDir/aaaa-1111.jsonl\n",
                    pidOut =
                        "$rs/h/.claude/sessions/100.json\n" +
                            """{"pid":100,"sessionId":"aaaa-1111","cwd":"/home/u/OTHER"}""" + "\n",
                    aliveOut = "100\n",
                )
            val sessions = ClaudeSessionCatalog(channel, "/h/.claude").liveSessions("/home/u/proj")
            assertEquals(1, sessions.size)
            assertFalse("cwd 不匹配 → 不算本项目活动会话（走兜底列表）", sessions[0].live)
        }

    @Test fun pidfileNameUsedWhenJsonlHasNoTitle() =
        runTest {
            val channel =
                fakeChannel(
                    mutableListOf(),
                    lsOut = "$projDir/aaaa-1111.jsonl\n",
                    pidOut =
                        "$rs/h/.claude/sessions/100.json\n" +
                            """{"pid":100,"sessionId":"aaaa-1111","cwd":"/home/u/proj","name":"pidfile 里的 AI 标题"}""" + "\n",
                    aliveOut = "100\n",
                    titleOut = "$rs$projDir/aaaa-1111.jsonl\n", // JSONL 探针无素材
                )
            val sessions = ClaudeSessionCatalog(channel, "/h/.claude").liveSessions("/home/u/proj")
            assertEquals("pidfile 里的 AI 标题", sessions[0].title)
        }

    @Test fun titleProbeQuotesPathsInjectionSafeAndBounded() {
        val cmd = ClaudeSessionCatalog.titleProbeCommand(listOf("/p/a b/it's.jsonl"))!!
        assertTrue("单引号包裹+内部引号转义", cmd.contains("""'/p/a b/it'\''s.jsonl'"""))
        assertFalse("用 sed 取最后一条，不用 tail", cmd.contains("tail"))
        assertTrue("每行字节封顶（防超大 tool_result 整流回）", cmd.contains("cut -c -${ClaudeSessionCatalog.PROBE_LINE_CAP}"))
        assertTrue("user 行取前 8（compact 续接把首条真实输入推后）", cmd.contains("grep -a -m 8"))
        assertNull(ClaudeSessionCatalog.titleProbeCommand(emptyList()))
    }

    @Test fun pidfilesCommandBoundsPerFileBytes() {
        val cmd = ClaudeSessionCatalog.pidfilesCommand("/h/.claude")
        assertTrue("每 pidfile 字节封顶（防 sessions/ 误放超大文件）", cmd.contains("head -c ${ClaudeSessionCatalog.PIDFILE_BYTE_CAP}"))
        assertFalse("不用 cat（无界）", cmd.contains("cat \""))
    }

    // ---------- 专用 cwd 探针（片段兜底） ----------

    @Test fun titleProbeCommandCarriesCwdFragmentProbe() {
        val cmd = ClaudeSessionCatalog.titleProbeCommand(listOf("/p/a.jsonl"))!!
        assertTrue("每文件块追加 grep -o cwd 片段探针", cmd.contains("grep -a -m 1 -o '\"cwd\":\"[^\"]*\"' -- \"\$f\" 2>/dev/null"))
        assertTrue("命令仍以 for f in ' 开头（fake channel 分派契约不破）", cmd.startsWith("for f in '"))
        assertFalse("仍不得含 tail（fake 按子串分派）", cmd.contains("tail"))
    }

    @Test fun parseTitleProbeUsesCwdFragmentAsFallback() {
        // 超大 user 行被 cut 截断成半 JSON（不可解析）→ 完整记录解不出 cwd；裸片段行兜底。
        val raw =
            "$rs/p/big.jsonl\n" +
                """{"type":"user","message":{"role":"user","content":"标题素材","cwd":"/被截断""" + "\n" + // 截断的半行
                "\"cwd\":\"/home/u/真实项目\"\n" // grep -o 片段
        val parts = ClaudeSessionCatalog.parseTitleProbe(raw)
        assertEquals("截断大行也能从片段解出 cwd", "/home/u/真实项目", parts["/p/big.jsonl"]?.cwd)
    }

    @Test fun parseTitleProbeFullRecordCwdBeatsFragment() {
        // 正文伪 cwd 场景：片段可能来自正文文本（file 首个 "cwd":" 匹配）——完整 User 记录的 cwd 仍优先。
        val raw =
            "$rs/p/one.jsonl\n" +
                "\"cwd\":\"/正文伪造\"\n" +
                """{"type":"user","message":{"role":"user","content":"你好"},"cwd":"/home/u/proj"}""" + "\n"
        val parts = ClaudeSessionCatalog.parseTitleProbe(raw)
        assertEquals("完整记录 cwd 优先于裸片段", "/home/u/proj", parts["/p/one.jsonl"]?.cwd)
    }

    @Test fun parseTitleProbeFragmentDoesNotPolluteTitles() {
        // 片段行不是 JSONL 记录——不得被当成 user 记录混进标题素材（excerpt 仍取真 user 行）。
        val raw =
            "$rs/p/one.jsonl\n" +
                "\"cwd\":\"/home/u/proj\"\n" +
                """{"type":"user","message":{"role":"user","content":"修 bug"}}""" + "\n"
        val parts = ClaudeSessionCatalog.parseTitleProbe(raw)
        assertEquals("修 bug", parts["/p/one.jsonl"]?.excerpt)
        assertEquals("/home/u/proj", parts["/p/one.jsonl"]?.cwd)
    }

    @Test fun parseTitleProbeSplitsPerFileAndCarriesCwd() {
        val raw =
            "$rs/p/one.jsonl\n" +
                """{"type":"ai-title","aiTitle":"标题一","sessionId":"s1"}""" + "\n" +
                "$rs/p/two.jsonl\n" +
                """{"type":"user","message":{"role":"user","content":"你好"},"cwd":"/home/u/proj"}""" + "\n" +
                "半截 JSON 行被截断也不崩" + "\n"
        val map = ClaudeSessionCatalog.parseTitleProbe(raw)
        assertEquals("标题一", map["/p/one.jsonl"]?.ai)
        assertEquals("你好", map["/p/two.jsonl"]?.excerpt)
        assertEquals("cwd 随标题素材带出", "/home/u/proj", map["/p/two.jsonl"]?.cwd)
    }

    private suspend fun sessionsWithProcStart(alive: String) =
        ClaudeSessionCatalog(
            fakeChannel(
                mutableListOf(),
                lsOut = "$projDir/newest.jsonl\n$projDir/aaaa-1111.jsonl\n",
                pidOut =
                    "$rs/h/.claude/sessions/100.json\n" +
                        """{"pid":100,"sessionId":"aaaa-1111","cwd":"/home/u/proj","procStart":"777"}""" + "\n",
                aliveOut = alive,
            ),
            "/h/.claude",
        ).liveSessions("/home/u/proj")

    @Test fun liveSessionRequiresProcStartMatch() =
        runTest {
            // procStart 一致 → 活；procStart 不一致（PID 复用）→ 死 → 走兜底（不把死会话钉住）。
            val matched = sessionsWithProcStart("100 777\n") // starttime 匹配
            assertEquals(1, matched.size)
            assertTrue("procStart 一致 → 活动", matched[0].live && matched[0].path.endsWith("aaaa-1111.jsonl"))
            val reused = sessionsWithProcStart("100 999\n") // starttime 不符 = PID 复用 → 死
            assertEquals("复用 PID → 走兜底全列表（mtime 序）", 2, reused.size)
            assertFalse("死会话不标 live", reused.any { it.live })
        }

    @Test fun emptyProjectYieldsEmptyList() =
        runTest {
            val commands = mutableListOf<String>()
            val channel = fakeChannel(commands, lsOut = "")
            assertTrue(ClaudeSessionCatalog(channel, "/h/.claude").liveSessions(null).isEmpty())
            assertEquals("无 jsonl → 只发一条 ls，不再探 pidfile/标题", 1, commands.size)
        }

    // ---------- 连接不可用 ≠ 可信空 ----------

    @Test fun scanLiveSessionsThrowsWhenChannelUnavailable() =
        runTest {
            // channel 契约：无连接时 exec 流抛 IOException（这里是它的子类 [ConnectionDeadException]）。
            // scanLiveSessions 必须上抛（watcher runCatching → 整轮跳过、活 tail 保留；阅读面 .catch → Error 态），
            // 绝不返回 probeOk=true 的可信空集：那会被 watchTargets 判「可信零活会话」→ cancelStale 撤光活 tail
            // （重连后从 offset 0 全史重放）。
            val channel = RemoteCommandChannel { _ -> flow { throw ConnectionDeadException("SSH 连接不可用") } }
            val thrown = runCatching { ClaudeSessionCatalog(channel, "/h/.claude").scanLiveSessions(null) }.exceptionOrNull()
            assertTrue("连接不可用须上抛 IOException（ConnectionDeadException 子类兼容；实际 $thrown）", thrown is IOException)
        }

    @Test fun scanLiveSessionsTrustedEmptyOnlyWhenLsSucceeds() =
        runTest {
            // 对照组：ls 成功且真无 jsonl → 可信空（probeOk=true）→ watchTargets 才允许 cancelStale
            // （收敛已删文件的 tail，见 WatchTargetsTest.trustworthyEmptyRefsYieldNoTargetsButAllowsCancel）。
            val scan = ClaudeSessionCatalog(fakeChannel(mutableListOf(), lsOut = ""), "/h/.claude").scanLiveSessions(null)
            assertTrue(scan.refs.isEmpty())
            assertTrue("真空是可信空（probeOk=true）", scan.probeOk)
        }
}
