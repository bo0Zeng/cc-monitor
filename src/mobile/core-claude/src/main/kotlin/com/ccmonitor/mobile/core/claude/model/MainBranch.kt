package com.ccmonitor.mobile.core.claude.model

/**
 * 重建会话主分支。
 *
 * Claude Code 双击 ESC = 在某条历史 user 消息处重新编辑发送；被回退的旧分支仍留在 JSONL 里，
 * 形成 parentUuid 树上的分叉。/compact 还会产生多 root（system 边界 root + pre-compact 完整对话 root）。
 *
 * 算法：fork 点选最新后代那支；多 root 时折叠废弃的回撤
 *  0. 入口按 uuid 去重（保首见，对重投幂等）
 *  1. children 索引 + 找 root（parentUuid 为空或不在集合内）
 *  2. Kahn 自底向上算每个子树的 latestDescTs / latestConvTs(+isInterrupt) / hasAssistant
 *  3. winner root = latestDescTs 最大者，永远保留；其余 plain-user root 若子树死胡同
 *     （无 assistant 后代 或 最新会话叶子是 interrupt）→ 整棵折叠
 *  4. 保留的 root 往下 walk：单 child 直进；fork 选 latestDescTs 最大的 child，兄弟整棵 off-main
 *
 * 不用「全局最新叶子倒推」：/compact 场景会把整棵 pre-compact 树误标 off-main。
 * 全迭代无递归：几千条记录的近线性链递归会爆栈。
 */
object MainBranch {
    /** 参与分支链的精简记录。timestamp 为 ISO-8601（字典序=时间序）。 */
    data class BranchRecord(
        val uuid: String,
        val parentUuid: String?,
        val timestamp: String,
        val type: String,
        val isInterrupt: Boolean,
    )

    /** 返回主线 uuid 集合。空集 → 空。 */
    fun computeMainBranch(rawRecords: List<BranchRecord>): Set<String> {
        if (rawRecords.isEmpty()) return emptySet()

        // 0. 按 uuid 去重，保首见（幂等：重复输入不得毒化 Kahn）。
        val seenUuids = HashSet<String>()
        val records = ArrayList<BranchRecord>(rawRecords.size)
        for (r in rawRecords) if (seenUuids.add(r.uuid)) records.add(r)

        val byUuid = HashMap<String, BranchRecord>()
        for (r in records) byUuid[r.uuid] = r

        // 1. children 索引 + roots（parent 不在集合内 → root）
        val childrenOf = HashMap<String, MutableList<BranchRecord>>()
        val roots = ArrayList<BranchRecord>()
        for (r in records) {
            val p = r.parentUuid
            if (p != null && byUuid.containsKey(p)) {
                childrenOf.getOrPut(p) { ArrayList() }.add(r)
            } else {
                roots.add(r)
            }
        }

        // 2. Kahn 自底向上
        val latestDescTs = HashMap<String, String>()
        val latestConvTs = HashMap<String, String>()
        val latestConvIsInterrupt = HashMap<String, Boolean>()
        val subtreeHasAssistant = HashMap<String, Boolean>()
        val remaining = HashMap<String, Int>()
        val queue = ArrayDeque<BranchRecord>()
        for (r in records) {
            val c = childrenOf[r.uuid]?.size ?: 0
            if (c == 0) queue.add(r) else remaining[r.uuid] = c
        }
        while (queue.isNotEmpty()) {
            val r = queue.removeFirst()
            var max = r.timestamp
            val rIsConv = r.type == "user" || r.type == "assistant"
            var convTs = if (rIsConv) r.timestamp else ""
            var convIsInterrupt = if (rIsConv) r.isInterrupt else false
            var hasAssistant = r.type == "assistant"
            childrenOf[r.uuid]?.let { kids ->
                for (k in kids) {
                    val kts = latestDescTs[k.uuid]
                    if (kts != null && kts > max) max = kts
                    val kConvTs = latestConvTs[k.uuid] ?: ""
                    if (kConvTs > convTs) {
                        convTs = kConvTs
                        convIsInterrupt = latestConvIsInterrupt[k.uuid] ?: false
                    }
                    if (subtreeHasAssistant[k.uuid] == true) hasAssistant = true
                }
            }
            latestDescTs[r.uuid] = max
            latestConvTs[r.uuid] = convTs
            latestConvIsInterrupt[r.uuid] = convIsInterrupt
            subtreeHasAssistant[r.uuid] = hasAssistant
            val pu = r.parentUuid ?: continue
            val p = byUuid[pu] ?: continue
            val next = (remaining[p.uuid] ?: 1) - 1
            if (next <= 0) {
                remaining.remove(p.uuid)
                queue.add(p)
            } else {
                remaining[p.uuid] = next
            }
        }
        // leftover（理论是环；append-only 不应发生）→ fallback 自身 ts，防 walk 拿到 null。
        for (uuid in remaining.keys) {
            val r = byUuid[uuid] ?: continue
            latestDescTs[uuid] = r.timestamp
            val conv = r.type == "user" || r.type == "assistant"
            latestConvTs[uuid] = if (conv) r.timestamp else ""
            latestConvIsInterrupt[uuid] = if (conv) r.isInterrupt else false
            subtreeHasAssistant[uuid] = r.type == "assistant"
        }

        // 3. winner root = latestDescTs 最大者
        var winner: BranchRecord? = null
        var winnerTs = ""
        for (root in roots) {
            val ts = latestDescTs[root.uuid] ?: root.timestamp
            if (winner == null || ts > winnerTs) {
                winner = root
                winnerTs = ts
            }
        }

        // 4. walk 保留的 root
        val onMain = HashSet<String>()
        for (root in roots) {
            if (root !== winner && root.type == "user") {
                val hasAssistant = subtreeHasAssistant[root.uuid] ?: false
                val latestIsInterrupt = latestConvIsInterrupt[root.uuid] ?: false
                if (!hasAssistant || latestIsInterrupt) continue // 废弃 ESC 回撤 root → 折叠
            }
            var cursor: BranchRecord? = root
            while (cursor != null) {
                if (!onMain.add(cursor.uuid)) break // 环防御
                val kids = childrenOf[cursor.uuid]
                if (kids.isNullOrEmpty()) break
                if (kids.size == 1) {
                    cursor = kids[0]
                    continue
                }
                var w = kids[0]
                var wTs = latestDescTs[kids[0].uuid] ?: kids[0].timestamp
                for (i in 1 until kids.size) {
                    val ts = latestDescTs[kids[i].uuid] ?: kids[i].timestamp
                    if (ts > wTs) {
                        w = kids[i]
                        wTs = ts
                    }
                }
                cursor = w
            }
        }
        return onMain
    }

