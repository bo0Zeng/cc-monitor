package com.ccmonitor.mobile.core.ssh

import com.ccmonitor.mobile.core.wire.gen.DialRequest
import com.ccmonitor.mobile.core.wire.gen.DialUse
import com.ccmonitor.mobile.core.wire.gen.Endpoint
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
        transportFactory = { FakeTransport(behavior).also { produced?.add(it) } },
        reconnectDispatcher = StandardTestDispatcher(testScheduler),
    )

internal fun testCfg(host: String) = testCfgP(host, ReconnectPolicy())

/** 一台直连的那一份（核心组好的形状：首选地址一条、`keyPath` 写那台的编号）＋ 那台的密码。 */
internal fun testCfgP(
    host: String,
    policy: ReconnectPolicy,
) = ConnectionConfig(
    request =
        DialRequest(
            host = host,
            port = 22,
            user = "u",
            keyPath = host,
            command = "",
            endpoints = listOf(Endpoint(host, 22)),
            jump = null,
            use = DialUse.STREAM,
            capture = null,
            forward = null,
            pty = null,
            stages = false,
            probe = false,
            agentSock = null,
            progress = false,
        ),
    auths = mapOf(host to AuthMethod.Password("p")),
    reconnect = policy,
)
