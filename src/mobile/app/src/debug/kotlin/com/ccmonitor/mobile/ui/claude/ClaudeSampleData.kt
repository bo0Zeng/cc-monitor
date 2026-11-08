package com.ccmonitor.mobile.ui.claude

import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.wire.gen.Doc
import com.ccmonitor.mobile.core.wire.gen.Inl
import com.ccmonitor.mobile.core.wire.gen.InlNode
import com.ccmonitor.mobile.core.wire.gen.LineDiff
import com.ccmonitor.mobile.core.wire.gen.LineDiffKind
import com.ccmonitor.mobile.core.wire.gen.LineDiffRow
import com.ccmonitor.mobile.core.wire.gen.MathBox
import com.ccmonitor.mobile.core.wire.gen.MathItem
import com.ccmonitor.mobile.core.wire.gen.MdBlock
import com.ccmonitor.mobile.core.wire.gen.MdItem

// 样本正文是后端解析好的那一形（块树）：真的树由核心 `md_core::parse` 出，这里照那一形摆几块。
private fun doc(vararg blocks: MdBlock) = Doc(blocks.toList(), emptyList())

private fun p(vararg k: Inl) = MdBlock.P(k.toList())

private fun s(text: String): Inl = Inl.Text(text)

private fun b(text: String): Inl = Inl.InlNode(InlNode.B(listOf(Inl.Text(text))))

private fun c(code: String): Inl = Inl.InlNode(InlNode.C(code))

/** `E=mc^2` 的画单（照 md-core 金样那一份抄来）；另两条不带画单，照原文排那一路。 */
private val emc2 =
    MathBox(
        w = 3840,
        h = 814,
        d = 0,
        items =
            listOf(
                MathItem.G("Math-Italic", 69, 0, 814, 1000),
                MathItem.G("Main-Regular", 61, 1074, 814, 1000),
                MathItem.G("Math-Italic", 109, 2129, 814, 1000),
                MathItem.G("Math-Italic", 99, 3007, 814, 1000),
                MathItem.G("Main-Regular", 50, 3440, 451, 700),
            ),
    )

private fun row(
    kind: LineDiffKind,
    text: String,
    oldNo: Int?,
    newNo: Int?,
) = LineDiffRow(kind = kind, text = text, oldNo = oldNo, newNo = newNo)

/** 样本里的行差（真的行差由核心算，这里照那一形摆几行）。 */
private fun sampleDiff(
    stat: String,
    vararg rows: LineDiffRow,
) = LineDiff(rows = rows.toList(), shown = rows.size, truncated = false, added = rows.count { it.kind == LineDiffKind.ADD }, removed = rows.count { it.kind == LineDiffKind.DEL }, stat = stat)

