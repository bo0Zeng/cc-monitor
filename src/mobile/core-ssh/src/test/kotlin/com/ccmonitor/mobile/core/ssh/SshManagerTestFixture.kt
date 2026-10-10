package com.ccmonitor.mobile.core.ssh

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope

/**
 * `SshConnectionManagerTest` 与 [SshConnectionRefcountTest] 的共享夹具：两个测试类必须造出同一个 manager。
 */
@OptIn(ExperimentalCoroutinesApi::class)
internal fun TestScope.testManager(
    produced: MutableList<FakeTransport>? = null,
    behavior: suspend FakeTransport.(ConnectionConfig) -> Unit = {},
): SshConnectionManager =
    SshConnectionManager(
        knownHostStore = null,
        transportFactory = { FakeTransport(behavior).also { produced?.add(it) } },
        reconnectDispatcher = StandardTestDispatcher(testScheduler),
    )

internal fun testCfg(host: String) =
    ConnectionConfig(host = host, port = 22, username = "u", auth = AuthMethod.Password("p"))

internal fun testCfgP(
    host: String,
    policy: ReconnectPolicy,
) = ConnectionConfig(host = host, port = 22, username = "u", auth = AuthMethod.Password("p"), reconnect = policy)
