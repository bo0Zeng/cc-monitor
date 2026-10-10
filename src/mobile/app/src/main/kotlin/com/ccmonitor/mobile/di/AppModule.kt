package com.ccmonitor.mobile.di

import android.util.Log
import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import com.ccmonitor.mobile.core.claude.bridge.UplinkSink
import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.core.claude.transport.DaemonConversationReader
import com.ccmonitor.mobile.core.claude.transport.DaemonHistoryReader
import com.ccmonitor.mobile.core.claude.transport.DaemonSessionSource
import com.ccmonitor.mobile.core.claude.transport.SessionSignals
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.ccmonitor.mobile.core.remote.RemoteExecutor
import com.ccmonitor.mobile.core.ssh.KnownHostStore
import com.ccmonitor.mobile.dev.DevSeeder
import com.ccmonitor.mobile.ssh.HostConnector
import com.ccmonitor.mobile.ssh.KnownHostDaoStore
import com.ccmonitor.mobile.ui.chat.ChatController
import com.ccmonitor.mobile.ui.chat.ChatSessionKey
import com.ccmonitor.mobile.ui.chat.ChatViewModel
import com.ccmonitor.mobile.ui.chat.ClaudeSessionId
import com.ccmonitor.mobile.ui.chat.ConnectionHolder
import com.ccmonitor.mobile.ui.chat.HostId
import com.ccmonitor.mobile.ui.claude.ReadingPaneViewModel
import com.ccmonitor.mobile.ui.host.HostViewModel
import com.ccmonitor.mobile.ui.identity.IdentityViewModel
import com.ccmonitor.mobile.ui.overview.SessionOverviewViewModel
import com.ccmonitor.mobile.ui.session.SessionTabManager
import com.ccmonitor.mobile.ui.session.SessionViewModel
import com.ccmonitor.mobile.ui.settings.ButtonSettingsViewModel
import com.ccmonitor.mobile.ui.settings.SettingsViewModel
import com.ccmonitor.mobile.ui.sftp.SftpViewModel
import org.koin.android.ext.koin.androidContext
import org.koin.androidx.viewmodel.dsl.viewModel
import org.koin.dsl.module

