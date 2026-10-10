package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.ssh.SessionStatus
import com.ccmonitor.mobile.core.ssh.SshTransport
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/**
 * 终端 shell 的断因闩与自动重挂决策。
 *
 * 依赖只有 flow 与 lambda（[terminalDropped] 输入流、[liveConnection]/[shellOpenedOn] provider），
 * 不认识 SshConnectionManager / TerminalSession / uiState 的具体类型，因而能在纯 JVM 里驱动死链场景。
 * `terminalDropped` 由 VM 从 uiState 派生后传入。
 *
 * @param terminalDropped 当前 shell 流是否已断（VM 从 uiState 派生传入）。
 * @param status VM 的会话状态流——[reattachNeeded] 的 combine 借其变化（RECONNECTING→CONNECTED）驱动重查 live 连接。
 * @param liveConnection 读现有一条 live 连接（`= { manager.connection(hostId) }`）——重挂前提 + 断因基准之一。
 * @param shellOpenedOn 读当前 shell 开在哪条 transport（VM 持 @Volatile，经 provider 读）——断因分类基准。
 */
internal class SessionReattachDetector(
    scope: CoroutineScope,
    private val terminalDropped: StateFlow<Boolean>,
    status: StateFlow<SessionStatus?>,
    private val liveConnection: () -> SshTransport?,
    private val shellOpenedOn: () -> SshTransport?,
) {
    /**
     * 本次 shell 断是否为用户 exit。钉在断流上升沿那一刻（[init] 里的常驻收集器捕获），之后不随连接重建漂移。
     * 注意：exit 造成的断流一直挂着 `terminalDropped=true`；若在重挂求值时才比对当前连接，那条连接之后
     * 独立死掉又重建，会把 exit 误判成连接死，从而重挂用户已关掉的 shell。
     * 新 shell 建立时经 [notifyNewShellOpened] 复位。
     */
    private val shellExited = MutableStateFlow(false)

    /**
     * 是否需要自动重挂交互终端 shell：shell 已断（[terminalDropped]）、断因不是用户 exit（[shellExited]），
     * 且现有一条 live 连接可挂（后台重连已重建）。界面只在前台可见时重挂。
     * status 从 RECONNECTING 变到 CONNECTED 时重查 live 连接。重挂成功后 shellOpenedOn 换成新对象、
     * terminalDropped 回 false，本流随之落 false，不成环。
     */
    val reattachNeeded: StateFlow<Boolean> =
        combine(terminalDropped, shellExited, status) { dropped, exited, _ ->
            // 旁读现有 live 连接：后台重连必经 status RECONNECTING→CONNECTED，这一变化让 combine 重算并读到新连接。
            val live = liveConnection()
            shouldReattach(
                terminalDropped = dropped,
                userExited = exited,
                liveConnActive = live != null && live.isConnected,
            )
        }.distinctUntilChanged()
            .stateIn(scope, SharingStarted.WhileSubscribed(5_000), false)

    init {
        // 常驻收集器（不是 WhileSubscribed），在断流上升沿判定断因：那一刻 shell 的连接仍是当前活着的同一对象
        // （exit 只关通道、transport 未死）则判 exit，否则判连接死。常驻才能可靠捕获边沿并让结果持久。
        scope.launch {
            terminalDropped.collect { dropped ->
                val opened = shellOpenedOn()
                val live = liveConnection()
                // exit 特征：断的那一刻其连接仍是当前活着的同一对象（只关远端通道、SSH transport 未死）。
                val droppedOnSameLiveConn = opened != null && live === opened && live.isConnected
                // update{} 原子读改写，与 notifyNewShellOpened 的写不丢更新。
                shellExited.update { prev -> updateShellExited(prev, dropped, droppedOnSameLiveConn) }
            }
        }
    }

    /**
     * 新 shell 建立后清断因闩，免得沿用上一个 shell 的断因。
     * 由 [SessionViewModel.establishConnectedState] 在置 `shellOpenedOn` 之后调用。
     */
    fun notifyNewShellOpened() {
        shellExited.value = false
    }
}

/**
 * 更新「本次 shell 断是否用户 exit」闩。只在断流上升沿（`dropped=true`）重算，
 * 其余时候（`dropped=false`，新连或重连中）保持 prev，断因因此钉在断流那一刻。
 * @param droppedOnSameLiveConn 断的那一刻，shell 的连接仍是当前活着的同一对象（`live === shellOpenedOn && live.isConnected`）
 *   → exit 特征（exit 只关远端 shell 通道、SSH transport 未死）。连接已死/已换 → false（=因连接死而断）。
 */
internal fun updateShellExited(
    prev: Boolean,
    dropped: Boolean,
    droppedOnSameLiveConn: Boolean,
): Boolean = if (dropped) droppedOnSameLiveConn else prev

/**
 * 是否该自动重挂交互终端 shell。三条全满足才重挂：
 * @param terminalDropped 当前 shell 流已断（`TerminalSession.streamClosed`）。
 * @param userExited 本次断流被判为用户 exit（[updateShellExited] 钉在断流那一刻的闩）。
 * @param liveConnActive 现有一条 live 连接可挂（`manager.connection(id)?.isConnected == true`，即后台已重连）。
 *
 * 不变量：`userExited=true` 恒返回 false，绝不重挂用户主动关掉的 shell，哪怕那条连接之后独立死掉又重建。
 * 只有断因是连接死且现有 live 连接时才重挂。
 */
internal fun shouldReattach(
    terminalDropped: Boolean,
    userExited: Boolean,
    liveConnActive: Boolean,
): Boolean = terminalDropped && !userExited && liveConnActive
