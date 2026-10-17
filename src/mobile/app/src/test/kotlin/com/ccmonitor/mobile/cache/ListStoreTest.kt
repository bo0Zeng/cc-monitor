package com.ccmonitor.mobile.cache

import com.ccmonitor.mobile.core.claude.link.RawJson
import com.ccmonitor.mobile.core.claude.link.wire.SyncHand
import com.ccmonitor.mobile.core.claude.link.wire.SyncHead
import com.ccmonitor.mobile.core.claude.link.wire.SyncPatch
import com.ccmonitor.mobile.core.claude.link.wire.Wire
import com.ccmonitor.mobile.core.data.db.CachedSession
import com.ccmonitor.mobile.core.data.db.ListHold
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import java.io.File

/**
 * 会话列表缓存那一层照金样换：库里那一份读回成手上那一份、照那台的一答换、存回去再读回来，与金样要的那一份逐条相等
 * （`sync-patch.golden.jsonl`，后端 `common/views.rs` 真答出来写的；`SyncHand.apply` 与 Rust 那一份读同一份）。
 */
class ListStoreTest {
    @Test
    fun `存 · 读 · 换 · 再存再读，照金样逐条`() {
        val cases = golden()
        // 正控：金样里整份 · 没变 · 只给变了的三种答都有。
        val kinds =
            cases
                .map { (_, p, _) ->
                    when {
                        p.whole -> "whole"
                        p.same -> "same"
                        else -> "part"
                    }
                }.toSet()
        assertEquals(setOf("whole", "same", "part"), kinds)
        for ((had, p, want) in cases) {
            val start = through(had) ?: SyncHand()
            assertEquals(want, through(start.apply(p)))
        }
    }

    @Test
    fun `还没答过（没有记号）⇒ 不存；库里的行没有记号那一行 ⇒ 读回来照画、不带记号`() {
        assertNull(storedOf("h", SyncHand(order = listOf("a"), items = mapOf("a" to mapOf("k" to 1.0)))))
        val back = handOf(null, listOf(CachedSession("h", "a", "{\"k\":1}", 0, null)))
        assertEquals(SyncHand(null, listOf("a"), mapOf("a" to mapOf("k" to 1.0)), null), back)
    }

    /** 存进库那几格再读回来（同 Room 那一层搬的：行原样 JSON 文本 · 先后 · 记号与头）；没有记号 ⇒ 不存（`null`）。 */
    private fun through(hand: SyncHand): SyncHand? {
        val (rows, hold) = storedOf("h", hand) ?: return null
        val listed = rows.mapIndexed { i, (sid, row) -> CachedSession("h", sid, row, i, null) }
        return handOf(ListHold(hold.hostId, hold.held, hold.head), listed)
    }

    @Suppress("UNCHECKED_CAST")
    private fun golden(): List<Triple<SyncHand, SyncPatch, SyncHand>> {
        val root = generateSequence(File("").absoluteFile) { it.parentFile }.first { File(it, "tests/__fixtures__").isDirectory && File(it, "src/backend/lib.rs").isFile }
        return File(root, "tests/__fixtures__/sync-patch.golden.jsonl").readLines().filter(String::isNotBlank).map { line ->
            val c = RawJson.read(line) as Map<String, Any?>
            Triple(hand(c["hand"]), Wire.bind<SyncPatch>(c["patch"]) ?: error("这一答装不进 SyncPatch：$line"), hand(c["want"]))
        }
    }

    @Suppress("UNCHECKED_CAST")
    private fun hand(v: Any?): SyncHand {
        val m = v as Map<String, Any?>
        return SyncHand(m["held"] as String?, m["order"] as List<String>, m["items"] as Map<String, Any>, m["head"]?.let { Wire.bind<SyncHead>(it) })
    }
}
