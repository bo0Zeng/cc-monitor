package com.ccmonitor.mobile.ui.chat

import android.content.Context
import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Scaffold
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ccmonitor.mobile.core.claude.bridge.BridgeFrame
import com.ccmonitor.mobile.core.claude.bridge.Pacing
import com.ccmonitor.mobile.core.claude.bridge.PipeSession
import com.ccmonitor.mobile.core.claude.bridge.PipeUplinkSink
import com.ccmonitor.mobile.core.claude.bridge.ReplayRow
import com.ccmonitor.mobile.core.claude.bridge.ReplayTransport
import com.ccmonitor.mobile.core.claude.bridge.ReplayVectors
import com.ccmonitor.mobile.core.claude.bridge.UplinkSink
import com.ccmonitor.mobile.core.ssh.SshConnectionManager
import com.ccmonitor.mobile.core.ssh.commandChannel
import com.ccmonitor.mobile.core.ui.theme.AppTheme
import com.ccmonitor.mobile.ssh.PipeLauncher
import com.ccmonitor.mobile.ssh.TmuxBackend
import com.ccmonitor.mobile.ssh.TmuxSendKeysSink
import org.koin.androidx.compose.koinViewModel
import org.koin.compose.koinInject
import org.koin.core.parameter.parametersOf

/**
 * 聊天面重放的隐藏入口，只在 debug 构建里，不进导航。
 *
 * ```
 * adb shell am start -n com.ccmonitor.mobile/.ui.chat.ChatReplayActivity           # 默认 long-reply
 * adb shell am start -n com.ccmonitor.mobile/.ui.chat.ChatReplayActivity -e case demo-debug
 *
 * # 接真上行（复用 app 里已有的活连接；会话名是 tmux 里跑着 Claude 的那个会话）
 * adb shell am start -n com.ccmonitor.mobile/.ui.chat.ChatReplayActivity -e host <hostId> -e tmux cc-abc12345
 *
 * # 走常驻管道（下行 tail `out.ndjson`，上行追加 `in.ndjson`）
 * #   会先起那条管道，已在跑就什么都不做。`-e launch cct` 可指定启动命令
 * adb shell am start -n com.ccmonitor.mobile/.ui.chat.ChatReplayActivity -e host <hostId> -e pipe <sessionId> \\
 *     -e cwd /home/me/proj      # Claude 在哪个项目里干活（不给就在登录目录，看不见项目）
 * ```
 *
 * 不带 host 时全程零网络，把重放帧画上屏。帧来自 `bridge/vectors/`（debug 变体把那个目录直接挂成 assets）。
 */
class ChatReplayActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val case = intent.getStringExtra("case") ?: DEFAULT_CASE
        setContent {
            AppTheme {
                // 上行三选一，见 [pickSink]。VM 走 Koin（`koinViewModel` + `parametersOf`），与生产同一条装配路径。
                val manager = koinInject<SshConnectionManager>()
                // `-e pipe <sid>` 走常驻管道，与 `-e tmux` 是两条路：管道不回显，tmux 里的 TUI 会回显。
                val pipeSid = intent.getStringExtra("pipe")?.takeIf { it.isNotBlank() }
                val pipeHost = intent.getStringExtra("host")?.takeIf { it.isNotBlank() }
                val pipe =
                    remember(manager, pipeSid, pipeHost) {
                        if (pipeSid != null && pipeHost != null) {
                            PipeSession(manager.commandChannel(pipeHost), pipeSid)
                        } else {
                            null
                        }
                    }
                val sink = remember(manager, pipe) { pickSink(manager, pipe, pipeHost) }
                val vm: ChatViewModel = koinViewModel { parametersOf(sink) }
                val rows = remember(case) { loadVector(this, case) }
                LaunchedEffect(case) {
                    // 把重放帧切成「最近一段」和若干「更早的页」，好在真机上演示上滑翻历史。
                    val recentFrom = (rows.size - RECENT_FRAMES).coerceAtLeast(0)
                    // 管道模式下先把管道起起来（幂等）；起不来要说出来，不能留一个永远没有下行的屏。
                    if (pipe != null && pipeHost != null && pipeSid != null) {
                        startPipe(manager, pipeHost, pipeSid)
                    }
                    // 管道模式下行来自真 `out.ndjson`，不是重放帧
                    vm.start(
                        pipe?.frames() ?: ReplayTransport(withInitFrame(rows, recentFrom), Pacing.Original).frames(),
                        prompt = if (pipe != null) null else promptOf(case),
                    )
                    if (recentFrom > 0 && pipe == null) {
                        // 用游标从后往前切，不用 `chunked()`：那样最后一块只有两三帧，`removeLastOrNull()` 先给的就是它，
                        // 而那几帧和尾段属同一条消息，新增 0 条，一上滑就说「已是最早」。
                        var cursor = recentFrom
                        vm.attachHistoryPaging {
                            if (cursor <= 0) {
                                null // 给完了 ⇒ UI 显示「已是最早」
                            } else {
                                val from = (cursor - PAGE_FRAMES).coerceAtLeast(0)
                                rows.subList(from, cursor).map { it.frame }.also { cursor = from }
                            }
                        }
                    }
                }
                val state by vm.state.collectAsStateWithLifecycle()
                Scaffold { inner ->
                    ChatScreen(
                        state = state,
                        onSend = vm::send,
                        onRetrySend = vm::retry,
                        onStop = vm::stop,
                        onLoadOlder = vm::loadOlder,
                        modifier = Modifier.fillMaxSize().padding(inner),
                    )
                }
            }
        }
    }

    /**
     * 起那条常驻管道，起不来要说出来。
     *
     * 注意：这里的 `Toast` 只是 debug 入口的权宜，与全应用统一的反馈通道不一致，别带进生产。
     */
    private suspend fun startPipe(
        manager: SshConnectionManager,
        pipeHost: String,
        pipeSid: String,
    ) {
        PipeLauncher
            .start(
                channel = manager.commandChannel(pipeHost),
                backend = TmuxBackend,
                sessionId = pipeSid,
                launchCommand = intent.getStringExtra("launch"),
                // 给了 `-e cwd <项目绝对路径>` 就让 Claude 在那儿干活，否则项目的 CLAUDE.md 和代码都看不见。
                workdir = intent.getStringExtra("cwd"),
            )?.let { Toast.makeText(this, "管道没起来：$it", Toast.LENGTH_LONG).show() }
    }

    /**
     * 三条上行路的分派，三者并列：管道（`in.ndjson`，不回显）、tmux（`send-keys`，TUI 会回显）、假 sink（重放演示）。
     */
    private fun pickSink(
        manager: SshConnectionManager,
        pipe: PipeSession?,
        pipeHost: String?,
    ): UplinkSink {
        val target = intent.getStringExtra("tmux")?.takeIf { it.isNotBlank() }
        val hostId = intent.getStringExtra("host")?.takeIf { it.isNotBlank() }
        return when {
            // 注意：这条 debug 路的探活是恒真桩，一律说「在听」，所以没有「管道被杀后不谎报已送达」那道门；
            // 起管道也不过账号门。这是 `-e` 参数驱动的调试入口，别把桩带进生产（生产那条在 `ChatRoute.ensureListeningFor`）。
            pipe != null && pipeHost != null -> PipeUplinkSink(manager.commandChannel(pipeHost), pipe) { null }
            // 复用 app 里已有的活连接；没有活连接时 `exec` 抛 `ConnectionDeadException`，成为可重试的拒绝。
            target != null && hostId != null -> TmuxSendKeysSink(manager.commandChannel(hostId), target)
            else -> DemoUplinkSink()
        }
    }

    private companion object {
        const val DEFAULT_CASE = "long-reply"

        /** 首屏只放最近这么多帧，其余留给上滑翻历史。 */
        const val RECENT_FRAMES = 40
        const val PAGE_FRAMES = 25

        /**
         * 把 `init` 帧补回首屏。`init` 是第 0 帧，首屏只取最后 [RECENT_FRAMES] 帧，不补的话命令选择器要翻到会话开头才出现。
         * `init` 是连接时就到的会话元数据（模型、cwd、catalog），与翻了多少历史无关，补回来不失真。
         * 时刻沿用首屏第一帧的，免得 [Pacing.Original] 按原始 t_ns 睡掉一大段。
         */
        fun withInitFrame(
            rows: List<ReplayRow>,
            recentFrom: Int,
        ): List<ReplayRow> {
            val recent = rows.drop(recentFrom)
            if (recent.any { it.frame is BridgeFrame.Init }) return recent
            val init = rows.firstOrNull { it.frame is BridgeFrame.Init } ?: return recent
            return listOf(init.copy(tNs = recent.firstOrNull()?.tNs ?: init.tNs)) + recent
        }

        fun loadVector(
            ctx: Context,
            case: String,
        ): List<ReplayRow> =
            ctx.assets.open("$case.frames.ndjson").bufferedReader().useLines {
                ReplayVectors.parse(it)
            }

        /** 演示素材自带提问，技术 case 没有 —— 上屏一句用户消息，画面才像一段对话。 */
        fun promptOf(case: String): String? =
            if (case == "demo-debug") "跑测试发现 total_value 算出来不对，帮我看看是怎么回事。" else null
    }
}
