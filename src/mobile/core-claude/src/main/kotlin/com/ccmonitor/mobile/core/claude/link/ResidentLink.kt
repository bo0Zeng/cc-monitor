package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.remote.RemoteDuplexChannel
import kotlinx.coroutines.CoroutineScope

/** 接一台的结局。除了 [Up]，每一种都是一条走不通的路，出口照它说一句、不另猜。 */
sealed interface LinkOutcome {
    data class Up(
        val client: FrameClient,
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
}

/**
 * 连一台：门槛（同一个 `BUILD_ID`）→ `--resident-ensure`（常驻不在就起）→ 长 exec `--resident-attach`
 * 接常驻流。和桌面连远端是同一条路（`IPC-PROTOCOL.md` 载体表「常驻套接字」）。
 * 两端对不对得上只核 `BUILD_ID` 这一处（改了冻结的格同加子命令一样要打版本号），不另问格目录。
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
        return if (attached is AttachOutcome.Attached) checked(attached.client) else LinkOutcome.Attach(attached)
    }

    /** 接上之后再对一次 hello 自报的身份（门槛问过之后那台可能被换过）。不对 ⇒ 关流。 */
    private fun checked(client: FrameClient): LinkOutcome {
        val theirs = client.hello.str("build_id")
        if (theirs == EmbeddedBuild.ID) return LinkOutcome.Up(client)
        client.close()
        return LinkOutcome.HelloDiffers(theirs)
    }

    data class Timeouts(
        val handshakeMs: Long,
    )

    companion object {
        const val ENSURE: String = "--resident-ensure"
        const val ATTACH: String = "--resident-attach"

        /** 这条连接要的流旗标：不重放历史（正文由 `history-tail` / `history-read` 另取）。 */
        val FLAGS: List<String> = listOf("--tail-only")
    }
}
