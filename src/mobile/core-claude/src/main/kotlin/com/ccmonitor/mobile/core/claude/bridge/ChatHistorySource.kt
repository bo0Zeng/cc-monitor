package com.ccmonitor.mobile.core.claude.bridge

import com.ccmonitor.mobile.core.claude.transport.SkeletonScan
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.ccmonitor.mobile.core.remote.ShellWord
import com.squareup.moshi.Moshi
import java.io.ByteArrayOutputStream

/**
 * 打开一条已有对话时，把历史读出来。
 *
 * `--resume` 不回放历史：Claude 记得上下文，屏幕却是空的。历史只能另外从会话记录
 * （`<配置目录>/projects/<编码后的 cwd>/<编号>.jsonl`）读。
 *
 * 读一次，不 tail：那份记录同时在被这条对话写，tail 它等于同一句话来两遍（管道一遍、记录一遍）。
 * 每一页都是一次有界的一次性读（`tail -c +N | head -c LEN`）。
 *
 * 记录里 `type: assistant|user` 的外壳是 `{type, message, …}`，`message` 与 CLI 行同形，
 * 所以直接喂 [CliFrameEncoder]。其余记录类型落 `ev`，不产渲染单元，自然不上屏。
 */
class ChatHistorySource(
    private val channel: RemoteCommandChannel,
    /**
     * 会话记录在远端的那个 shell 词（来自 `ClaudePaths.sessionRecordPath`）。
     * 是 [ShellWord] 而非字面路径：再引一层的话远端打开的是一个不存在的名字，历史静默读空。
     */
    private val recordPath: ShellWord,
    private val pageRecords: Int = PAGE_RECORDS,
    private val pageMaxBytes: Long = PAGE_MAX_BYTES,
) {
    private var skeleton: SkeletonScan.Result? = null
    private var cursor: Long = Long.MAX_VALUE
    private var exhausted = false
    private val anyAdapter = Moshi.Builder().build().adapter(Any::class.java)

    /**
     * 读再往前一页。
     *
     * @return null = 已经到最早（UI 显示「已是最早」而不是无限转圈）。
     *   空表不等于到顶：这一段可能全是不产渲染单元的记录，上层应再要一页。
     */
    suspend fun loadOlder(): List<BridgeFrame>? {
        if (exhausted) return null
        val scan = skeleton ?: scanSkeleton().also { skeleton = it }
        if (scan.records.isEmpty()) {
            exhausted = true
            return null
        }
        // 首次翻页从文件末尾往前算；`eofBytes` 是骨架扫描顺带给的。
        if (cursor == Long.MAX_VALUE) cursor = scan.eofBytes

        val range = SkeletonScan.rangeBefore(scan.records, cursor, pageRecords, pageMaxBytes)
        if (range.isEmpty) {
            exhausted = true
            return null
        }
        val text = collect(SkeletonScan.rangeContentCommand(recordPath, range))
        cursor = range.start

        val enc = CliFrameEncoder()
        val frames =
            text
                .lineSequence()
                .filter { it.isNotBlank() }
                // 单行坏掉只丢这一行，不拖垮整页。
                .mapNotNull { runCatching { anyAdapter.fromJson(it) as? Map<*, *> }.getOrNull() }
                .flatMap { enc.feed(it).asSequence() }
                .toList()
        return trimToUnitBudget(frames)
    }

    /**
     * 把这一页裁到 [MAX_PREPEND_UNITS] 个渲染单元以内，保留靠近当前窗口的尾部。
     *
     * 一条记录会展开成多个 unit，只按记录数封顶不够；`LazyColumn` 的 key 锚定窗口按 unit 算，
     * 超了往前插一页后列表位置会乱跳。数 unit 用真的 [ChatTurnAssembler]，不另写一份判据。
     */
    private fun trimToUnitBudget(frames: List<BridgeFrame>): List<BridgeFrame> {
        if (unitsOf(frames) <= MAX_PREPEND_UNITS) return frames
        var lo = 1
        var hi = frames.size
        // 二分找最多能保留多少条尾部帧而不超预算。
        while (lo < hi) {
            val mid = (lo + hi + 1) / 2
            if (unitsOf(frames.takeLast(mid)) <= MAX_PREPEND_UNITS) lo = mid else hi = mid - 1
        }
        return frames.takeLast(lo)
    }

    private fun unitsOf(frames: List<BridgeFrame>): Int {
        val probe = ChatTurnAssembler()
        frames.forEach { probe.feed(it) }
        return probe.units().size
    }

    /** 骨架只扫一次，翻页不重扫。 */
    private suspend fun scanSkeleton(): SkeletonScan.Result {
        val tsv = collect(SkeletonScan.skeletonCommand(recordPath))
        val parsed = SkeletonScan.parse(tsv)
        // 只走主线：分支与子 agent 的记录不混进这条对话。
        return SkeletonScan.Result(SkeletonScan.mainBranchSkeleton(parsed.records), parsed.eofBytes)
    }

    private suspend fun collect(command: String): String {
        val buf = ByteArrayOutputStream()
        channel.exec(command).collect { buf.write(it) }
        return buf.toString("UTF-8")
    }

    companion object {
        /** 单次翻页的记录条数。单元数上限另由 [MAX_PREPEND_UNITS] 守。 */
        const val PAGE_RECORDS = 40

        /** 单次翻页的字节封顶：一条巨型 Write 记录就能撑爆一页。 */
        const val PAGE_MAX_BYTES = 512L * 1024

        /**
         * 单次往前插的渲染单元上限，与阅读面同一个数：列表的 key 锚定窗口按 unit 算
         * （`NearestItemsExtraItemCount = 100`），超了就锚不住。
         */
        const val MAX_PREPEND_UNITS = 100
    }
}
