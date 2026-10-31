package com.ccmonitor.mobile.service

import android.content.Context
import android.content.Intent
import android.util.Log
import androidx.core.content.ContextCompat
import com.ccmonitor.mobile.core.ssh.SshConnectionManager
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.ui.chat.ChatController
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

/**
 * 此刻有哪几样活（保活服务的存活判据就是它，[KeepAlivePolicy.hasWork]）：活动会话几个 · 有没有在飞轮次 · 有没有一台在核心里传东西
 * （一组删 / 复制 / 下到本机 · 一趟分块传，各台常驻流的 `working`）。
 */
data class Work(
    val sessions: Int,
    val inFlight: Boolean,
    val moving: Boolean,
) {
    val any: Boolean get() = KeepAlivePolicy.hasWork(sessions, inFlight, moving)
}

/** 三样并成一条流（应用级）：服务照它自停，应用照它在「有活」那一下起服务（[startOnWork]）。 */
@OptIn(ExperimentalCoroutinesApi::class)
class KeepAliveWork(
    private val manager: SshConnectionManager,
    private val chat: ChatController,
    private val backends: HostBackends,
) {
    private val moving: Flow<Boolean> =
        backends.feeds.flatMapLatest { feeds ->
            if (feeds.isEmpty()) flowOf(false) else combine(feeds.values.map { it.working }) { w -> w.any { it } }
        }

    val work: Flow<Work> =
        combine(manager.sessions, chat.hasInFlightTurn, moving) { _, inFlight, m -> Work(manager.activeCount(), inFlight, m) }

    /** 此刻那一份（服务的系统回调里同步问）。 */
    fun now(): Work =
        Work(
            manager.activeCount(),
            chat.hasInFlightTurn.value,
            backends.feeds.value.values
                .any { it.working.value },
        )

    /**
     * 「有活」那一下起保活服务（不挂在哪一屏上：文件页下载 · 聊天附件在传 · 终端连着都从这里起）；没活时服务自停。
     * 系统不许在后台起前台服务（Android 12+）⇒ 记一条，等下一次有活再起。
     */
    fun startOnWork(
        scope: CoroutineScope,
        context: Context,
    ) = scope.launch {
        work.map { it.any }.distinctUntilChanged().filter { it }.collect { _ ->
            val app = context.applicationContext
            runCatching { ContextCompat.startForegroundService(app, Intent(app, SshKeepAliveService::class.java)) }
                .onFailure { Log.w(TAG, "起保活服务没成：${it.message}") }
        }
    }

    private companion object {
        const val TAG = "KeepAliveWork"
    }
}
