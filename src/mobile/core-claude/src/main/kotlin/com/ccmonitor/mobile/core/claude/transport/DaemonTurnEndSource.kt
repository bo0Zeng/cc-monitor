package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.ProducerScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.channelFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

/**
 * α 这条路这一刻为什么不能用。五档，处置各不同，不许压成一句：
 *
 * | 档 | 谁产出 | 还会变好吗 |
 * |---|---|---|
 * | [NotSpeaking] | [DaemonTurnEndSource] | 会（装上了 / 路径填对了 / 网回来了）⇒ 退避重试 |
 * | [NoTailOnly] | [DaemonTurnEndSource] | 要升级对端 ⇒ 退避重试（上限兜着浪费） |
 * | [NoTurnEndFrames] | [DaemonTurnEndSource] | 同上 |
 * | [StreamDied] | [DaemonTurnEndSource] | 会（抖一下）⇒ 退避重试 |
 * | [NotClaudeHost] | 调用方（它才知道这台主机配的是哪一家） | 不会 ⇒ 别再探了 |
 */
enum class AlphaUnavailable {
    /**
     * ① 探测拿不到 `hello`：没装 / 路径不对 / 装了但不说话 / 超时 / 通道挂了。
     * 表现一样（什么都没有），`DaemonProbe` 那句诊断原样带出来。
     */
    NotSpeaking,

    /**
     * ② 对端的 `capabilities` 里没有 `tail-only` ⇒ 不起 α。
     *
     * 不带 `--tail-only` 时，后端在连上那一刻把每个 jsonl 从头走一遍，每见一条 turn-end 记录就发一帧
     * `TurnEnd`；[TurnEndDebouncer] 会把它折成每条活会话一条假的「完成了」。
     * flag 是按 `want ∩ capabilities` 拼的，对端不声明时会被静默丢掉、命令看起来一切正常，
     * 所以必须在这一侧显式拦。
     */
    NoTailOnly,

    /**
     * ③ 对端 `hello.emits` 里没有 `turn_end` ⇒ 它不会发我们唯一要的那种帧。
     *
     * 消费方按 [JsonlFrame.Hello.emits] 门控，缺则回退 β。旧后端的 hello 没有 `emits` ⇒ 空集 ⇒ 落本档：
     * 「它没说」与「它说了不发」一视同仁。
     */
    NoTurnEndFrames,

    /** ④ 起来过，然后断了（SSH 断 / 对端退出）。正常结束也算断。 */
    StreamDied,

    /**
     * ⑤ 这台主机配的不是 Claude。后端的流式 watcher 只跟记录树那一家（Claude），轮次边沿按
     * `stop_reason=="end_turn"` 判，所以不会为 Codex 发 `turn_end`；β 会（Codex 那支认 `task_complete`）。
     * Codex 主机留在 β 上，不必再探。本档由调用方产出，[DaemonTurnEndSource] 不知道主机配了哪一家。
     */
    NotClaudeHost,
}

/**
 * α 这条流的 turn-end 消费方：探测 → 能力门控 → 起流 → 折成「完成一轮」事件。
 *
 * 与 [DaemonSessionSource] 各起一条流是有意的：总览面那条跟着屏的生死，
 * 前台服务这条跟着连接的生死。
 *
 * 流量：后端对每条新 jsonl 行照样发一帧 `line`（没有 flag 能关掉它），`turn_end` 是附加的边沿信号。
 * 所以 α 相比 β 买到的是：进程死后不再从 0 重下整份（游标在对端、`--tail-only` prime 到 EOF）；
 * 一台主机一条流替掉多条 tail；每轮流量约省一半（`line` 带的是解析后的 `message`，比原文小）。
 *
 * 时钟由调用方喂（[events] 的 `nowMs`）。Android 上传 `SystemClock::elapsedRealtime`（单调、算睡眠时间）。
 *
 * 空闲时零唤醒：没有待结算的轮次就阻塞在 `receive()`；有的才挂一个到最早那条成熟为止的超时
 * （[TurnEndRoute.msUntilNextRipe]）。固定节拍的 ticker 在整夜没人说话的机器上是纯耗电。
 */
