package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.remote.RemoteDuplexChannel
import kotlinx.coroutines.CoroutineScope

/** 接一台的结局。除了 [Up]，每一种都是一条走不通的路，出口照它说一句、不另猜。 */
sealed interface LinkOutcome {
    data class Up(
        val client: FrameClient,
        val catalog: CellsCatalog,
    ) : LinkOutcome

    /** 门槛没过（另一版 · 没装 · 问不成）。 */
    data class Gate(
        val verdict: GateVerdict,
    ) : LinkOutcome

    /** `--resident-ensure` 没起成常驻。 */
    data class Ensure(
        val outcome: OneShotOutcome,
    ) : LinkOutcome

    /** 常驻流没接上。 */
    data class Attach(
        val outcome: AttachOutcome,
    ) : LinkOutcome

    /** 接上的那一份 hello 自报的身份和这一版不同（门槛问过之后被换过）。 */
    data class HelloDiffers(
        val theirs: String?,
    ) : LinkOutcome

    /** 格目录问不成或缺手机要读的格：两端契约对不上。 */
    data class Contract(
        val missing: List<CellsCatalog.Cell>,
        val reply: Reply?,
    ) : LinkOutcome
}

/**
 * 连一台：门槛（同一个 `BUILD_ID`）→ `--resident-ensure`（常驻不在就起）→ 长 exec `--resident-attach`
 * 接常驻流 → 读格目录核手机要的格。和桌面连远端是同一条路（`IPC-PROTOCOL.md` 载体表「常驻套接字」）。
 * C 通道（[OneShot]）只在接流之前用这两次；之后一切都经 [FrameClient]。
 */
class ResidentLink(
    private val oneShot: OneShot,
    private val duplex: RemoteDuplexChannel,
) {
    suspend fun open(
        scope: CoroutineScope,
        nonce: String,
        timeouts: Timeouts,
        onBreak: (String) -> Unit,
    ): LinkOutcome {
        val gate = BackendGate.check(oneShot)
        if (gate != GateVerdict.Same) return LinkOutcome.Gate(gate)
        val ensured = oneShot.run(ENSURE)
        if (ensured !is OneShotOutcome.Ok) return LinkOutcome.Ensure(ensured)
        val attached = FrameClient.attach(duplex.open(BackendBin.command(ATTACH)), FLAGS, scope, timeouts.handshakeMs, nonce, onBreak)
        return if (attached is AttachOutcome.Attached) checked(attached.client, timeouts) else LinkOutcome.Attach(attached)
    }

    /** 接上之后再核两件：hello 自报的身份 · 格目录里有没有手机要读的格。不过 ⇒ 关流。 */
    private suspend fun checked(
        client: FrameClient,
        timeouts: Timeouts,
    ): LinkOutcome {
        val theirs = client.hello.str("build_id")
        val reply = if (theirs == EmbeddedBuild.ID) client.call(CellsCatalog.COMMAND, null, timeouts.callMs) else null
        val catalog = (reply as? Reply.Ok)?.let { CellsCatalog.of(it.data) }
        val missing = catalog?.missing(MobileCells.READS).orEmpty()
        val outcome =
            when {
                reply == null -> LinkOutcome.HelloDiffers(theirs)
                catalog == null -> LinkOutcome.Contract(emptyList(), reply)
                missing.isNotEmpty() -> LinkOutcome.Contract(missing, null)
                else -> LinkOutcome.Up(client, catalog)
            }
        if (outcome !is LinkOutcome.Up) client.close()
        return outcome
    }

    data class Timeouts(
        val handshakeMs: Long,
        val callMs: Long,
    )

    companion object {
        const val ENSURE: String = "--resident-ensure"
        const val ATTACH: String = "--resident-attach"

        /** 这条连接要的流旗标：不重放历史（正文由 `history-tail` / `history-read` 另取）。 */
        val FLAGS: List<String> = listOf("--tail-only")
    }
}
