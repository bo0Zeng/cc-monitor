package com.ccmonitor.mobile.core.claude.transport

/**
 * 把「谁从谁 fork 出来」摊成一棵可直接渲染的有序树。
 *
 * 血统在 JSONL 文件里、不在后端 wire 里（`session_added` 不带 `forkedFrom`）：每条消息都带
 * `forkedFrom{sessionId, messageUuid}`，读会话第一条就知道父。解析用 `JsonlRecord.forkedFrom`，
 * 取记录的 IO 在 [DaemonHistoryReader]，这里是纯函数。
 *
 * 没有分支的对话就是平的一行，`depth == 0` 是常态，UI 不为树预留缩进槽位。
 */
object SessionLineage {
    /** 一行：会话 id + 它在树里的深度（0 = 根）。 */
    data class Node(
        val sessionId: String,
        val depth: Int,
    )

    /**
     * @param order 会话 id 的展示顺序（调用方已排好，本函数不重排同级）。
     * @param parentOf `sid → 父 sid`。父不在 [order] 里（已归档 / 不在本次列表）⇒ 该条按根处理。
     * @return 与 [order] 同一批 id，重排成「父在前、子紧随其后」，并带 depth。
     *
     * 环要能走出去：`forkedFrom` 是文件里的数据，可能被改坏。成环时环上的节点按根处理，宁可画错不可挂死。
     */
    fun arrange(
        order: List<String>,
        parentOf: Map<String, String>,
    ): List<Node> {
        val present = order.toHashSet()
        // 父不在场 ⇒ 视作根（不是丢掉：那条会话仍要显示）
        val effectiveParent = parentOf.filterValues { it in present }.filterKeys { it in present }
        val roots = order.filter { effectiveParent[it] == null || isCyclic(it, effectiveParent) }
        val childrenOf = order.groupBy { effectiveParent[it] }
        val out = ArrayList<Node>(order.size)
        val emitted = HashSet<String>()
        for (root in roots) emit(root, 0, childrenOf, out, emitted)
        // 兜底：没被走到的按根补在末尾，一条都不许丢。按现在的 [isCyclic] 这一行走不到
        // （走满步数也算环），留着是为了改 [isCyclic] 时不会静默丢行。
        for (id in order) if (emitted.add(id)) out += Node(id, 0)
        return out
    }

    private fun emit(
        id: String,
        depth: Int,
        childrenOf: Map<String?, List<String>>,
        out: MutableList<Node>,
        emitted: MutableSet<String>,
    ) {
        if (!emitted.add(id)) return // 防重入（也是环的第二道闸）
        out += Node(id, depth)
        for (child in childrenOf[id].orEmpty()) emit(child, depth + 1, childrenOf, out, emitted)
    }

    /** 沿父链上溯，回到自己 ⇒ 有环。步数以 map 大小封顶，绝不无限走。 */
    private fun isCyclic(
        start: String,
        parentOf: Map<String, String>,
    ): Boolean {
        var cur = parentOf[start] ?: return false
        repeat(parentOf.size) {
            if (cur == start) return true
            cur = parentOf[cur] ?: return false
        }
        return true // 走满了还没到头 ⇒ 也当成环
    }
}
