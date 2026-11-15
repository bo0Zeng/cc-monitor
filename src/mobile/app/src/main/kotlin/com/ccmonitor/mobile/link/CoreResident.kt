package com.ccmonitor.mobile.link

import com.ccmonitor.mobile.core.claude.link.ResidentCore
import com.ccmonitor.mobile.core.rs.Core
import com.ccmonitor.mobile.core.rs.ResidentEvents
import com.ccmonitor.mobile.core.ssh.ensureRustCore
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

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

    override fun read(
        handle: Long,
        path: String,
        cap: Long,
        withinMs: Long,
    ): Long = Core.transferRead(handle, path, cap, withinMs)

    override fun transferStop(id: Long) = Core.transferStop(id)

    override fun attach(
        handle: Long,
        op: String,
    ): String = Core.attach(handle, op)

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

    override fun pickMenu(
        handle: Long,
        rows: String,
    ): String {
        ensureRustCore()
        return Core.pickMenu(handle, rows)
    }

    /** core-claude 那两个读口要的那一口（`ColumnFace` · `GroupsFace`）：直通核心。 */
    fun chatsColumn(ask: String): String = Core.chatsColumn(ask)

    fun groupsMerge(data: String): String = Core.groupsMerge(data)

    /** 逐台问一圈再合（`NeedsFace` · `SearchMerge` 那一行）：等各台都有结局才回，挪到 IO 线程上等。 */
    suspend fun needsAll(data: String): String =
        withContext(Dispatchers.IO) {
            ensureRustCore()
            Core.needsAll(data)
        }

    suspend fun searchAll(data: String): String =
        withContext(Dispatchers.IO) {
            ensureRustCore()
            Core.searchAll(data)
        }

    override fun deploy(
        handle: Long,
        slots: String,
    ): Boolean = Core.deploy(handle, slots)
}