/** debug 预览用样本，覆盖 4 种 RenderUnit + diff / pending / error 工具态。 */
fun sampleRenderUnits(): List<RenderUnit> =
    listOf(
        RenderUnit.UserText("s1", "帮我把 LineDiff 的 cellBudget 守卫加上单测，并解释下为什么要 toLong()。", "u1"),
        RenderUnit.Thinking(
            "s2",
            "用户担心大矩阵 m*n 溢出 Int。需要在分配矩阵前用 Long 比较 cellBudget，超限退化为整删+整增。",
            "a1",
        ),
        RenderUnit.AssistantMarkdown(
            "s3",
            doc(
                p(s("好的，我来加 "), b("cellBudget 守卫"), s("。要点：")),
                MdBlock.List(
                    1,
                    true,
                    listOf(
                        MdItem(null, listOf(p(s("在分配 "), c("IntArray((m+1)*width)"), s(" "), b("之前"), s("判断")))),
                        MdItem(null, listOf(p(s("用 "), c("m.toLong() * n"), s(" 避免 "), c("Int"), s(" 溢出（100k × 100k 远超 "), c("Int.MAX"), s("）")))),
                        MdItem(null, listOf(p(s("超限退化为全删 + 全增")))),
                    ),
                ),
                MdBlock.Code(
                    "kotlin",
                    "if (m.toLong() * n > cellBudget) {\n    // 退化路径：整删 + 整增\n    return degraded(oldLines, newLines)\n}",
                    false,
                ),
                MdBlock.Quote(listOf(p(s("注：阈值默认 4_000_000 格，约对应 2000×2000 行。")))),
            ),
            "",
            "a1",
        ),
        RenderUnit.ToolCall(
            key = "s4",
            toolUseId = "t1",
            name = "Bash",
            summary = "./gradlew :core-claude:testDebugUnitTest --console=plain",
            resultText = "BUILD SUCCESSFUL in 7s\nTOTAL tests=37 failures+errors=0 skipped=0",
            isError = false,
            pending = false,
            sourceUuid = "a2",
        ),
        RenderUnit.ToolCall(
            key = "s5",
            toolUseId = "t2",
            name = "Edit",
            summary = "core-claude/src/main/kotlin/.../LineDiff.kt",
            diff =
                sampleDiff(
                    "+1",
                    row(LineDiffKind.ADD, "if (m.toLong() * n > cellBudget) return degraded(oldLines, newLines)", null, 1),
                    row(LineDiffKind.CTX, "val c = IntArray((m + 1) * width)", 1, 2),
                    row(LineDiffKind.CTX, "for (i in 1..m) {", 2, 3),
                ),
            patchFilePath = "core-claude/src/main/kotlin/.../LineDiff.kt",
            resultText = "已应用编辑。",
            isError = false,
            pending = false,
            sourceUuid = "a3",
        ),
        RenderUnit.ToolCall(
            key = "s6",
            toolUseId = "tm",
            name = "MultiEdit",
            summary = "core-claude/.../MainBranch.kt",
            diff =
                sampleDiff(
                    "+2 −2",
                    row(LineDiffKind.HEAD, "@@ -1,1 +1,1 @@", null, null),
                    row(LineDiffKind.DEL, "fun resolve(records) = records", 1, null),
                    row(LineDiffKind.ADD, "fun resolve(records) = computeMainBranch(records)", null, 1),
                    row(LineDiffKind.HEAD, "@@ -9,1 +9,1 @@", null, null),
                    row(LineDiffKind.DEL, "// TODO dedup", 9, null),
                    row(LineDiffKind.ADD, "val seen = HashSet<String>()", null, 9),
                ),
            patchFilePath = "core-claude/.../MainBranch.kt",
            resultText = "已应用 2 处编辑。",
            isError = false,
            pending = false,
            sourceUuid = "a3b",
        ),
        RenderUnit.ToolCall(
            key = "s7",
            toolUseId = "t3",
            name = "Read",
            summary = "/home/pi/project/foo.kt",
            resultText = null,
            isError = false,
            pending = true,
            sourceUuid = "a4",
        ),
        RenderUnit.ToolCall(
            key = "s8",
            toolUseId = "t4",
            name = "Bash",
            summary = "cat /nonexistent",
            resultText = "cat: /nonexistent: No such file or directory",
            isError = true,
            pending = false,
            sourceUuid = "a5",
        ),
        RenderUnit.AssistantMarkdown("s9", doc(p(s("全部测试通过 ✅，已经把守卫加上并验证了边界用例。"))), "", "a6"),
        // 公式样本：块公式单独一块，行内公式占一个行内槽；钱数在树里就是字。
        RenderUnit.AssistantMarkdown(
            "s10",
            doc(
                p(s("块公式：")),
                MdBlock.Math("E=mc^2", emc2),
                MdBlock.Math("x = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}", null),
                p(s("行内 "), Inl.InlNode(InlNode.M("E=mc^2", emc2)), s(" · "), Inl.InlNode(InlNode.M("a^2+b^2", null)), s(" 与价格 \$5 到 \$10。")),
            ),
            "",
            "a7",
        ),
    )
