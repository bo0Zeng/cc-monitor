package com.ccmonitor.mobile.ui.claude

import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.core.claude.model.ContentBlock
import com.ccmonitor.mobile.core.claude.model.JsonlRecord
import com.ccmonitor.mobile.core.claude.model.RecordClassifier
import com.ccmonitor.mobile.core.remote.ConnectionDeadException
import com.ccmonitor.mobile.core.remote.ExecResult
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.ccmonitor.mobile.core.remote.RemoteExecutor
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.io.IOException

/**
 * ReadingPaneViewModel 用 fake channel（无需 manager/真连接）在 JVM 上验：
 * ① 无 jsonl → Empty；② tail 续命——切走(停止收集)<5s 回来不重 tail、>5s 才重 tail（WhileSubscribed 的核心收益）。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ReadingPaneViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    /**
     * 截断检测用的 [RemoteExecutor] 替身。
     *
     * 返回 `exit=0` + 空 stdout ⇒ `toLongOrNull()` = null ⇒ 「问不出来」⇒ 保守沿用旧 offset、不复位。
     * 刻意不返回一个编出来的大小：那会让这些与截断无关的测试悄悄依赖一个假数字。
     */
    private fun noSizeExecutor() = RemoteExecutor { ExecResult(exitStatus = 0, stdout = "", stderrTail = "") }

    /**
     * `wc -c` 答一个真实形状的大小。
     *
     * 截断检测走 [RemoteExecutor]，不走 fake channel；同一件事只留一份假数据。
     * 输出形状照抄真 `wc -c < file`：纯数字 + 换行，退出码 0。
     */
    private fun sizeExecutor(bytes: Long) = RemoteExecutor { ExecResult(exitStatus = 0, stdout = "$bytes\n", stderrTail = "") }

    @Before
    fun setMain() = kotlinx.coroutines.Dispatchers.setMain(dispatcher)

    @After
    fun resetMainDispatcher() = kotlinx.coroutines.Dispatchers.resetMain()

    @Test
    fun emptyWhenNoJsonl() =
        runTest(dispatcher) {
            // ls 无输出 → discoverLatestJsonl 为 null → Empty。
            val channel = RemoteCommandChannel { _ -> emptyFlow() }
            val vm = ReadingPaneViewModel(channel, executor = noSizeExecutor(), cwdArg = null, cpu = dispatcher)
            val job = launch { vm.state.collect {} } // 订阅触发 WhileSubscribed 上游
            advanceUntilIdle()
            assertTrue("无 jsonl 应 Empty", vm.state.value is ClaudeReadState.Empty)
            job.cancel()
        }

    @Test
    fun tailEofMarksErrorAsConnectionDead() =
        runTest(dispatcher) {
            // tail -F 正常返回(EOF)=底层连接断 → Error.connectionDead=true（onRetry 据此驱动 SSH 重连、非仅重挂 tail）。
            val channel =
                RemoteCommandChannel { cmd ->
                    when {
                        cmd.startsWith("ls") -> flowOf("/p/abcdef12.jsonl\n".toByteArray())
                        cmd.contains("tail") -> flowOf("""{"type":"x"}""".toByteArray(), "\n".toByteArray()) // 一行后 EOF
                        else -> emptyFlow()
                    }
                }
            val vm = ReadingPaneViewModel(channel, executor = noSizeExecutor(), cwdArg = null, cpu = dispatcher)
            val job = launch { vm.state.collect {} }
            advanceTimeBy(500)
            runCurrent()
            val st = vm.state.value
            assertTrue("tail EOF 应 Error（实际 $st）", st is ClaudeReadState.Error)
            assertTrue("EOF 断连 → connectionDead=true", (st as ClaudeReadState.Error).connectionDead)
            job.cancel()
        }

    @Test
    fun connectionDeadExceptionSetsFlagPlainErrorDoesNot() =
        runTest(dispatcher) {
            // exec 抛 typed ConnectionDeadException → Error.connectionDead=true（reconnect）；抛泛 IOException → false（retry）。
            fun vmFailingTailWith(fail: Throwable): ReadingPaneViewModel {
                val channel =
                    RemoteCommandChannel { cmd ->
                        when {
                            cmd.startsWith("ls") -> flowOf("/p/abcdef12.jsonl\n".toByteArray())
                            cmd.contains("tail") -> flow<ByteArray> { throw fail }
                            else -> emptyFlow()
                        }
                    }
                return ReadingPaneViewModel(channel, executor = noSizeExecutor(), cwdArg = null, cpu = dispatcher)
            }
            val deadVm = vmFailingTailWith(ConnectionDeadException("dead"))
            val j1 = launch { deadVm.state.collect {} }
            advanceTimeBy(500)
            runCurrent()
            val dead = deadVm.state.value
            assertTrue("typed 断连 → Error（实际 $dead）", dead is ClaudeReadState.Error)
            assertTrue("ConnectionDeadException → connectionDead=true", (dead as ClaudeReadState.Error).connectionDead)
            j1.cancel()

            val transientVm = vmFailingTailWith(IOException("transient"))
            val j2 = launch { transientVm.state.collect {} }
            advanceTimeBy(500)
            runCurrent()
            val transient = transientVm.state.value
            assertTrue("泛错 → Error（实际 $transient）", transient is ClaudeReadState.Error)
            assertTrue("泛 IOException → connectionDead=false（可 retry）", !(transient as ClaudeReadState.Error).connectionDead)
            j2.cancel()
        }

    @Test
    fun tailSurvivesResubscribeWithin5sButRestartsAfter() =
        runTest(dispatcher) {
            var tailCalls = 0
            val channel =
                RemoteCommandChannel { cmd ->
                    when {
                        cmd.startsWith("ls") -> flowOf("/p/abcdef12.jsonl\n".toByteArray()) // 发现到一个会话
                        cmd.contains("tail") ->
                            flow {
                                tailCalls++
                                awaitCancellation() // 模拟 tail -F 无限挂着
                            }
                        else -> emptyFlow()
                    }
                }
            val vm = ReadingPaneViewModel(channel, executor = noSizeExecutor(), cwdArg = null, cpu = dispatcher)

            val j1 = launch { vm.state.collect {} }
            advanceTimeBy(50)
            runCurrent()
            assertEquals("首次订阅起 1 次 tail", 1, tailCalls)

            j1.cancel() // 切走（停止收集）
            advanceTimeBy(2_000) // < 5s 窗口
            runCurrent()
            val j2 = launch { vm.state.collect {} } // 5s 内回来
            advanceTimeBy(50)
            runCurrent()
            assertEquals("5s 内回来不重 tail（续命）", 1, tailCalls)

            j2.cancel()
            advanceTimeBy(6_000) // > 5s，上游被取消
            runCurrent()
            val j3 = launch { vm.state.collect {} } // 超 5s 后回来
            advanceTimeBy(50)
            runCurrent()
            assertEquals("超 5s 才重 tail", 2, tailCalls)
            j3.cancel()
        }

    @Test
    fun resumeUsesByteOffsetNotFullReread() =
        runTest(dispatcher) {
            // 首次 tail 吐 2 行后 EOF（断线）→ 记录 offset=16；>5s 重订阅 → resume 用 `tail -c +17` 不重下整份。
            val commands = mutableListOf<String>()
            var tailCalls = 0
            val channel =
                RemoteCommandChannel { cmd ->
                    commands.add(cmd)
                    when {
                        cmd.startsWith("ls") -> flowOf("/p/abcdef12.jsonl\n".toByteArray())
                        cmd.contains("tail") -> {
                            tailCalls++
                            if (tailCalls == 1) {
                                flowOf("""{"x":1}""".toByteArray(), "\n".toByteArray(), """{"x":2}""".toByteArray(), "\n".toByteArray())
                            } else {
                                flow { awaitCancellation() } // resume tail 挂住
                            }
                        }
                        else -> emptyFlow()
                    }
                }
            val vm = ReadingPaneViewModel(channel, executor = sizeExecutor(999), cwdArg = null, cpu = dispatcher)

            val j1 = launch { vm.state.collect {} }
            advanceTimeBy(300) // 首个 tail 吐 2 行(各 8 字节)后 EOF → offset=16、state=Error（不用 advanceUntilIdle：sample 定时器永不空闲）
            runCurrent()
            j1.cancel()
            advanceTimeBy(6_000) // >5s，上游取消
            runCurrent()
            val j2 = launch { vm.state.collect {} } // 重订阅 → resume
            advanceTimeBy(300)
            runCurrent()

            val tails = commands.filter { it.contains("tail -c") }
            assertEquals("两次 tail", 2, tails.size)
            assertTrue("首次全量 -c +1：${tails[0]}", tails[0].contains("tail -c +1 "))
            assertTrue("resume 用 offset+1=17：${tails[1]}", tails[1].contains("tail -c +17 "))
            j2.cancel()
        }

    // 断连不清零——tail 吐一条可渲染记录后 EOF（断线）→ state=Error(connectionDead) 但携「断开前最后一屏」
    // （lastUnits 非空），UI 据此保留内容 + 断开条、不空屏；重连（重订阅）自动续渲。
    @Test
    fun disconnectPreservesLastRenderedContentNotBlank() =
        runTest(dispatcher) {
            var tailCalls = 0
            val userLine = """{"type":"user","uuid":"u1","timestamp":"2026-01-01T00:00:00Z","message":{"role":"user","content":"hi"}}"""
            val channel =
                RemoteCommandChannel { cmd ->
                    when {
                        cmd.startsWith("ls") -> flowOf("/p/abcdef12.jsonl\n".toByteArray())
                        cmd.contains("tail") -> {
                            tailCalls++
                            if (tailCalls == 1) flowOf("$userLine\n".toByteArray()) else flow { awaitCancellation() }
                        }
                        else -> emptyFlow()
                    }
                }
            val vm = ReadingPaneViewModel(channel, executor = sizeExecutor(999), cwdArg = null, cpu = dispatcher)
            val j = launch { vm.state.collect {} }
            advanceTimeBy(300) // tail 吐一行后 EOF → 断线态
            runCurrent()
            val s = vm.state.value
            assertTrue("断线 → Error(connectionDead)：$s", s is ClaudeReadState.Error && s.connectionDead)
            assertTrue("断线保留断开前最后一屏（lastUnits 非空、不清零）", (s as ClaudeReadState.Error).lastUnits.isNotEmpty())
            j.cancel()
        }

    @Test
    fun retryReTriggersTailFromOffset() =
        runTest(dispatcher) {
            // 断线后 retry() 重挂 tail：不重下整份，从记下的 offset 续（tail -c +17）。
            val commands = mutableListOf<String>()
            var tailCalls = 0
            val channel =
                RemoteCommandChannel { cmd ->
                    commands.add(cmd)
                    when {
                        cmd.startsWith("ls") -> flowOf("/p/abcdef12.jsonl\n".toByteArray())
                        cmd.contains("tail") -> {
                            tailCalls++
                            if (tailCalls == 1) {
                                flowOf("""{"x":1}""".toByteArray(), "\n".toByteArray(), """{"x":2}""".toByteArray(), "\n".toByteArray())
                            } else {
                                flow { awaitCancellation() }
                            }
                        }
                        else -> emptyFlow()
                    }
                }
            val vm = ReadingPaneViewModel(channel, executor = sizeExecutor(999), cwdArg = null, cpu = dispatcher)
            val job = launch { vm.state.collect {} }
            advanceTimeBy(300) // 首个 tail EOF → offset=16
            runCurrent()
            vm.retry() // 一键重连（不需退出重进）
            advanceTimeBy(300)
            runCurrent()

            val tails = commands.filter { it.contains("tail -c") }
            assertEquals("retry 重挂一次 tail", 2, tails.size)
            assertTrue("retry 从 offset 续传 -c +17：${tails[1]}", tails[1].contains("tail -c +17 "))
            job.cancel()
        }

    @Test
    fun loadFullSessionResetsToFullReread() =
        runTest(dispatcher) {
            // 「载完整会话」放弃窗口/offset → 从 byte 0 全量重载（tail -c +1，对比 resume/retry 的 +17）。
            // 骨架命令返回空 → fallback 全量（windowed=false）；首 tail EOF 后 offset=16；loadFullSession → 复位 → +1。
            val commands = mutableListOf<String>()
            var tailCalls = 0
            val channel =
                RemoteCommandChannel { cmd ->
                    commands.add(cmd)
                    when {
                        cmd.startsWith("ls") -> flowOf("/p/abcdef12.jsonl\n".toByteArray())
                        cmd.contains("tail") -> {
                            tailCalls++
                            if (tailCalls == 1) {
                                flowOf("""{"x":1}""".toByteArray(), "\n".toByteArray(), """{"x":2}""".toByteArray(), "\n".toByteArray())
                            } else {
                                flow { awaitCancellation() }
                            }
                        }
                        else -> emptyFlow() // 骨架 awk → 空 → fallback 全量
                    }
                }
            val vm = ReadingPaneViewModel(channel, executor = sizeExecutor(999), cwdArg = null, cpu = dispatcher)
            val job = launch { vm.state.collect {} }
            advanceTimeBy(300) // 首个 tail EOF → offset=16
            runCurrent()
            vm.loadFullSession("/p/abcdef12.jsonl") // 载完整会话 → 复位 offset + 钉 path
            advanceTimeBy(300)
            runCurrent()

            val tails = commands.filter { it.contains("tail -c") }
            assertEquals("载完整重挂一次 tail", 2, tails.size)
            assertTrue("载完整从 0 全量 -c +1（非 resume 的 +17）：${tails[1]}", tails[1].contains("tail -c +1 "))
            job.cancel()
        }

    @Test
    fun truncationResetsToFullRereadNotResumeOffset() =
        runTest(dispatcher) {
            // resume 前 wc -c 得 size < 已记 offset(16) → 文件被重写(/compact) → 重置为 full re-read（-c +1，非 -c +17）。
            val commands = mutableListOf<String>()
            var tailCalls = 0
            val channel =
                RemoteCommandChannel { cmd ->
                    commands.add(cmd)
                    when {
                        cmd.startsWith("ls") -> flowOf("/p/abcdef12.jsonl\n".toByteArray())
                        cmd.contains("tail") -> {
                            tailCalls++
                            if (tailCalls == 1) {
                                flowOf("""{"x":1}""".toByteArray(), "\n".toByteArray(), """{"x":2}""".toByteArray(), "\n".toByteArray())
                            } else {
                                flow { awaitCancellation() }
                            }
                        }
                        else -> emptyFlow()
                    }
                }
            val vm = ReadingPaneViewModel(channel, executor = sizeExecutor(3), cwdArg = null, cpu = dispatcher)

            val j1 = launch { vm.state.collect {} }
            advanceTimeBy(300)
            runCurrent()
            j1.cancel()
            advanceTimeBy(6_000)
            runCurrent()
            val j2 = launch { vm.state.collect {} }
            advanceTimeBy(300)
            runCurrent()

            val tails = commands.filter { it.contains("tail -c") }
            assertEquals("两次 tail", 2, tails.size)
            assertTrue("截断→重置回全量 -c +1（非 -c +17）：${tails[1]}", tails[1].contains("tail -c +1 "))
            j2.cancel()
        }

    @Test
    fun liveSessionOverridesNewestMtimeAndShowsReadableTitle() =
        runTest(dispatcher) {
            // pidfile 判活：较旧 mtime 的会话正被 Claude 跑（活 pidfile 指向它）→ tail 目标是活动会话
            // 而非最新 mtime；下拉标签用可读标题（ai-title），不是 UUID8。
            val commands = mutableListOf<String>()
            val proj = "/h/.claude/projects/-home-u-proj"
            val rs = "\u001E"
            val channel =
                RemoteCommandChannel { cmd ->
                    commands.add(cmd)
                    when {
                        // newest0 = 最新 mtime（列在前），live1111 = 较旧但正被跑
                        cmd.startsWith("ls") -> flowOf("$proj/newest0.jsonl\n$proj/live1111.jsonl\n".toByteArray())
                        cmd.contains("sessions/*.json") ->
                            flowOf(
                                ("$rs/h/.claude/sessions/500.json\n" + """{"pid":500,"sessionId":"live1111","cwd":"/home/u/proj"}""" + "\n").toByteArray(),
                            )
                        cmd.startsWith("for p in") -> flowOf("500\n".toByteArray()) // 500 活（无 procStart → 存在性即可）
                        cmd.startsWith("for f in '") ->
                            flowOf(
                                ("$rs$proj/live1111.jsonl\n" + """{"type":"ai-title","aiTitle":"重构传输层","sessionId":"live1111"}""" + "\n").toByteArray(),
                            )
                        cmd.contains("tail") -> flow { awaitCancellation() }
                        else -> emptyFlow()
                    }
                }
            val vm = ReadingPaneViewModel(channel, executor = noSizeExecutor(), cwdArg = "/home/u/proj", cpu = dispatcher)
            val job = launch { vm.state.collect {} }
            advanceTimeBy(300)
            runCurrent()

            val tails = commands.filter { it.contains("tail -c") }
            assertTrue("有 tail", tails.isNotEmpty())
            assertTrue("tail 目标 = 活动会话(live1111) 非最新 mtime(newest0)：${tails.first()}", tails.first().contains("live1111.jsonl") && !tails.first().contains("newest0"))
            val sess = vm.sessions.value
            assertEquals("下拉只列活动会话", 1, sess.size)
            assertEquals("$proj/live1111.jsonl", sess[0].path)
            assertEquals("可读标题（ai-title），非 UUID8", "重构传输层", sess[0].label)
            // 活会话 sid 进 liveSessionIds：阅读器「▶ 续接」活会话守卫的数据源（防双写 JSONL 叉毁）。
            assertEquals("活会话 sid 集", setOf("live1111"), vm.liveSessionIds.value)
            job.cancel()
        }

    // ---------- 钉住任意历史 JSONL（历史页「点击=阅读」） ----------

    @Test
    fun pinnedInitTailsPinnedPathAndMergesDiscovery() =
        runTest(dispatcher) {
            // pinned 非空 → init 即钉住（selectedPath 预置）；tail 目标=钉住 path（不走发现选路）；
            // 下拉 = 钉住项（label=传入标题）在前 + 发现列表并入。
            val commands = mutableListOf<String>()
            val channel =
                RemoteCommandChannel { cmd ->
                    commands.add(cmd)
                    when {
                        cmd.startsWith("ls") -> flowOf("/p/other999.jsonl\n".toByteArray())
                        cmd.contains("tail") -> flow { awaitCancellation() }
                        else -> emptyFlow()
                    }
                }
            val vm =
                ReadingPaneViewModel(
                    channel,
                    executor = sizeExecutor(999),
                    cwdArg = null,
                    pinnedPathArg = "/p/hist1234-abcd.jsonl",
                    pinnedLabelArg = "修复登录",
                    cpu = dispatcher,
                )
            assertEquals("init 即钉住（不等订阅/发现）", "/p/hist1234-abcd.jsonl", vm.selectedPath.value)
            val job = launch { vm.state.collect {} }
            advanceTimeBy(300)
            runCurrent()

            val tails = commands.filter { it.contains("tail -c") }
            assertTrue("有 tail", tails.isNotEmpty())
            assertTrue(
                "tail 目标=钉住 path（非发现的最新 other999）：${tails.first()}",
                tails.first().contains("hist1234-abcd.jsonl") && !tails.first().contains("other999"),
            )
            val sess = vm.sessions.value
            assertEquals("钉住项在前", "/p/hist1234-abcd.jsonl", sess.first().path)
            assertEquals("钉住项 label=历史页标题", "修复登录", sess.first().label)
            assertTrue("发现列表并入", sess.any { it.path == "/p/other999.jsonl" })
            job.cancel()
        }

    @Test
    fun pinnedKeepsSingleItemWhenDiscoveryFails() =
        runTest(dispatcher) {
            // 发现（ls）失败 → 下拉保底钉住单项；阅读（tail 钉住 path）不受影响。
            val commands = mutableListOf<String>()
            val channel =
                RemoteCommandChannel { cmd ->
                    commands.add(cmd)
                    when {
                        cmd.startsWith("ls") -> flow { throw java.io.IOException("连接断") }
                        cmd.contains("tail") -> flow { awaitCancellation() }
                        else -> emptyFlow()
                    }
                }
            val vm = ReadingPaneViewModel(channel, executor = sizeExecutor(999), cwdArg = null, pinnedPathArg = "/p/hist.jsonl", pinnedLabelArg = "标题", cpu = dispatcher)
            val job = launch { vm.state.collect {} }
            advanceTimeBy(300)
            runCurrent()

            assertEquals("发现失败 → 保底钉住单项", listOf(JsonlSession("/p/hist.jsonl", "标题")), vm.sessions.value)
            assertTrue("tail 钉住 path 照常", commands.any { it.contains("tail -c") && it.contains("hist.jsonl") })
            job.cancel()
        }

    @Test
    fun pinnedLoadFullSessionRereadsFromZero() =
        runTest(dispatcher) {
            // 钉住会话的「载完整会话」→ 从 byte 0 全量（tail -c +1，非续传 offset）。
            val commands = mutableListOf<String>()
            var tailCalls = 0
            val channel =
                RemoteCommandChannel { cmd ->
                    commands.add(cmd)
                    when {
                        cmd.startsWith("ls") -> flowOf("/p/hist.jsonl\n".toByteArray())
                        cmd.contains("tail") -> {
                            tailCalls++
                            if (tailCalls == 1) {
                                flowOf("""{"x":1}""".toByteArray(), "\n".toByteArray(), """{"x":2}""".toByteArray(), "\n".toByteArray())
                            } else {
                                flow { awaitCancellation() }
                            }
                        }
                        else -> emptyFlow() // 骨架 awk → 空 → fallback 全量
                    }
                }
            val vm = ReadingPaneViewModel(channel, executor = sizeExecutor(999), cwdArg = null, pinnedPathArg = "/p/hist.jsonl", pinnedLabelArg = "标题", cpu = dispatcher)
            val job = launch { vm.state.collect {} }
            advanceTimeBy(300) // 首个 tail EOF → offset=16
            runCurrent()
            vm.loadFullSession("/p/hist.jsonl")
            advanceTimeBy(300)
            runCurrent()

            val tails = commands.filter { it.contains("tail -c") }
            assertEquals("载完整重挂一次 tail", 2, tails.size)
            assertTrue("从 0 全量 -c +1：${tails[1]}", tails[1].contains("tail -c +1 "))
            assertTrue("目标仍是钉住 path", tails[1].contains("hist.jsonl"))
            job.cancel()
        }

    // ---------- 窗口模式 tail 追加去重（防 LazyColumn items(key) 撞 key 崩溃） ----------

    @Test
    fun windowedTailDuplicateUuidYieldsNoDuplicateUnitKeys() =
        runTest(dispatcher) {
            // 大会话（>200 条主线 → windowed=true）被 /compact 原地重写后，tail -F 检测截断会从头重放
            // 窗口内同 uuid 的记录（骨架 END 缺失回退重读末行同理）——窗口模式若不按 uuid 保首见去重，
            // 同 key RenderUnit 直达 LazyColumn items(key) → IllegalArgumentException 崩。
            val n = 201 // > WINDOW_RECORDS(200) → windowStartOffset>0 → windowed=true

            fun ts(i: Int) = "2026-01-01T00:00:00.%03dZ".format(i)

            fun userJson(i: Int) =
                """{"type":"user","uuid":"u$i","parentUuid":"u${i - 1}","timestamp":"${ts(i)}",""" +
                    """"message":{"role":"user","content":"w$i"}}"""
            val skeleton =
                buildString {
                    for (i in 0 until n) {
                        append("${i * 100}\tu$i\t${if (i == 0) "" else "u${i - 1}"}\tuser\t${ts(i)}\t0\n")
                    }
                    append("${n * 100}\n") // END：扫描时刻 EOF（tail 从此续接）
                }
            val channel =
                RemoteCommandChannel { cmd ->
                    when {
                        cmd.startsWith("ls") -> flowOf("/p/abcdef12.jsonl\n".toByteArray())
                        cmd.startsWith("LC_ALL=C awk") -> flowOf(skeleton.toByteArray()) // 骨架扫描
                        // 窗口正文（LC_ALL=C tail -c，无 -F）：回窗口尾部两条即可（窗口内已有 u199/u200）
                        cmd.startsWith("LC_ALL=C tail") -> flowOf((userJson(199) + "\n" + userJson(200) + "\n").toByteArray())
                        cmd.contains("tail") ->
                            flow {
                                delay(100) // 虚拟时间：模拟新行晚于管线就位到达（真实网络行为；避免 trigger 订阅前 tryEmit 被丢）
                                emit((userJson(200) + "\n").toByteArray()) // 截断重放：与窗口内同 uuid
                                emit((userJson(201) + "\n").toByteArray()) // 新记录照常追加
                                awaitCancellation()
                            }
                        else -> emptyFlow()
                    }
                }
            val vm = ReadingPaneViewModel(channel, executor = noSizeExecutor(), cwdArg = null, cpu = dispatcher)
            val job = launch { vm.state.collect {} }
            advanceTimeBy(1_000)
            runCurrent()

            val ready = vm.state.value as? ClaudeReadState.Ready
            assertTrue("应为 Ready（实际 ${vm.state.value}）", ready != null)
            assertTrue("201 条主线 → 窗口模式", ready!!.windowed)
            val keys = ready.units.map { it.key }
            assertEquals("units key 不得重复（LazyColumn items(key) 契约）：$keys", keys.size, keys.toSet().size)
            assertEquals("同 uuid 重放保首见（只一个 u200 unit）", 1, keys.count { it == "u200#0" })
            assertTrue("新记录 u201 照常追加", keys.contains("u201#0"))
            job.cancel()
        }

    // ---------- 断连 ≠「未找到会话」 ----------

    @Test
    fun discoveryChannelFailureYieldsErrorNotEmpty() =
        runTest(dispatcher) {
            // 连接不可用（channel 契约：exec 流抛 typed ConnectionDeadException，不用 emptyFlow 伪装「成功且输出空」）→
            // 状态须是 Error（可点「重连」），不得误报 Empty——「未找到 Claude 会话」是 ls 成功且真无 jsonl 的语义。
            val channel = RemoteCommandChannel { _ -> flow { throw ConnectionDeadException("SSH 连接不可用") } }
            val vm = ReadingPaneViewModel(channel, executor = noSizeExecutor(), cwdArg = null, cpu = dispatcher)
            val job = launch { vm.state.collect {} }
            advanceUntilIdle()
            assertTrue("断连应 Error 而非 Empty：${vm.state.value}", vm.state.value is ClaudeReadState.Error)
            job.cancel()
        }

    // ---------- Codex 渲染 per-kind（无 parentUuid → 文件序 + 合成 uuid；用量 per-kind） ----------

    @Test
    fun codexRendersMultipleNullUuidRecordsWithUniqueKeys() =
        runTest(dispatcher) {
            // Codex 记录多无 uuid（payload.id 不稳）：3 条渲染记录若不补合成 uuid 会全撞 "_#0"
            // → LazyColumn items(key) 崩。补 cx-<index> 后各成唯一 key。同时用量走 Codex 聚合（contextWindow/去缓存 input）。
            fun env(
                ts: String,
                payload: String,
            ) = """{"timestamp":"$ts","type":"response_item","payload":$payload}"""
            val userMsg = """{"type":"message","role":"user","content":[{"type":"input_text","text":"帮我看看"}]}"""
            val asstMsg = """{"type":"message","role":"assistant","content":[{"type":"output_text","text":"好的"}]}"""
            val user2Msg = """{"type":"message","role":"user","content":[{"type":"input_text","text":"继续"}]}"""
            val tokenCount =
                """{"timestamp":"2026-07-18T22:45:03Z","type":"event_msg","payload":{"type":"token_count","info":""" +
                    """{"total_token_usage":{"input_tokens":100,"cached_input_tokens":40,"output_tokens":10,""" +
                    """"reasoning_output_tokens":5,"total_tokens":110},"last_token_usage":{"total_tokens":80},""" +
                    """"model_context_window":258400}}}"""
            val lines =
                env("2026-07-18T22:45:00Z", userMsg) + "\n" +
                    env("2026-07-18T22:45:01Z", asstMsg) + "\n" +
                    env("2026-07-18T22:45:02Z", user2Msg) + "\n" +
                    tokenCount + "\n"
            val codexPath = "/h/.codex/sessions/2026/07/18/rollout-2026-07-18T22-45-32-019f78e8-84dd-7ac0-b479-e9c1b9caec67.jsonl"
            val channel =
                RemoteCommandChannel { cmd ->
                    when {
                        cmd.contains("tail") ->
                            flow {
                                delay(100) // 虚拟时间：让新行晚于管线就位（trigger 无 replay，订阅前 tryEmit 会被丢）
                                emit(lines.toByteArray())
                                awaitCancellation() // 保持流开着（别 EOF→Error）
                            }
                        else -> emptyFlow() // Codex 跳骨架；发现(ls/pidfile)用 Claude catalog→空（pinned 不依赖）
                    }
                }
            val vm =
                ReadingPaneViewModel(
                    channel,
                    executor = noSizeExecutor(),
                    cwdArg = null,
                    pinnedPathArg = codexPath,
                    pinnedLabelArg = "codex 会话",
                    agentKindArg = AgentKind.Codex,
                    cpu = dispatcher,
                )
            val job = launch { vm.state.collect {} }
            advanceTimeBy(500)
            runCurrent()

            val ready = vm.state.value as? ClaudeReadState.Ready
            assertTrue("应 Ready（实际 ${vm.state.value}）", ready != null)
            val keys = ready!!.units.map { it.key }
            assertEquals("3 条渲染记录（2 user + 1 assistant）", 3, keys.size)
            assertEquals("无 null-uuid 撞键（unitKey 唯一，否则 LazyColumn 崩）", keys.size, keys.toSet().size)
            assertTrue("无 \"_#0\" 占位键（已补合成 uuid）", keys.none { it.startsWith("_#") })
            // 用量走 Codex 聚合：contextWindow=真上限、input 去缓存、cacheRead=cached。
            val usage = ready.usage
            assertEquals("Codex 真上下文上限", 258_400L, usage?.contextWindow)
            assertEquals("当前占用=last_token_usage.total", 80L, usage?.lastContextTokens) // Long? → 需 L
            assertEquals("input 去缓存 100-40", 60L, usage?.input)
            assertEquals("cacheRead=cached_input", 40L, usage?.cacheRead)
            job.cancel()
        }

    @Test
    fun codexTailUsesFullReadNoSkeleton() =
        runTest(dispatcher) {
            // Codex 无 parentUuid → 跳骨架窗口首载（那靠 MainBranch）→ 直接全量 tail -c +1（非骨架/窗口）。
            val commands = mutableListOf<String>()
            val codexPath = "/h/.codex/sessions/2026/07/18/rollout-x-019f78e8-84dd-7ac0-b479-e9c1b9caec67.jsonl"
            val channel =
                RemoteCommandChannel { cmd ->
                    commands.add(cmd)
                    when {
                        cmd.contains("tail") -> flow { awaitCancellation() }
                        else -> emptyFlow()
                    }
                }
            val vm =
                ReadingPaneViewModel(channel, executor = noSizeExecutor(), cwdArg = null, pinnedPathArg = codexPath, agentKindArg = AgentKind.Codex, cpu = dispatcher)
            val job = launch { vm.state.collect {} }
            advanceTimeBy(300)
            runCurrent()

            assertTrue("Codex 不跑骨架 awk 命令", commands.none { it.contains("awk") })
            val tails = commands.filter { it.contains("tail -c") }
            assertTrue("Codex 全量 tail -c +1（非窗口 offset）：$tails", tails.isNotEmpty() && tails.first().contains("tail -c +1 "))
            job.cancel()
        }

    @Test
    fun codexResumeTargetCwdFromSessionMeta() =
        runTest(dispatcher) {
            // Codex cwd 在 session_meta（User 无 cwd）→ resumeTarget.cwd 从 session_meta 提；sessionId=末尾 UUID。
            val uuid = "019f78e8-84dd-7ac0-b479-e9c1b9caec67"
            val codexPath = "/h/.codex/sessions/2026/07/18/rollout-2026-07-18T22-45-32-$uuid.jsonl"
            val meta = """{"timestamp":"t","type":"session_meta","payload":{"id":"$uuid","cwd":"/home/u/proj"}}"""
            val userMsg = """{"timestamp":"t","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hi"}]}}"""
            val channel =
                RemoteCommandChannel { cmd ->
                    when {
                        cmd.contains("tail") ->
                            flow {
                                delay(100)
                                emit((meta + "\n" + userMsg + "\n").toByteArray())
                                awaitCancellation()
                            }
                        else -> emptyFlow()
                    }
                }
            val vm = ReadingPaneViewModel(channel, executor = noSizeExecutor(), cwdArg = null, pinnedPathArg = codexPath, agentKindArg = AgentKind.Codex, cpu = dispatcher)
            val job = launch { vm.state.collect {} }
            advanceTimeBy(500)
            runCurrent()
            val rt = vm.resumeTarget.value
            assertEquals("sessionId=末尾 UUID", uuid, rt?.sessionId)
            assertEquals("cwd 从 session_meta（Codex User 无 cwd）", "/home/u/proj", rt?.cwd)
            job.cancel()
        }

    @Test
    fun resumeTargetFromFilenameAndJsonlCwd() =
        runTest(dispatcher) {
            // resume 目标——sessionId 取文件名去后缀、cwd 取 JSONL User 记录的 cwd 字段（权威）。
            val channel =
                RemoteCommandChannel { cmd ->
                    when {
                        cmd.startsWith("ls") -> flowOf("/p/abcdef12.jsonl\n".toByteArray())
                        cmd.contains("tail") ->
                            flow {
                                emit("""{"type":"user","uuid":"u1","cwd":"/home/pi/proj"}""".toByteArray())
                                emit("\n".toByteArray())
                                awaitCancellation() // 保持流开着，别 EOF
                            }
                        else -> emptyFlow()
                    }
                }
            val vm = ReadingPaneViewModel(channel, executor = noSizeExecutor(), cwdArg = null, cpu = dispatcher)
            val job = launch { vm.state.collect {} }
            advanceTimeBy(300)
            runCurrent()

            val rt = vm.resumeTarget.value
            assertEquals("sessionId = 文件名去 .jsonl", "abcdef12", rt?.sessionId)
            assertEquals("cwd = JSONL cwd 字段", "/home/pi/proj", rt?.cwd)
            job.cancel()
        }

    /**
     * 上滑翻一页只取被请求的那段字节，绝不重下整份。
     *
     * 这是这个功能存在的全部理由：会话能大到几百 MB，在那种会话上「全量重下」等于不可用，也不省流量。
     *
     * 判据是发出去的命令：翻页命令必须是有界的 `tail -c +N | head -c LEN`，
     * 且不许出现 `tail -c +1`（那是 `loadFullSession` 的全量重下）。
     */
    @Test
    fun scrollUpPagingFetchesOnlyTheRequestedRangeNeverTheWholeFile() =
        runTest(dispatcher) {
            val commands = mutableListOf<String>()
            // 骨架：4 条主线记录，byteOffset 0/100/200/300，EOF=400
            val skeleton =
                buildString {
                    listOf(0, 100, 200, 300).forEachIndexed { i, off ->
                        append("$off\tu$i\t\tassistant\t2026-08-01T00:0$i:00Z\t\n")
                    }
                    append("400\t\t\t\t\t\n")
                }
            val channel =
                RemoteCommandChannel { cmd ->
                    commands.add(cmd)
                    when {
                        cmd.startsWith("ls") -> flowOf("/p/abcdef12.jsonl\n".toByteArray())
                        cmd.contains("awk") -> flowOf(skeleton.toByteArray())
                        else -> flow { awaitCancellation() }
                    }
                }
            val vm = ReadingPaneViewModel(channel, executor = sizeExecutor(400), cwdArg = null, cpu = dispatcher)
            val job = launch { vm.state.collect {} }
            advanceTimeBy(300)
            runCurrent()

            val beforePaging = commands.size
            vm.loadOlderHistory("/p/abcdef12.jsonl")
            advanceTimeBy(300)
            runCurrent()

            val newCommands = commands.drop(beforePaging)
            assertTrue("翻页应真的发了命令：$newCommands", newCommands.isNotEmpty())
            val ranged = newCommands.filter { it.contains("head -c") }
            assertTrue("翻页必须用有界区间命令（tail -c +N | head -c LEN）：$newCommands", ranged.isNotEmpty())
            assertTrue(
                "翻页绝不许从头重下（tail -c +1）——那是 loadFullSession 的行为：$newCommands",
                newCommands.none { it.contains("tail -c +1 ") },
            )
            job.cancel()
        }

    /**
     * 单次 prepend 的渲染单元数不许超 100：`LazyColumn` 的 key 锚定有窗口边界。
     *
     * key→index 映射表只覆盖锚点附近的窗口（`NearestItemsSlidingWindowSize=30` /
     * `NearestItemsExtraItemCount=100`），单次 prepend ≤100 项安全、≥130 项会失败（视图跳）。
     *
     * 分页切的是 record，而渲染单位是 unit（一条 record 展开多个）：
     * 工具密集的一页 40 条 record 完全可能产出 100+ 个 unit。
     * 这条喂一页会爆预算的记录，断言裁剪后的单元数落在预算内。
     */
    @Test
    fun aSingleOlderPageNeverExceedsTheLazyColumnAnchoringBudget() {
        // 造一页「工具密集」的记录：每条 assistant 带 1 段正文 + 3 个工具调用 ⇒ 每条产 4 个 unit
        val records =
            (0 until 60).map { i ->
                JsonlRecord.Assistant(
                    uuid = "a$i",
                    parentUuid = if (i == 0) null else "a${i - 1}",
                    blocks =
                        listOf(ContentBlock.Text("回答 $i")) +
                            (0 until 3).map { t -> ContentBlock.ToolUse("toolu_${i}_$t", "Bash", emptyMap()) },
                    model = "m",
                    timestamp = "2026-08-01T00:00:00Z",
                    sessionId = null,
                    isApiError = false,
                )
            }
        val full = RecordClassifier.classify(records).size
        assertTrue("前提：这一页必须真的超预算（实际 $full 个单元），否则本测试什么都没测", full > MAX_PREPEND_UNITS)

        val kept = trimToPrependBudget(records)
        val keptUnits = RecordClassifier.classify(kept).size
        assertTrue("裁剪后必须落进预算：$keptUnits", keptUnits <= MAX_PREPEND_UNITS)
        assertTrue("不能裁到空 —— 那样翻页就永远前进不了", kept.isNotEmpty())
        assertEquals("必须保留靠近当前窗口的那一截（列表尾部）", records.last().uuid, kept.last().uuid)
    }
}
