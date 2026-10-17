package com.ccmonitor.mobile.di

import android.util.Log
import com.ccmonitor.mobile.cache.LocalCache
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.dev.DevSeeder
import com.ccmonitor.mobile.link.BackendFiles
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.ssh.HostConnector
import com.ccmonitor.mobile.ssh.OldHostKeys
import com.ccmonitor.mobile.ui.chat.ChatController
import com.ccmonitor.mobile.ui.chat.ChatOpen
import com.ccmonitor.mobile.ui.chat.ChatViewModel
import com.ccmonitor.mobile.ui.chat.ConnectionHolder
import com.ccmonitor.mobile.ui.chat.sessionOf
import com.ccmonitor.mobile.ui.chats.ChatsBoard
import com.ccmonitor.mobile.ui.chats.HostName
import com.ccmonitor.mobile.ui.chats.RecentChats
import com.ccmonitor.mobile.ui.chats.hostChats
import com.ccmonitor.mobile.ui.files.FilesViewModel
import com.ccmonitor.mobile.ui.host.HostViewModel
import com.ccmonitor.mobile.ui.identity.IdentityViewModel
import com.ccmonitor.mobile.ui.session.SessionTabManager
import com.ccmonitor.mobile.ui.session.SessionViewModel
import com.ccmonitor.mobile.ui.session.keys.KeysEditorViewModel
import kotlinx.coroutines.flow.map
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
            val cache = get<LocalCache>()
            ChatViewModel(get<ChatController>().sessionOf(open, { h -> backends.of(h).also { f -> f.ensure() } }, backends::of, cache::chat))
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

        // 每台一条常驻流（进程级）；会话屏每台一份清单、抽屉「最近」一份，都经它问那台。
        single {
            val hosts = get<HostRepository>()
            val files = BackendFiles(androidContext())
            val cache = get<LocalCache>()
            HostBackends(get(), labelOf = { id -> hosts.get(id)?.label ?: id }, keptOf = cache::batches, backendSlots = files::slots)
        }
        // 会话屏：参数 ＝ `parametersOf(当前那台)`；机器表照库里的先后。
        viewModel { params ->
            val make = hostChats(get(), get())
            ChatsBoard(params.get(0), get<HostRepository>().observeAll().map { hs -> hs.map { HostName(it.id, it.label.ifBlank { it.id }) } }, make)
        }
        // 抽屉「最近」：参数 ＝ `parametersOf(那台, 它的名字)`。
        viewModel { params ->
            val make = hostChats(get(), get())
            val host = HostName(params.get(0), params.get(1))
            RecentChats { scope -> make(host, scope) }
        }
        // 本地缓存（会话列表 · 记录页 · 续点 · 待发 · 草稿 · 在做的一组）：那台成品原样落 Room。
        single { LocalCache(get()) }
        viewModel { IdentityViewModel(get()) }
        viewModel { KeysEditorViewModel(get(), get()) }
        viewModel { HostViewModel(get(), get(), get()) }
        // 终端连接编排 VM。位置参数与 `parametersOf(hostId, cd, tmuxSession, resume)`
        // 一一对应，空串是 null 的哨兵；按位取值是因为多参解构超过 detekt 上限。
        viewModel { params ->
            SessionViewModel(
                hostId = params.get(0),
                cdArg = params.get(1),
                tmuxArg = params.get(2),
                resumeArg = params.get(3),
                manager = get(),
                hostRepo = get(),
                identityRepo = get(),
                movedRepo = get(), // 迁移说明那一格：手机原来存的启动配置，每台列一次
                hostConnector = get(),
                backends = get(), // 这台的常驻流：终端那几件问那台核心
            )
        }
        // 文件屏的浏览状态 VM：读写问那台后端（这台的常驻流）；SSH 连接与终端共用，按持有者计数保活。
        viewModel { (hostId: String) -> FilesViewModel(hostId, get(), get(), get()) }
        single { HostConnector(get(), get(), get()) }
        single { OldHostKeys(get()) } // 主机钥匙旧表迁进核心（只活这一版）：迁移对不上那一句与它的出路
        single { SessionTabManager() } // 多 tab 的应用级状态：打开的会话和按 key 保活的 ViewModelStore
        single { DevSeeder(androidContext(), get(), get()) }
    }
