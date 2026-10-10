package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.core.claude.model.ContentBlock
import com.ccmonitor.mobile.core.claude.model.JsonlRecord
import com.ccmonitor.mobile.core.claude.model.MainBranch
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** 骨架扫描纯逻辑（解析 / 骨架主线==全量主线 / 窗口起始 / 命令）。 */
class SkeletonScanTest {
    @Test fun parseTsv() {
        val tsv = "0\tu1\t\tuser\t2024-01-01T00:00:00Z\t0\n42\tu2\tu1\tassistant\t2024-01-01T00:00:01Z\t0\n100\n"
        val res = SkeletonScan.parse(tsv)
        assertEquals(2, res.records.size)
        assertEquals(SkeletonRecord(0, "u1", "", "user", "2024-01-01T00:00:00Z", false), res.records[0])
        assertEquals(42L, res.records[1].byteOffset)
        assertEquals("u1", res.records[1].parentUuid)
        assertEquals(100L, res.eofBytes) // END 总字节
    }

    @Test fun parseSkipsBadLines() {
        val tsv = "notanumber\tu\t\tuser\tts\t0\n\n0\tu1\t\tuser\tts\t0\n"
        assertEquals(1, SkeletonScan.parse(tsv).records.size) // 坏 offset 行 + 空行跳过
    }

    @Test fun parseInterruptFlag() {
        val res = SkeletonScan.parse("0\tu\t\tuser\tts\t1\n")
        assertTrue(res.records[0].isInterrupt)
    }

    @Test fun parseEofFallbackToLastRecord() {
        // 无 END 行 → eof 回退为最后记录 byteOffset。
        val res = SkeletonScan.parse("0\tu1\t\tuser\tts\t0\n50\tu2\tu1\tassistant\tts\t0\n")
        assertEquals(50L, res.eofBytes)
    }

    @Test fun toBranchRecordNeedsUuidAndTs() {
        assertNull(SkeletonScan.toBranchRecord(SkeletonRecord(0, "", "", "user", "ts", false))) // 无 uuid
        assertNull(SkeletonScan.toBranchRecord(SkeletonRecord(0, "u", "", "user", "", false))) // 无 ts
        val b = SkeletonScan.toBranchRecord(SkeletonRecord(0, "u", "p", "user", "ts", true))
        assertEquals("u", b?.uuid)
        assertEquals("p", b?.parentUuid)
        assertEquals(true, b?.isInterrupt)
    }

    @Test fun emptyParentBecomesNull() {
        assertNull(SkeletonScan.toBranchRecord(SkeletonRecord(0, "u", "", "user", "ts", false))?.parentUuid)
    }

    // 有 uuid+ts 的非 BRANCH_TYPES 记录改作 pass-through（type="unknown"），与全量 Unknown 逃生舱对齐——不孤儿化其后对话。
    @Test fun toBranchRecordPassesThroughNonBranchTypesWithUuid() {
        for (t in listOf("summary", "agent-name", "mode", "file-history-snapshot")) {
            assertEquals("$t → pass-through type=unknown", "unknown", SkeletonScan.toBranchRecord(SkeletonRecord(0, "u", "p", t, "ts", false))?.type)
        }
        for (t in listOf("user", "assistant", "attachment", "system")) {
            assertEquals("$t 原样纳入", t, SkeletonScan.toBranchRecord(SkeletonRecord(0, "u", "p", t, "ts", false))?.type)
        }
        // ai-title/custom-title 全量路由到 Title（uuid 恒 null 永不入链）→ 骨架即便带 uuid 也须排除（两路对齐）。
        for (t in listOf("ai-title", "custom-title")) {
            assertNull("$t 应排除（与全量 Title 对齐）", SkeletonScan.toBranchRecord(SkeletonRecord(0, "u", "p", t, "ts", false)))
        }
        // 无 uuid → 仍排除（无数据、不可链）。
        assertNull(SkeletonScan.toBranchRecord(SkeletonRecord(0, "", "p", "summary", "ts", false)))
    }

    // 全量 MainBranch.extractBranchRecord 与骨架 toBranchRecord 的类型准入必须一致（否则两路主线可能不同）。
    // 给 JsonlRecord 加新结构类型、只改一处的话此测会红；加新类型时把它加进 recs。
    @Test fun extractAndSkeletonAcceptSameTypes() {
        val ts = "2024-01-01T00:00:00Z"
        val recs =
            listOf(
                JsonlRecord.User("u", null, listOf(ContentBlock.Text("x")), false, ts, null, null),
                JsonlRecord.Assistant("a", null, listOf(ContentBlock.Text("x")), "m", ts, null, false),
                JsonlRecord.System("s", null, null, null, false, ts),
                JsonlRecord.Attachment("at", null, ts),
                JsonlRecord.Title("t", null),
                JsonlRecord.Unknown("mode"), // 无 uuid → 两路都拒
                JsonlRecord.Unknown("future-type", "uk", "p", ts), // 有 uuid → 两路都作 pass-through 纳入
            )
        for (r in recs) {
            val full = MainBranch.extractBranchRecord(r)
            val skelType = full?.type ?: "unknown-type" // full 拒 → 该记录 uuid 多为空、类型也不入链
            val skel = SkeletonRecord(0, r.uuid ?: "", r.parentUuid ?: "", skelType, r.timestamp ?: "", false)
            assertEquals(
                "${r::class.simpleName} 两路准入须一致",
                full != null,
                SkeletonScan.toBranchRecord(skel) != null,
            )
        }
    }

