package com.ccmonitor.mobile.core.claude.bridge

import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow

/** 一行录制：帧 + 它在录制时的到达时刻（纳秒，相对录制起点）。 */
data class ReplayRow(
    val tNs: Long,
    val frame: BridgeFrame,
)

/** 重放节奏。 */
sealed interface Pacing {
    /** 照录制时的真实间隔发。 */
    data object Original : Pacing

    /** 不等待，全速发完。给 Compose UI 测试用：那里的 `delay` 是真睡。 */
    data object None : Pacing
}

/**
 * 把录制好的 `bridge/vectors/<case>.frames.ndjson` 按原始节奏重放成帧流，
 * 让「帧到达 → 上屏」这条链不连服务器也能验证。
 *
 * JVM 单测里 `runTest` 已把 [delay] 虚拟化，照 [Pacing.Original] 跑也是瞬间完成；
 * 只有 Compose UI 测试需要 [Pacing.None]。
 */
class ReplayTransport(
    private val rows: List<ReplayRow>,
    private val pacing: Pacing = Pacing.Original,
) {
    val size: Int get() = rows.size

    fun frames(): Flow<BridgeFrame> =
        flow {
            var prev: Long? = null
            for (row in rows) {
                if (pacing is Pacing.Original) {
                    val gapNs = prev?.let { row.tNs - it } ?: 0L
                    // 负间隔按 0 处理
                    val gapMs = (gapNs / NANOS_PER_MILLI).coerceAtLeast(0L)
                    if (gapMs > 0) delay(gapMs)
                }
                prev = row.tNs
                emit(row.frame)
            }
        }

    companion object {
        private const val NANOS_PER_MILLI = 1_000_000L
    }
}

/** 录制文件的解析，与 [ReplayTransport] 分开。 */
object ReplayVectors {
    /**
     * @param lines `<case>.frames.ndjson` 的每一行。
     *
     * 注意：首行是 `__meta__`（录制环境说明），不是帧，要跳过。
     */
    fun parse(
        lines: Sequence<String>,
        codec: BridgeCodec = BridgeCodec(),
    ): List<ReplayRow> =
        lines
            .mapNotNull { line ->
                if (line.isBlank()) return@mapNotNull null
                val map = codec.rawRow(line) ?: return@mapNotNull null
                if (map.containsKey("__meta__")) return@mapNotNull null
                val frameMap = map["frame"] as? Map<*, *> ?: return@mapNotNull null
                ReplayRow(
                    tNs = (map["t_ns"] as? Number)?.toLong() ?: 0L,
                    frame = codec.decode(frameMap),
                )
            }.toList()
}