class DaemonTurnEndSource(
    /**
     * 叫 `remote` 而不是 `channel`：流体是 `channelFlow`，`ProducerScope` 自己有个成员叫 `channel`，
     * 在流体内写 `channel` 会解析成那个 `SendChannel`。
     */
    private val remote: RemoteCommandChannel,
    private val daemonPath: String,
) {
    /** 本流对外只说三件事。 */
    sealed interface Event {
        /** α 真的接上了（能力门全过）。[command] 带出来给日志，诊断时第一个要看它。 */
        data class Engaged(
            val hello: JsonlFrame.Hello,
            val command: String,
        ) : Event

        /** 一轮结束了。 */
        data class Round(
            val done: TurnEndRoute.Done,
        ) : Event

        /**
         * α 这一刻用不了。本事件之后流一定结束，调用方据此回落 β。
         *
         * 绝不静默结束：无声停掉的 α 会让服务以为 α 还在供货，β 一直不起来，完成通知再也不来。
         */
        data class Unavailable(
            val why: AlphaUnavailable,
            val detail: String,
        ) : Event
    }

    /** 单消费者循环的输入：一帧，或者流到头了。两者走同一个口，所以 [TurnEndRoute] 只被一个协程碰。 */
    private sealed interface Signal {
        data class Arrived(
            val frame: JsonlFrame,
        ) : Signal

        data class Ended(
            val error: Throwable?,
        ) : Signal
    }

    /**
     * 连上后端并持续发「完成一轮」事件。
     *
     * @param probeTimeoutMs 等 `hello` 的上限。冷启（SSH + 后端扫 `~/.claude`）比稳态慢得多。
     * @param settleMs 静默多久算一轮结束，默认与 β 同一个常量（[TurnEndDebouncer.SETTLE_MS]）。
     * @param nowMs 时钟。见类头注。
     */
    fun events(
        probeTimeoutMs: Long = PROBE_TIMEOUT_MS,
        settleMs: Long = TurnEndDebouncer.SETTLE_MS,
        nowMs: () -> Long = System::currentTimeMillis,
    ): Flow<Event> =
        channelFlow {
            val probe = DaemonProbe(remote, daemonPath)
            val (cmd, result) = probe.streamCommand(probeTimeoutMs, WANT)
            val hello = result.hello
            if (cmd == null || hello == null) {
                send(
                    Event.Unavailable(
                        AlphaUnavailable.NotSpeaking,
                        result.failure ?: "daemon 探测失败（无更多信息）",
                    ),
                )
                return@channelFlow
            }
            // 先 tail-only 后 emits：两档都不过时先报「会被灌假通知」那条，它是会做错事，后者只是用不了。
            if (!result.capabilities.contains(DaemonCommands.CAP_TAIL_ONLY)) {
                send(
                    Event.Unavailable(
                        AlphaUnavailable.NoTailOnly,
                        "这台后端（${hello.buildId ?: "版本号说不出"}）不声明 " +
                            "`${DaemonCommands.CAP_TAIL_ONLY}`，挂上去会被灌一串历史轮次 ⇒ 不起 α。" +
                            "它声明的是：${result.capabilities}",
                    ),
                )
                return@channelFlow
            }
            if (!hello.emits.contains(EMIT_TURN_END)) {
                send(
                    Event.Unavailable(
                        AlphaUnavailable.NoTurnEndFrames,
                        "这台后端（${hello.buildId ?: "版本号说不出"}）没声明会发 `$EMIT_TURN_END` 帧。" +
                            "它声明的是：${hello.emits}",
                    ),
                )
                return@channelFlow
            }
            send(Event.Engaged(hello, cmd))
            pump(cmd, settleMs, nowMs)
        }

    /**
     * 能力门过了之后那一段：一个生产者（搬帧）＋ 一个消费者（折事件）。
     *
     * [TurnEndRoute] 只在消费者那一边被碰，否则是数据竞争，表现为偶尔少一条通知。
     * 所以帧与「流到头了」走同一个 [Signal] 口。
     */
    private suspend fun ProducerScope<Event>.pump(
        cmd: String,
        settleMs: Long,
        nowMs: () -> Long,
    ) {
        val route = TurnEndRoute(settleMs)
        val signals = Channel<Signal>(Channel.UNLIMITED)
        val feeder =
            launch {
                try {
                    DaemonTransport(remote, cmd).frames().collect { signals.send(Signal.Arrived(it)) }
                    signals.send(Signal.Ended(null))
                } catch (e: kotlin.coroutines.cancellation.CancellationException) {
                    // 取消不是故障，必须向上传播。
                    throw e
                } catch (e: java.io.IOException) {
                    signals.send(Signal.Ended(e))
                }
            }
        try {
            while (true) {
                val waitMs = route.msUntilNextRipe(nowMs())
                val signal =
                    if (waitMs == null) {
                        signals.receive()
                    } else {
                        withTimeoutOrNull(waitMs) { signals.receive() }
                    }
                if (signal == null) {
                    // 静默满窗（最早那条成熟了）。
                    route.settle(nowMs()).forEach { send(Event.Round(it)) }
                    continue
                }
                when (signal) {
                    is Signal.Arrived -> {
                        route.onFrame(signal.frame, nowMs())
                        // 顺手结算一次：别的会话可能在这条帧到达之前就已经静默满窗了。
                        route.settle(nowMs()).forEach { send(Event.Round(it)) }
                    }

                    is Signal.Ended -> {
                        // 先把没结算的倒出来再说「不能用了」，不倒就是漏通知。
                        route.flushAll(nowMs()).forEach { send(Event.Round(it)) }
                        val e = signal.error
                        send(
                            Event.Unavailable(
                                AlphaUnavailable.StreamDied,
                                if (e == null) {
                                    "daemon 流已结束（对端退出）。命令：$cmd"
                                } else {
                                    "daemon 流中断：${e::class.simpleName}: ${e.message}。命令：$cmd"
                                },
                            ),
                        )
                        return
                    }
                }
            }
        } finally {
            feeder.cancel()
        }
    }

    companion object {
        /** `hello.emits` 里那个帧型名。 */
        const val EMIT_TURN_END: String = "turn_end"

        /**
         * 等 `hello` 的默认上限，比总览面的 15 秒长：本流跑在前台服务里、没人盯着转圈，
         * 等久一点换一次接上比早早回落 β（更费流量）划算。
         */
        const val PROBE_TIMEOUT_MS: Long = 20_000L

        /**
         * 要的能力。
         *
         * - `tail-only`：硬要求（没有就不起 α，见 [AlphaUnavailable.NoTailOnly]）。
         * - `bg`：要，但不是硬要求。不要它的话后台会话不被宣告、也没有 `turn_end`，而 β 是监视后台会话的；
         *   不声明 `bg` 的后端上 α 覆盖面窄一点，但不为此拒起 α（整台退回 β 更贵）。
         */
        val WANT: List<String> = listOf(DaemonCommands.CAP_BG, DaemonCommands.CAP_TAIL_ONLY)
    }
}
