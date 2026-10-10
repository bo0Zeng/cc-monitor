package com.ccmonitor.mobile.core.claude.model

/**
 * 把有序的 [JsonlRecord] 流折叠成 [RenderUnit] 列表。
 *
 * 关键职责是 tool_use ↔ tool_result 配对：tool_use 出现在 assistant 记录里，
 * 其结果在之后某条 user 记录的 tool_result 块里（按 tool_use_id 关联）。单趟前向扫描，
 * 用 id→index 表回填结果即可（tool_use 必先于其 result）。
 *
 * 过滤：meta user（注入的上下文/命令回显）、system、attachment、unknown、title 全部跳过 ——
 * 它们不是对话正文。孤儿 tool_result（无匹配 tool_use）丢弃。
 */
object RecordClassifier {
    fun classify(records: List<JsonlRecord>): List<RenderUnit> {
        val units = ArrayList<RenderUnit>()
        val toolIndexById = HashMap<String, Int>()
        // 全量分类不需要挂起孤儿：能配上的都在同一批里
        for (rec in records) classifyRecord(rec, null, units, toolIndexById)
        return units
    }

    /**
     * 增量分类缓存。[onMainUuids] = 上次 `MainBranch.resolve` 输出记录的 uuid 序列（前缀判定用）；
     * [units]/[toolIndexById] = 分类到该点的累积状态（[toolIndexById] 让迟到的 tool_result 能回填旧 ToolCall）。
     */
    data class IncrementalState(
        val onMainUuids: List<String>,
        val units: List<RenderUnit>,
        val toolIndexById: Map<String, Int>,
        /**
         * 孤儿 tool_result：它的 `tool_use` 还没载进来。有界首载必然产生：窗口起点切在会话中间时，
         * 正在执行的工具的 `tool_use` 在没载的更早那段。丢掉它的话，上滑载入老段后那张卡永远 pending。
         */
        val orphanResults: Map<String, OrphanResult> = emptyMap(),
    ) {
        companion object {
            val EMPTY = IncrementalState(emptyList(), emptyList(), emptyMap())
        }
    }

    /** 挂起的 tool_result（等它的 `tool_use` 被 prepend 进来时回填）。 */
    data class OrphanResult(
        val text: String,
        val isError: Boolean,
        val structuredPatch: List<PatchHunk>?,
        val patchFilePath: String?,
    )

    /**
     * 增量分类，免得阅读面每 tick 全量重建所有 [RenderUnit]。
     * [onMainRecords] = `MainBranch.resolve` 的输出（主分支过滤+去重、按文件序）。
     * 常态是 append-only（[prev].onMainUuids 是本次的前缀）：复用 prevUnits，只分类新增记录，
     * 新记录里的 tool_result 用保留的 toolIndexById 回填已有 ToolCall。
     * 前缀不匹配（ESC 回退改了主链）→ 回退全量重算。结果与 [classify] 逐条等价。
     *
     * @param collectOrphans 是否挂起「`tool_use` 还没载进来」的 tool_result，供后续 [classifyPrepend] 回填。
     *   默认关：开着每次都要拷一份只增不减、存着结果全文的 map，而阅读面从不 prepend。
     */
    fun classifyIncremental(
        prev: IncrementalState,
        onMainRecords: List<JsonlRecord>,
        collectOrphans: Boolean = false,
    ): IncrementalState {
        val newUuids = ArrayList<String>(onMainRecords.size)
        for (r in onMainRecords) newUuids.add(r.uuid ?: "")
        // 复用前提（前缀判定只比 uuid 序列、不比内容）：records append-only 且 `MainBranch.resolve` 保首见，
        // 同一 uuid 的首见记录永不变，分类结果也不变。
        // 注意：按字节 offset 续传或截断时要保住这一点（offset 落在完整行边界；截断时重置为 EMPTY），否则会静默复用陈旧 unit。
        val canReuse =
            prev.onMainUuids.size <= newUuids.size &&
                newUuids.subList(0, prev.onMainUuids.size) == prev.onMainUuids
        val units: ArrayList<RenderUnit>
        val toolIndexById: HashMap<String, Int>
        val startIdx: Int
        if (canReuse) {
            units = ArrayList(prev.units) // 浅拷贝：未被新 tool_result 触碰的旧 unit 保持同一实例
            toolIndexById = HashMap(prev.toolIndexById)
            startIdx = prev.onMainUuids.size
        } else {
            units = ArrayList()
            toolIndexById = HashMap()
            startIdx = 0
        }
        val orphans = if (collectOrphans) HashMap(prev.orphanResults) else null
        for (i in startIdx until onMainRecords.size) classifyRecord(onMainRecords[i], orphans, units, toolIndexById)
        return IncrementalState(newUuids, units, toolIndexById, orphans ?: emptyMap())
    }

