package com.ccmonitor.mobile.core.claude.link.wire

/**
 * 手上那一份带键集合：记号 ＋ 这一窗的先后 ＋ 各项 ＋ 头；空的那一份 ＝ 没问过。
 * [apply] 照后端那一答换、没有判断，与 Rust 那一份（`cmd_wire::sync::Hand::apply`）照同一份金样 `sync-patch.golden.jsonl`。
 */
data class SyncHand(
    val held: String? = null,
    val order: List<String> = emptyList(),
    val items: Map<String, Any> = emptyMap(),
    val head: SyncHead? = null,
) {
    /** `same` ⇒ 只换记号；`whole` ⇒ 从空的起；带了 `order` / `head` 就换；`up` 盖上；只留键在 `order` 里的。 */
    fun apply(p: SyncPatch): SyncHand {
        if (p.same) return copy(held = p.held)
        val base = if (p.whole) SyncHand() else this
        val order = p.order ?: base.order
        val keep = order.toSet()
        return SyncHand(p.held, order, (base.items + p.up).filterKeys { it in keep }, p.head ?: base.head)
    }
}
