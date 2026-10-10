package com.ccmonitor.mobile.core.claude.model

import com.ccmonitor.mobile.core.claude.model.MainBranch.BranchRecord
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** 主线计算的 12 个黄金用例（回撤折叠、/compact 多 root、重投幂等）+ resolve() 包装。 */
class MainBranchTest {
    private fun rec(
        uuid: String,
        parent: String?,
        ts: String,
        type: String,
        isInterrupt: Boolean = false,
    ) =
        BranchRecord(uuid, parent, ts, type, isInterrupt)

    /** computeMainBranch → 排序后的 uuid 列表，便于断言。 */
    private fun onMain(records: List<BranchRecord>) = MainBranch.computeMainBranch(records).sorted()

    // 1. 单链单 root → 全 on-main
    @Test fun singleChainAllOnMain() {
        val r =
            listOf(
                rec("A", null, "t01", "user"),
                rec("B", "A", "t02", "assistant"),
                rec("C", "B", "t03", "user"),
                rec("D", "C", "t04", "assistant"),
            )
        assertEquals(listOf("A", "B", "C", "D"), onMain(r))
    }

    // 逃生舱：链中间一条 type="unknown"（未知/解析失败记录 pass-through）不切断链——其后对话仍在主线、不孤儿化折叠。
    @Test fun unknownPassThroughKeepsDescendantsOnMain() {
        val r =
            listOf(
                rec("A", null, "t01", "user"),
                rec("B", "A", "t02", "assistant"),
                rec("U", "B", "t03", "unknown"), // 逃生舱 pass-through（非 conv，仅连通链）
                rec("C", "U", "t04", "user"),
                rec("D", "C", "t05", "assistant"),
            )
        assertEquals("单链贯通：U 及其后 C/D 全在主线", listOf("A", "B", "C", "D", "U"), onMain(r))
    }

    // 2. 树内 fork → latest-descendant 晚的赢
    @Test fun forkLatestSiblingWins() {
        val r =
            listOf(
                rec("A", null, "t01", "user"),
                rec("B", "A", "t02", "assistant"),
                rec("C1", "B", "t03", "user"), // 被回退
                rec("C2", "B", "t05", "user"), // 赢家
            )
        assertEquals(listOf("A", "B", "C2"), onMain(r))
    }

    // 3. 首条 claude 回复前就回撤 → 无 assistant 的废弃 root 折叠
    @Test fun firstMsgRetractBeforeResponseFolded() {
        val r =
            listOf(
                rec("R1", null, "t01", "user"), // 废弃首条
                rec("R2", null, "t03", "user"), // 重发
                rec("A2", "R2", "t04", "assistant"),
            )
        assertEquals(listOf("A2", "R2"), onMain(r))
    }

    // 4. 三连回撤（打断后回撤）→ 折叠前两、留最后
    @Test fun tripleRetractKeepLast() {
        val r =
            listOf(
                rec("R1", null, "t01", "user"),
                rec("A1", "R1", "t02", "assistant"),
                rec("I1", "A1", "t03", "user", isInterrupt = true),
                rec("R2", null, "t04", "user"),
                rec("A2", "R2", "t05", "assistant"),
                rec("I2", "A2", "t06", "user", isInterrupt = true),
                rec("R3", null, "t07", "user"), // 活跃
                rec("A3", "R3", "t08", "assistant"),
            )
        assertEquals(listOf("A3", "R3"), onMain(r))
    }

    // 5. /compact 多 root → 全保留（关键：不误折 pre-compact 历史）
    @Test fun compactMultiRootAllKept() {
        val r =
            listOf(
                rec("P1", null, "t01", "user"),
                rec("PA", "P1", "t02", "assistant"),
                rec("S", null, "t03", "system"), // /compact 边界
                rec("CS", "S", "t04", "user"),
                rec("CA", "CS", "t05", "assistant"),
            )
        assertEquals(listOf("CA", "CS", "P1", "PA", "S"), onMain(r))
    }

    // 6. 链断 root（祖先被裁）完整对话 → 保留
    @Test fun chainBreakRootKept() {
        val r =
            listOf(
                rec("X", "MISSING", "t01", "user"),
                rec("XA", "X", "t02", "assistant"),
                rec("Y", null, "t03", "user"),
                rec("YA", "Y", "t04", "assistant"),
            )
        assertEquals(listOf("X", "XA", "Y", "YA"), onMain(r))
    }

    // 7. 打断后回撤 root，末尾跟 /model 等 system（ts 更晚但非会话）→ 仍折叠
    @Test fun retractWithTrailingSystemStillFolded() {
        val r =
            listOf(
                rec("R1", null, "t01", "user"),
                rec("A1", "R1", "t02", "assistant"),
                rec("I1", "A1", "t03", "user", isInterrupt = true),
                rec("S1", "I1", "t04", "system"),
                rec("S2", "S1", "t05", "system"),
                rec("R2", null, "t06", "user"),
                rec("A2", "R2", "t07", "assistant"),
            )
        assertEquals(listOf("A2", "R2"), onMain(r))
    }