    // 关键不变量：骨架算的主线 == 全量记录算的主线（同一 DAG）。用 ESC-fork 场景。
    @Test fun skeletonMainBranchEqualsFullMainBranch() {
        // DAG: root(u) → a1(assistant) ；root 下再 fork 一个废弃 ESC 回撤 u2（无 assistant 子）→ 应被折叠。
        // 全量记录
        fun user(
            u: String,
            p: String?,
            ts: String,
        ) = JsonlRecord.User(u, p, listOf(ContentBlock.Text("hi")), false, ts, null, null)

        fun asst(
            u: String,
            p: String?,
            ts: String,
        ) = JsonlRecord.Assistant(u, p, listOf(ContentBlock.Text("ok")), "m", ts, null, false)
        val full =
            listOf(
                user("r", null, "2024-01-01T00:00:00Z"),
                asst("a1", "r", "2024-01-01T00:00:05Z"),
                user("r2", null, "2024-01-01T00:00:02Z"), // 第二 root（plain user 死胡同、无 assistant）→ 折叠
            )
        val fullMain = MainBranch.resolve(full).mapNotNull { it.uuid }.toSet()
        // 对应骨架（同 uuid/parent/ts/type）
        val skel =
            listOf(
                SkeletonRecord(0, "r", "", "user", "2024-01-01T00:00:00Z", false),
                SkeletonRecord(10, "a1", "r", "assistant", "2024-01-01T00:00:05Z", false),
                SkeletonRecord(20, "r2", "", "user", "2024-01-01T00:00:02Z", false),
            )
        val skelMain = SkeletonScan.mainBranchSkeleton(skel).map { it.uuid }.toSet()
        assertEquals(fullMain, skelMain) // 骨架主线 == 全量主线
        assertTrue("r" in skelMain && "a1" in skelMain)
        assertTrue("r2" !in skelMain) // 死胡同折叠
    }

    @Test fun windowStartOffset() {
        // 10 条主线，offset 0,100,…,900；eof=1000。maxBytes=0=关阀 → 纯计数窗口。
        val main =
            (0 until 10).map { SkeletonRecord(it * 100L, "u$it", if (it == 0) "" else "u${it - 1}", "user", "ts$it", false) }
        assertEquals(0L, SkeletonScan.windowStartOffset(main, 10, 1000, 0)) // 主线==n → 全量
        assertEquals(0L, SkeletonScan.windowStartOffset(main, 20, 1000, 0)) // 主线<n → 全量
        assertEquals(700L, SkeletonScan.windowStartOffset(main, 3, 1000, 0)) // 最后 3 条 → idx7 offset=700
    }

    // 窗口首载字节封顶。
    @Test fun windowStartOffsetByteCap() {
        val main =
            (0 until 10).map { SkeletonRecord(it * 100L, "u$it", if (it == 0) "" else "u${it - 1}", "user", "ts$it", false) }
        // 计数窗口本就 ≤ 封顶 → 不变（[700,1000]=300 ≤ 500）。
        assertEquals(700L, SkeletonScan.windowStartOffset(main, 3, 1000, 500))
        // 计数窗口(全量[0,1000]=1000)超封顶 250 → 收紧到 byteOffset ≥ (1000-250)=750 的最早记录 = idx8 offset=800。
        assertEquals(800L, SkeletonScan.windowStartOffset(main, 10, 1000, 250))
        // 组合：计数窗口非零(n=5→countStart=idx5=500)且仍超封顶 250 → 从 500 再收紧到 ≥750 的 800（恒 ≥ countStart、绝不多载）。
        assertEquals(800L, SkeletonScan.windowStartOffset(main, 5, 1000, 250))
        // 尾巨记录：eof 远超末条(900) → floor=9000 无记录满足 → 退最后一条 offset=900（最小行边界窗口）。
        assertEquals(900L, SkeletonScan.windowStartOffset(main, 10, 10_000, 1000))
        // 空主线 → 0。
        assertEquals(0L, SkeletonScan.windowStartOffset(emptyList(), 5, 1000, 500))
    }

    @Test fun commands() {
        assertTrue(SkeletonScan.skeletonCommand("/a/b.jsonl").startsWith("LC_ALL=C awk '"))
        assertTrue(SkeletonScan.skeletonCommand("/a/b.jsonl").endsWith("'/a/b.jsonl'"))
        assertEquals("LC_ALL=C tail -c +101 '/a/b.jsonl'", SkeletonScan.windowContentCommand("/a/b.jsonl", 100))
        // 单引号路径转义
        assertTrue(SkeletonScan.windowContentCommand("/a'b.jsonl", 0).contains("'/a'\\''b.jsonl'"))
    }

    // awk 跑在远端，JVM 测不了语义，这里钉命令形状。
    // 无尾 `\n` 的末行（Claude 写到一半）下 `off` 会超真实 EOF 1 字节 → tail 跳过接缝首字节、丢记录；
    // 吐 `eof=off+length($0)`（内容结束处）则 eofBytes ≤ 真实 EOF（末行有 `\n` 欠 1 无害、无 `\n` 精确）。
    @Test fun awkEndEmitsContentEndNotLineEnd() {
        assertTrue("主块须算 eof=行内容结束处", SkeletonScan.AWK_SCRIPT.contains("eof=off+length(\$0)"))
        assertTrue("END 吐 eof", SkeletonScan.AWK_SCRIPT.contains("END{print eof}"))
        assertFalse("不得回退 END{print off}（含末行 +1、无尾 \\n 时超真实 EOF）", SkeletonScan.AWK_SCRIPT.contains("END{print off}"))
        assertTrue("BEGIN 初始化 eof（空文件兜底吐 0）", SkeletonScan.AWK_SCRIPT.contains("BEGIN{off=0;eof=0}"))
    }
}
