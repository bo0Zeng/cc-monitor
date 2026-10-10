package com.ccmonitor.mobile.ui.overview

import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Scaffold
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ccmonitor.mobile.core.claude.transport.DaemonCommands
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.ccmonitor.mobile.core.ssh.SshConnectionManager
import com.ccmonitor.mobile.core.ssh.commandChannel
import com.ccmonitor.mobile.core.ui.theme.AppTheme
import kotlinx.coroutines.flow.flowOf
import org.koin.androidx.compose.koinViewModel
import org.koin.compose.koinInject
import org.koin.core.parameter.parametersOf

/**
 * 会话总览的隐藏入口，只在 debug 构建里，不进导航。
 *
 * ```
 * # 真 daemon（复用 app 里已有的活连接）
 * adb shell am start -n com.ccmonitor.mobile/.ui.overview.SessionOverviewActivity \
 *     -e host <hostId> -e daemon /path/to/remote-daemon
 *
 * # 桩（不连网，验分区/空态/降级三条渲染路径）
 * adb shell am start -n com.ccmonitor.mobile/.ui.overview.SessionOverviewActivity
 * ```
 *
 * 用来在真机上看：daemon 帧按三个分区画上屏，`attachable=false` 的行看得见但点不动，`superseded` 单列一区。
 */
class SessionOverviewActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            AppTheme {
                val manager = koinInject<SshConnectionManager>()
                val host = intent.getStringExtra("host")?.takeIf { it.isNotBlank() }
                val daemon = intent.getStringExtra("daemon")?.takeIf { it.isNotBlank() }
                // 没给 host/daemon ⇒ 用桩 channel 喂一段手写帧，三条渲染路径在真机上都走得到
                val channel: RemoteCommandChannel =
                    if (host != null && daemon != null) manager.commandChannel(host) else StubChannel
                val vm: SessionOverviewViewModel = koinViewModel { parametersOf(channel, daemon ?: STUB_PATH) }
                val state by vm.state.collectAsStateWithLifecycle()
                Scaffold { inner ->
                    SessionOverviewScreen(
                        state = state,
                        // 这里不路由，只看「点得动 / 点不动」
                        onOpen = { Toast.makeText(this, "打开 ${it.sessionId}", Toast.LENGTH_SHORT).show() },
                        onReconnect = vm::reconnect,
                        modifier = Modifier.fillMaxSize().padding(inner),
                    )
                }
            }
        }
    }

    private companion object {
        const val STUB_PATH = "/stub/remote-daemon"

        /** `/proj/one` 的目录名。`--list-sessions` 认的是它，不是真实路径。 */
        const val ONE_DIR = "-proj-one"

        /**
         * 只有 `/proj/one` 会被查询面翻到（`--list-projects` 只报它）。
         * `sessionCount = 7` 而流上加查询面只显示 2 条，组头应写「已显示 2 / 共 7」；
         * 别的项目的行只从流上来，组头一个数都不许出，这是真机上的阴性对照。
         */
        val PROJECTS_STUB =
            """{"dirName":"$ONE_DIR","projectPath":"/proj/one","sessionCount":7,"lastActivityMs":1786700000000}"""

        /** `/proj/one` 下的一条纯历史对话，没有记号（流上没提它）。 */
        const val SESSIONS_STUB =
            """{"aiTitle":"上周那条改按钮的","cwd":"/proj/one","jsonlPath":"/p/9999.jsonl",""" +
                """"messageCountApprox":38,"sessionId":"9999aaaa","startedAtMs":1786600000000,"updatedAtMs":1786600000000}"""

        /** 加上查询面的成功标记；少了它，读的那一层会把结果当「问不出来」。 */
        fun guarded(body: String): ByteArray = "$body\n${DaemonCommands.QUERY_OK_MARKER}\n".toByteArray()

        /**
         * 桩：直接吐一段 daemon 流帧。不模拟探测，`DaemonProbe` 会拿它当真回应，所以第一行必须是认得出的 `hello`。
         * 每一条真会发出去的命令都要有回应。
         */
        val StubChannel =
            RemoteCommandChannel { cmd ->
                // 分叉来源探针（`for f in …` 批量投影）：段标记 `\u001e<path>` 加分叉片段。
                if (cmd.startsWith("for f in ")) {
                    return@RemoteCommandChannel flowOf(
                        (
                            "\u001e/p/eeee5555.jsonl\n" +
                                // 注意：`grep -a -m 1 -o` 吐的是片段，不带外层花括号；写成完整记录行的话片段正则不匹配，树不出现。
                                // 单测的 fixture 是同一形状，两边要一起改。
                                "\"forkedFrom\":{\"sessionId\":\"cccc3333\",\"messageUuid\":\"u1\"}\n" +
                                "\u001e/p/aaaa1111.jsonl\n" +
                                "\u001e/p/cccc3333.jsonl\n" +
                                "\u001e/p/dddd4444.jsonl\n" +
                                "\u001e/p/bbbb2222.jsonl\n"
                        ).toByteArray(),
                    )
                }
                // 查询面也要答，否则组头说不出「已显示 / 共」。只让 `/proj/one` 被翻到，见 [PROJECTS_STUB]。
                if (cmd.contains("--list-projects")) {
                    return@RemoteCommandChannel flowOf(guarded(PROJECTS_STUB))
                }
                if (cmd.contains("--list-sessions")) {
                    return@RemoteCommandChannel flowOf(guarded(if (cmd.contains(ONE_DIR)) SESSIONS_STUB else ""))
                }
                flowOf(
                    (
                        listOf(
                            """{"kind":"hello","v":1,"build_id":"stub","capabilities":["bg"],"emits":["line"]}""",
                            // 每条都要带 `path`，否则分叉来源探针全部跳过、树画不出来。
                            // 点灯读的是 `activity`；单测 fixture 与这里要用同一种帧形状。
                            """{"kind":"session_added","sid":"aaaa1111","path":"/p/aaaa1111.jsonl","cwd":"/proj/one","name":"重构上行","activity":"working"}""",
                            """{"kind":"session_added","sid":"bbbb2222","path":"/p/bbbb2222.jsonl","cwd":"/proj/two","activity":"needs_you","waiting_for":"user"}""",
                            """{"kind":"session_added","sid":"cccc3333","path":"/p/cccc3333.jsonl","cwd":"/proj/three","activity":"idle","attachable":false}""",
                            """{"kind":"session_added","sid":"dddd4444","path":"/p/dddd4444.jsonl","cwd":"/proj/four","activity":"working"}""",
                            // 与 cccc3333 同为 idle、同一分区，树枝才画得出来（跨分区不缩进）
                            """{"kind":"session_added","sid":"eeee5555","path":"/p/eeee5555.jsonl","cwd":"/proj/three/fork","activity":"idle"}""",
                            """{"kind":"session_removed","sid":"dddd4444","cause":"superseded"}""",
                            """{"kind":"overflow","dropped":3}""",
                        ).joinToString("\n") + "\n"
                    ).toByteArray(),
                )
            }
    }
}