    // 8. 唯一 root 即便死胡同也保留（别折掉唯一内容）
    @Test fun singleDeadEndRootKept() {
        assertEquals(listOf("R1"), onMain(listOf(rec("R1", null, "t01", "user"))))
    }

    // 9. 空集 → 空
    @Test fun emptyIsEmpty() {
        assertTrue(onMain(emptyList()).isEmpty())
    }

    // 10. root 级毒化：pre-compact 树里 1 条 attachment 重投 → 结果不变
    @Test fun dupAttachmentInPreCompactTreeUnchanged() {
        val r =
            listOf(
                rec("R1", null, "t01", "user"),
                rec("A1", "R1", "t02", "assistant"),
                rec("ATT", "A1", "t03", "attachment"),
                rec("U2", "ATT", "t04", "user"),
                rec("A2", "U2", "t05", "assistant"),
                rec("S", null, "t06", "system"),
                rec("CS", "S", "t07", "user"),
                rec("CA", "CS", "t08", "assistant"),
            )
        val clean = onMain(r)
        assertEquals(listOf("A1", "A2", "ATT", "CA", "CS", "R1", "S", "U2"), clean)
        assertEquals(clean, onMain(r + rec("ATT", "A1", "t03", "attachment")))
    }

    // 11. fork 级毒化：赢家子树里 1 条 attachment 重投 → 赢家不变
    @Test fun dupAttachmentInForkWinnerUnchanged() {
        val r =
            listOf(
                rec("A", null, "t01", "user"),
                rec("B", "A", "t02", "assistant"),
                rec("C2", "B", "t04", "user"), // 真赢家（子树最新 t08）
                rec("ATT", "C2", "t06", "attachment"),
                rec("D2", "ATT", "t08", "assistant"),
                rec("C1", "B", "t05", "user", isInterrupt = true), // 兄弟自身 ts 晚于 C2 自身
            )
        val clean = onMain(r)
        assertEquals(listOf("A", "ATT", "B", "C2", "D2"), clean)
        assertEquals(clean, onMain(r + rec("ATT", "C2", "t06", "attachment")))
    }

    // 12. 全文件重投 → 幂等
    @Test fun fullRedeliveryIdempotent() {
        fun build() =
            listOf(
                rec("R1", null, "t01", "user"),
                rec("A1", "R1", "t02", "assistant"),
                rec("I1", "A1", "t03", "user", isInterrupt = true),
                rec("R2", null, "t04", "user"),
                rec("A2", "R2", "t05", "assistant"),
                rec("S", null, "t06", "system"),
                rec("CS", "S", "t07", "user"),
            )
        assertEquals(onMain(build()), onMain(build() + build()))
    }

    // --- resolve() 在 JsonlRecord 层的包装：抽链 + 过滤 + 文件序 ---
    @Test fun resolveFiltersToMainBranchInFileOrder() {
        fun user(
            uuid: String,
            parent: String?,
            ts: String,
            text: String = "x",
        ) =
            JsonlRecord.User(uuid, parent, listOf(ContentBlock.Text(text)), false, ts, null, null)

        fun asst(
            uuid: String,
            parent: String?,
            ts: String,
        ) =
            JsonlRecord.Assistant(uuid, parent, listOf(ContentBlock.Text("ok")), "m", ts, null, false)

        val records =
            listOf(
                JsonlRecord.Title("t", "s"), // 无 uuid → 丢
                user("A", null, "t01"),
                asst("B", "A", "t02"),
                user("C1", "B", "t03"), // 被回退兄弟 → off-main
                user("C2", "B", "t05", "redo"), // 赢家
                JsonlRecord.Unknown("permission-mode"), // 无 uuid → 丢
            )
        val main = MainBranch.resolve(records).map { it.uuid }
        assertEquals(listOf("A", "B", "C2"), main) // 文件序，C1 折叠，标题/unknown 丢弃
    }

    // resolve 识别 interrupt 文本，折叠废弃 root
    @Test fun resolveDetectsInterruptText() {
        fun user(
            uuid: String,
            parent: String?,
            ts: String,
            text: String,
        ) =
            JsonlRecord.User(uuid, parent, listOf(ContentBlock.Text(text)), false, ts, null, null)

        fun asst(
            uuid: String,
            parent: String?,
            ts: String,
        ) =
            JsonlRecord.Assistant(uuid, parent, listOf(ContentBlock.Text("ok")), "m", ts, null, false)

        val records =
            listOf(
                user("R1", null, "t01", "first try"),
                asst("A1", "R1", "t02"),
                user("I1", "A1", "t03", "[Request interrupted by user]"), // 打断叶子 → R1 root 折叠
                user("R2", null, "t04", "redo"),
                asst("A2", "R2", "t05"),
            )
        assertEquals(listOf("R2", "A2"), MainBranch.resolve(records).map { it.uuid })
    }
}