    /**
     * 从 JsonlRecord 抽 [BranchRecord]：只接受 user/assistant/attachment/system（这四种带 uuid+timestamp，
     * 夹在链中间不渲染也要 track，否则父链断成碎片）。无 uuid/timestamp → null，不参与链。
     */
    fun extractBranchRecord(rec: JsonlRecord): BranchRecord? {
        val type =
            when (rec) {
                is JsonlRecord.User -> "user"
                is JsonlRecord.Assistant -> "assistant"
                is JsonlRecord.Attachment -> "attachment"
                is JsonlRecord.System -> "system"
                // 有 uuid 的未知/解析失败记录作 pass-through 链节点（非 conv/assistant/interrupt），不让其后的对话变孤儿；无 uuid 的下面过滤。
                is JsonlRecord.Unknown -> "unknown"
                else -> return null
            }
        val uuid = rec.uuid ?: return null
        val ts = rec.timestamp ?: return null
        val isInterrupt = rec is JsonlRecord.User && isInterruptUser(rec)
        return BranchRecord(uuid, rec.parentUuid, ts, type, isInterrupt)
    }

    /** user 记录是否是 "[Request interrupted by user…]" 打断标记（多 root 折叠的死胡同信号）。 */
    private fun isInterruptUser(u: JsonlRecord.User): Boolean {
        val firstText = u.blocks.firstNotNullOfOrNull { (it as? ContentBlock.Text)?.text } ?: return false
        return firstText.startsWith("[Request interrupted by user")
    }

    /**
     * 便捷入口：解析后的记录流 → 仅保留主分支上的记录，按文件顺序、每 uuid 一次（保首见）。
     * 无 uuid 的记录（title/mode 等元数据）不属于对话链，过滤掉（分类器本就丢弃它们）。
     */
    fun resolve(records: List<JsonlRecord>): List<JsonlRecord> {
        val onMain = computeMainBranch(records.mapNotNull { extractBranchRecord(it) })
        val emitted = HashSet<String>()
        return records.filter { r ->
            val id = r.uuid ?: return@filter false
            id in onMain && emitted.add(id)
        }
    }
}