/** :app 的 Koin 模块（ViewModel + dev seeder）。 */
val appModule =
    module {
        // 聊天 VM 是薄壳，真身是 `ChatController`（single）持有的 `ChatSession`：下行不挂在 viewModelScope 上，
        // 离开聊天屏不会断。`key` 决定是哪个对话，同一对话在不同屏、不同 VM 实例之间必须给同一个 key。
        //
        // 参数一律按类型取，不按位置解构：谁在 `parametersOf` 前面多插一个参数，位置解构会拿错参数，
        // 类型恰好兼容时还是静默错配。uplink 收参数而不是 `getOrNull()` 全局解析，调用方决定接哪条上行。
        viewModel {
            // 按类型取（`ChatSessionKey` 是 value class）；裸 String 会和别的字符串参数静默错配。
            val key = it.getOrNull<ChatSessionKey>()?.value ?: ChatSessionKey.DEFAULT
            ChatViewModel(
                get<ChatController>().sessionFor(
                    key = key,
                    uplink = it.getOrNull<UplinkSink>(),
                    // 带上主机，对话才持有得住那条连接
                    hostId = it.getOrNull<HostId>()?.value,
                    // 跨通路信号汇按 Claude 自己的对话编号索引，不是上面那个 `key`
                    claudeSessionId = it.getOrNull<ClaudeSessionId>()?.value,
                ),
            )
        }
        // 应用级的对话持有者，tail 活在这里，不活在屏幕里。逐出要记日志，否则「对话怎么没了」无从查起。
        single {
            // 持有和放手连接都经 HostConnector，DI 层不直接拿连接管理器。
            val connector = get<HostConnector>()
            ChatController(
                connections =
                    object : ConnectionHolder {
                        override fun retain(
                            hostId: String,
                            holder: String,
                        ) {
                            connector.retain(hostId, holder)
                        }

                        override fun release(
                            hostId: String,
                            holder: String,
                        ) {
                            connector.release(hostId, holder)
                        }
                    },
                onEvicted = { key, total ->
                    Log.w("ChatController", "对话数超过 ${ChatController.MAX_SESSIONS}，收掉最旧的一个（key=$key，累计 $total）")
                },
                // 跨通路信号汇，进程级，两个屏共用一个。
                signals = get(),
            )
        }

        // 跨通路信号汇：聊天通路和总览通路之间的桥。聊天通路投配额与「被挡住了」，总览通路每收到一份 daemon 快照
        // 投一次等待态。必须是 single：两个屏拿到不同实例时，投进去的信号在另一边永远看不见。
        single { SessionSignals() }
        // 会话总览。参数按类型取；daemon 路径由调用方给。
        viewModel {
            val channel = it.get<RemoteCommandChannel>()
            val daemonPath = it.get<String>()
            SessionOverviewViewModel(
                source = DaemonSessionSource(channel, daemonPath),
                // 上面那个 single，与聊天屏共用。
                signals = get(),
                // 分叉来源（`forkedFrom`）走批量投影探针，是裸 shell 的 `for f in …`，不经 daemon，只要 channel。
                history = DaemonHistoryReader(channel),
                // 列表来源是 daemon 的一次性查询面：流上宣告的只是一小部分会话，而且流量大得多。
                conversations = DaemonConversationReader(channel, daemonPath),
            )
        }
        viewModel { IdentityViewModel(get()) }
        viewModel { SettingsViewModel(get()) }
        viewModel { ButtonSettingsViewModel(get()) } // get() = CustomButtonDao
        viewModel { HostViewModel(get(), get(), get()) }
        // 阅读面 VM。位置参数与 `parametersOf(channel, cwd, claudeDir, pinnedPath, pinnedLabel)` 一一对应，
        // 空串是 null 的哨兵；按位取值是因为 5 参解构超过 detekt 上限。
        viewModel { params ->
            ReadingPaneViewModel(
                channel = params.get(0),
                // 按类型取，且用 `get` 不用 `getOrNull`：调用方漏传就在构造时炸，不悄悄让截断检测失效。
                executor = params.get<RemoteExecutor>(),
                cwdArg = params.get(1),
                claudeDirArg = params.get(2),
                pinnedPathArg = params.get(3),
                pinnedLabelArg = params.get(4),
                // agent 种类，按类型取；缺省走档案的缺省档。
                agentKindArg = params.getOrNull<AgentKind>() ?: AgentProfile.DEFAULT.kind,
            )
        }
        // 终端连接编排 VM。位置参数与 `parametersOf(hostId, cd, launcherId, tmuxSession, resumeSessionId, resumeCommand)`
        // 一一对应，空串是 null 的哨兵；按位取值是因为多参解构超过 detekt 上限。
        viewModel { params ->
            SessionViewModel(
                hostId = params.get(0),
                cdArg = params.get(1),
                launcherIdArg = params.get(2),
                tmuxArg = params.get(3),
                resumeArg = params.get(4),
                resumeCommandArg = params.get(5),
                manager = get(),
                hostRepo = get(),
                identityRepo = get(),
                launcherRepo = get(),
                settingsRepo = get(),
                hostConnector = get(),
                tabManager = get(), // resume 失败时重新驱动用的句柄注册表
                knownHostStore = get(), // TOFU 恢复路径
            )
        }
        // SFTP 连接编排和浏览状态 VM；与终端共享同一主机连接，按持有者计数保活。
        viewModel { (hostId: String) -> SftpViewModel(hostId, get(), get()) }
        // core-ssh 的 KnownHostStore 端口接 Room；sshModule 里的连接管理器靠它解析。
        single<KnownHostStore> { KnownHostDaoStore(get()) } // get()=KnownHostDao（dataModule 提供）
        single { HostConnector(get(), get(), get()) }
        single { SessionTabManager() } // 多 tab 的应用级状态：打开的会话和按 key 保活的 ViewModelStore
        single { DevSeeder(androidContext(), get(), get(), get(), get()) }
    }
