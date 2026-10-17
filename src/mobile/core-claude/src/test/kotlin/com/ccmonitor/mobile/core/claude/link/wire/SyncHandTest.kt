package com.ccmonitor.mobile.core.claude.link.wire

import com.ccmonitor.mobile.core.claude.link.Fixtures
import com.ccmonitor.mobile.core.claude.link.Json
import org.junit.Assert.assertEquals
import org.junit.Test

/** [SyncHand.apply] 照金样逐条（后端 `common/views.rs` 真答出来写的那一份；Rust 的 `Hand::apply` 读同一份）。 */
class SyncHandTest {
    @Test
    fun applyFollowsEveryGoldenLine() {
        val cases =
            Fixtures
                .text("sync-patch.golden.jsonl")
                .lines()
                .filter(String::isNotBlank)
                .map { Json.obj(it) ?: error("不是 JSON 对象：$it") }
        val patches = cases.map { Wire.bind<SyncPatch>(it["patch"]) ?: error("这一答装不进 SyncPatch：${it["patch"]}") }
        // 正控：三种答（整份 · 没变 · 只给变了的）金样里都有，本条不在空转。
        val kinds =
            patches
                .map {
                    when {
                        it.whole -> "whole"
                        it.same -> "same"
                        else -> "part"
                    }
                }.toSet()
        assertEquals(setOf("whole", "same", "part"), kinds)
        cases.zip(patches).forEach { (c, p) -> assertEquals(c["case"] as String, hand(c["want"]), hand(c["hand"]).apply(p)) }
    }

    @Suppress("UNCHECKED_CAST")
    private fun hand(v: Any?): SyncHand {
        val m = v as Map<String, Any?>
        return SyncHand(m["held"] as String?, m["order"] as List<String>, m["items"] as Map<String, Any>, m["head"]?.let { Wire.bind<SyncHead>(it) })
    }
}
