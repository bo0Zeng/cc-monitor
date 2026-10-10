package com.ccmonitor.mobile.core.claude.transport

import kotlinx.coroutines.flow.Flow

/** [TailTransport] 吐出来的一帧：开场白 [Hello] 之后逐行 [Line]。 */
sealed interface JsonlFrame {
    /**
     * 一行原始 jsonl。[byteOffset] = 本行末尾（含 `\n`）的累计原始字节 = resume 锚点（取 [LineFramer.Framed.endOffset]）。
     * [seq] = 这条 tail 从 0 起的序数，不是 resume 键。[raw] 原样透传，消费方自己 parse。
     */
    data class Line(
        val sessionId: String?,
        val path: String?,
        val seq: Long,
        val raw: String,
        val byteOffset: Long,
    ) : JsonlFrame

    /** 开场白：tail 起来了（还没读到行）。 */
    data object Hello : JsonlFrame
}

/** 逐行读一个追加文件的传输（[TailTransport]）。 */
interface JsonlTransport {
    fun frames(): Flow<JsonlFrame>
}