    /**
     * 向前插入更早的一段（上滑翻历史），与 [classifyIncremental] 对称：那个往后追加，这个往前插入。
     *
     * 1. 边界去重：翻页区间与已载区间在接缝处必然重叠，按 uuid 去重（prepend 后位置全变了）。
     * 2. 索引平移：`toolIndexById` 存的是下标，前面插了 k 条之后旧下标全部 +k，否则迟到的结果回填到错的卡上。
     * 3. 孤儿回填：已载段里 `tool_use` 在更早处的 tool_result（[IncrementalState.orphanResults]）此刻可能配上了；
     *    不回填那张卡永远 pending。分两段载必须等于一次全载。
     *
     * @param olderRecords 更早那一段的主线记录（`MainBranch.resolve` 的输出，按文件序）。
     */
    fun classifyPrepend(
        prev: IncrementalState,
        olderRecords: List<JsonlRecord>,
    ): IncrementalState {
        // ① 边界去重：已经在的不再来一遍
        val known = prev.onMainUuids.toHashSet()
        val fresh = olderRecords.filter { (it.uuid ?: "") !in known }
        if (fresh.isEmpty()) return prev

        val olderUnits = ArrayList<RenderUnit>()
        val olderToolIndex = HashMap<String, Int>()
        val olderOrphans = HashMap<String, OrphanResult>()
        for (rec in fresh) classifyRecord(rec, olderOrphans, olderUnits, olderToolIndex)

        // ② 索引平移：旧 unit 整体后移 olderUnits.size 位
        val shift = olderUnits.size
        val mergedUnits = ArrayList<RenderUnit>(shift + prev.units.size)
        mergedUnits.addAll(olderUnits)
        mergedUnits.addAll(prev.units)
        val mergedIndex = HashMap<String, Int>(olderToolIndex)
        prev.toolIndexById.forEach { (id, idx) -> mergedIndex[id] = idx + shift }

        // ③ 孤儿回填：老段带来的 tool_use 可能正是它们等的那个
        val stillOrphan = HashMap<String, OrphanResult>(olderOrphans)
        prev.orphanResults.forEach { (id, orphan) ->
            val idx = mergedIndex[id]
            val call = idx?.let { mergedUnits[it] as? RenderUnit.ToolCall }
            if (call == null) {
                stillOrphan[id] = orphan
            } else {
                mergedUnits[idx] =
                    call.copy(
                        resultText = orphan.text,
                        isError = orphan.isError,
                        pending = false,
                        structuredPatch = orphan.structuredPatch,
                        patchFilePath = orphan.patchFilePath,
                    )
            }
        }

        return IncrementalState(
            onMainUuids = fresh.map { it.uuid ?: "" } + prev.onMainUuids,
            units = mergedUnits,
            toolIndexById = mergedIndex,
            orphanResults = stillOrphan,
        )
    }

    /** 单条记录 → 追加/回填 [units]（[toolIndexById] 跨记录累积，供 tool_result 回填先前的 ToolCall）。 */
    private fun classifyRecord(
        rec: JsonlRecord,
        orphans: HashMap<String, OrphanResult>?,
        units: ArrayList<RenderUnit>,
        toolIndexById: HashMap<String, Int>,
    ) {
        when (rec) {
            is JsonlRecord.Assistant ->
                if (!rec.isSidechain) {
                    rec.blocks.forEachIndexed { bi, block -> classifyAssistantBlock(rec, bi, block, units, toolIndexById) }
                }
            is JsonlRecord.User ->
                // isMeta（注入上下文/skill 正文）与 isSidechain（子 agent）都不渲染。
                if (!rec.isMeta && !rec.isSidechain) classifyUserRecord(rec, units, toolIndexById, orphans)
            is JsonlRecord.System,
            is JsonlRecord.Title,
            is JsonlRecord.Attachment,
            is JsonlRecord.Unknown,
            -> {} // 非正文，跳过
        }
    }

