package com.ccmonitor.mobile.core.rs

import java.util.concurrent.ConcurrentHashMap

/**
 * 常驻流交的事按那台的编号分到它的收件处（[Core.Sink.resident] 只有一个，进程里谁实现 Sink 都转到这里）。
 * 编号还没登记（[Core.residentOpen] 刚回、还没 [register]）时到的事先攒着，登记那一刻按序交出去。
 */
object ResidentEvents {
    private val sinks = ConcurrentHashMap<Long, (String, String) -> Unit>()
    private val early = ConcurrentHashMap<Long, MutableList<Pair<String, String>>>()

    fun register(
        handle: Long,
        to: (kind: String, body: String) -> Unit,
    ) {
        synchronized(this) {
            sinks[handle] = to
            early.remove(handle)
        }?.forEach { (k, b) -> to(k, b) }
    }

    fun deliver(
        handle: Long,
        kind: String,
        body: String,
    ) {
        val to =
            sinks[handle] ?: synchronized(this) {
                sinks[handle] ?: run {
                    early.getOrPut(handle) { mutableListOf() } += kind to body
                    null
                }
            }
        to?.invoke(kind, body)
    }
}
