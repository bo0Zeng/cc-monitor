package com.ccmonitor.mobile.link

import com.ccmonitor.mobile.core.claude.link.ResidentCore
import com.ccmonitor.mobile.core.rs.Core
import com.ccmonitor.mobile.core.rs.ResidentEvents
import com.ccmonitor.mobile.core.ssh.ensureRustCore

/** [ResidentCore] 直通嵌的 Rust 核心（`Core.resident*`）；交回来的事按那台的编号经 [ResidentEvents] 分过来。 */
object CoreResident : ResidentCore {
    override fun open(
        machine: String,
        mine: String,
        tz: String,
        events: (kind: String, body: String) -> Unit,
    ): Long {
        ensureRustCore()
        return Core.residentOpen(machine, mine, tz).also { ResidentEvents.register(it, events) }
    }

    override fun ssh(
        handle: Long,
        request: String?,
    ) {
        Core.residentSsh(handle, request)
    }

    override fun call(
        handle: Long,
        cmd: String,
        args: String,
        withinMs: Long,
    ): Long = Core.residentCall(handle, cmd, args, withinMs)

    override fun watch(
        handle: Long,
        sid: String,
        holding: Boolean,
    ) = Core.residentWatch(handle, sid, holding)

    override fun kick(
        handle: Long,
        retry: Boolean,
    ) = Core.residentKick(handle, retry)

    override fun stage(
        handle: Long,
        fd: Int,
        name: String,
        withinMs: Long,
    ): Long = Core.transferStage(handle, fd, name, withinMs)

    override fun pull(
        handle: Long,
        fd: Int,
        args: String,
        shown: String,
        withinMs: Long,
    ): Long = Core.transferPull(handle, fd, args, shown, withinMs)

    override fun batchStart(
        handle: Long,
        want: String,
    ): Long = Core.batchStart(handle, want)

    override fun batchAnswer(
        id: Long,
        answer: String?,
    ) {
        Core.batchAnswer(id, answer)
    }

    override fun batchStop(id: Long) {
        Core.batchStop(id)
    }

    override fun deploy(
        handle: Long,
        slots: String,
    ): Boolean = Core.deploy(handle, slots)
}