    private fun classifyAssistantBlock(
        rec: JsonlRecord.Assistant,
        bi: Int,
        block: ContentBlock,
        units: ArrayList<RenderUnit>,
        toolIndexById: HashMap<String, Int>,
    ) {
        val key = unitKey(rec.uuid, bi)
        when (block) {
            is ContentBlock.Text ->
                if (block.text.isNotBlank()) units.add(RenderUnit.AssistantMarkdown(key, block.text, rec.uuid))
            is ContentBlock.Thinking ->
                if (block.thinking.isNotBlank()) units.add(RenderUnit.Thinking(key, block.thinking, rec.uuid))
            is ContentBlock.ToolUse -> {
                toolIndexById[block.id] = units.size
                units.add(
                    RenderUnit.ToolCall(
                        key = key,
                        toolUseId = block.id,
                        name = block.name,
                        input = block.input,
                        resultText = null,
                        isError = false,
                        pending = true,
                        sourceUuid = rec.uuid,
                    ),
                )
            }
            is ContentBlock.ToolResult, is ContentBlock.Unknown -> {} // assistant 不产出这些
        }
    }

    /**
     * user 记录按整条分类（slash/bash 识别需要整条文本）：
     * 拼接 Text 块→rawText + 顺带 reconcile tool_result 块；rawText 空→仅 tool_result（tool-group 回填，不出 unit）；
     * 非空→stripInternalNoise，剥空→skip（interrupt/纯 stdout 噪音）；否则识别 compact/slash/bash→专门卡，兜底 UserText。
     */
    private fun classifyUserRecord(
        rec: JsonlRecord.User,
        units: ArrayList<RenderUnit>,
        toolIndexById: HashMap<String, Int>,
        orphans: HashMap<String, OrphanResult>? = null,
    ) {
        val sb = StringBuilder()
        for (block in rec.blocks) {
            when (block) {
                is ContentBlock.Text -> {
                    if (sb.isNotEmpty()) sb.append('\n')
                    sb.append(block.text)
                }
                is ContentBlock.ToolResult ->
                    reconcile(units, toolIndexById, block, rec.structuredPatch, rec.patchFilePath, orphans)
                else -> {} // thinking/tool_use/unknown 不会出现在 user
            }
        }
        val rawText = sb.toString()
        if (rawText.isBlank()) return // tool_result 记录（已回填）或空 → 不产 unit
        val stripped = stripInternalNoise(rawText)
        if (stripped.isEmpty()) return // 纯噪音（ESC 中断 / 纯 local-command-stdout 等）→ skip
        units.add(userUnit(unitKey(rec.uuid, 0), stripped, rec.uuid))
    }

    /** 剥噪音后的非空 user 文本 → 专门卡（compact/slash/bash，互斥）或兜底 UserText。 */
    private fun userUnit(
        key: String,
        stripped: String,
        uuid: String?,
    ): RenderUnit {
        val slash = parseSlashCommand(stripped)
        val bashIn = parseBashInput(stripped)
        val bashOut = parseBashOutput(stripped)
        return when {
            isCompactSummary(stripped) -> RenderUnit.CompactSummary(key, stripped, uuid)
            slash != null -> RenderUnit.SlashCommand(key, slash.name, slash.args, uuid)
            bashIn != null -> RenderUnit.BashInput(key, bashIn.command, uuid)
            bashOut != null -> RenderUnit.BashOutput(key, bashOut.stdout, bashOut.stderr, uuid)
            else -> RenderUnit.UserText(key, stripped, uuid)
        }
    }

    /** 稳定 key：记录 uuid + 块在该记录内的序号。基于身份非列表位置，流式追加不漂移。 */
    private fun unitKey(
        uuid: String?,
        blockIndex: Int,
    ): String = "${uuid ?: "_"}#$blockIndex"

    /** 用 tool_result 回填对应 ToolCall；孤儿（无 id 匹配）丢弃。 */
    private fun reconcile(
        units: ArrayList<RenderUnit>,
        toolIndexById: HashMap<String, Int>,
        result: ContentBlock.ToolResult,
        structuredPatch: List<PatchHunk>?, // 来自 User 记录 top-level（toolUseResult.structuredPatch）
        patchFilePath: String?, // toolUseResult.filePath
        orphans: HashMap<String, OrphanResult>? = null, // 查不到 tool_use 时挂起，等 prepend 回填
    ) {
        val id = result.toolUseId ?: return
        val idx = toolIndexById[id]
        if (idx == null) {
            // 不丢：窗口起点切在中间时，它的 tool_use 在还没载的老段里
            orphans?.put(id, OrphanResult(result.text, result.isError, structuredPatch, patchFilePath))
            return
        }
        val call = units[idx] as? RenderUnit.ToolCall ?: return
        units[idx] =
            call.copy(
                resultText = result.text,
                isError = result.isError,
                pending = false,
                structuredPatch = structuredPatch, // 保真 diff，有则优先于 input 推导
                patchFilePath = patchFilePath,
            )
    }
}
