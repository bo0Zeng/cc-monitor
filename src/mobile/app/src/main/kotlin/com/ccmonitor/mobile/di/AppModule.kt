package com.ccmonitor.mobile.di

import android.util.Log
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.ssh.KnownHostStore
import com.ccmonitor.mobile.dev.DevSeeder
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.ssh.HostConnector
import com.ccmonitor.mobile.ssh.KnownHostDaoStore
import com.ccmonitor.mobile.ui.chat.ChatController
import com.ccmonitor.mobile.ui.chat.ChatOpen
import com.ccmonitor.mobile.ui.chat.ChatViewModel
import com.ccmonitor.mobile.ui.chat.ConnectionHolder
import com.ccmonitor.mobile.ui.chat.sessionOf
import com.ccmonitor.mobile.ui.host.HostViewModel
import com.ccmonitor.mobile.ui.identity.IdentityViewModel
import com.ccmonitor.mobile.ui.overview.ConversationsViewModel
import com.ccmonitor.mobile.ui.session.SessionTabManager
import com.ccmonitor.mobile.ui.session.SessionViewModel
import com.ccmonitor.mobile.ui.settings.ButtonSettingsViewModel
import com.ccmonitor.mobile.ui.sftp.SftpViewModel
import org.koin.android.ext.koin.androidContext
import org.koin.androidx.viewmodel.dsl.viewModel
import org.koin.dsl.module

/** :app 的 Koin 模块（ViewModel + dev seeder）。 */
val appModule =
    module {
        // 聊天 VM 是薄壳，真身是 `ChatController`（single）持有的 `ChatSession`：下行不挂在 viewModelScope 上，
        // 离开聊天屏不会断。`key` 决定是哪个对话，同一对话在不同屏、不同 VM 实例之间必须给同一个 key。
        // 参数按类型取一件 `ChatOpen`，不按位置取几个裸串。
        viewModel {
            val open = it.get<ChatOpen>()
            val backends = get<HostBackends>()
            ChatViewModel(get<ChatController>().sessionOf(open, { h -> backends.of(h).also { f -> f.ensure() } }, backends::of))
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
            )
        }

        // 每台一条常驻流（进程级）；「对话」一屏与抽屉「最近」共用同一个清单 VM。参数顺序 = `parametersOf(hostId, machine)`。
        single {
            val hosts = get<HostRepository>()
            HostBackends(get(), labelOf = { id -> hosts.get(id)?.label ?: id })
        }
        viewModel { params ->
            ConversationsViewModel(backend = get<HostBackends>().of(params.get(0)), machine = params.get(1))
        }
        viewModel { IdentityViewModel(get()) }
        viewModel { ButtonSettingsViewModel(get()) } // get() = CustomButtonDao
        viewModel { HostViewModel(get(), get(), get(), get()) }
        // 终端连接编排 VM。位置参数与 `parametersOf(hostId, cd, launcherId, tmuxSession)`
        // 一一对应，空串是 null 的哨兵；按位取值是因为多参解构超过 detekt 上限。
        viewModel { params ->
            SessionViewModel(
                hostId = params.get(0),
                cdArg = params.get(1),
                launcherIdArg = params.get(2),
                tmuxArg = params.get(3),
                manager = get(),
                hostRepo = get(),
                identityRepo = get(),
                launcherRepo = get(),
                hostConnector = get(),
                backends = get(), // 这台的常驻流：终端那几件问那台核心
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
