package com.ccmonitor.mobile.ui.claude

import com.ccmonitor.mobile.core.claude.model.RenderUnit

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
            """
            好的，我来加 **cellBudget 守卫**。要点：

            1. 在分配 `IntArray((m+1)*width)` **之前**判断
            2. 用 `m.toLong() * n` 避免 `Int` 溢出（100k × 100k 远超 `Int.MAX`）
            3. 超限退化为全删 + 全增

            ```kotlin
            if (m.toLong() * n > cellBudget) {
                // 退化路径：整删 + 整增
                return degraded(oldLines, newLines)
            }
            ```

            > 注：阈值默认 4_000_000 格，约对应 2000×2000 行。
            """.trimIndent(),
            "a1",
        ),
        RenderUnit.ToolCall(
            key = "s4",
            toolUseId = "t1",
            name = "Bash",
            input = mapOf("command" to "./gradlew :core-claude:testDebugUnitTest --console=plain"),
            resultText = "BUILD SUCCESSFUL in 7s\nTOTAL tests=37 failures+errors=0 skipped=0",
            isError = false,
            pending = false,
            sourceUuid = "a2",
        ),
        RenderUnit.ToolCall(
            key = "s5",
            toolUseId = "t2",
            name = "Edit",
            input =
                mapOf(
                    "file_path" to "core-claude/src/main/kotlin/.../LineDiff.kt",
                    "old_string" to "val c = IntArray((m + 1) * width)\nfor (i in 1..m) {",
                    "new_string" to "if (m.toLong() * n > cellBudget) return degraded(oldLines, newLines)\nval c = IntArray((m + 1) * width)\nfor (i in 1..m) {",
                ),
            resultText = "已应用编辑。",
            isError = false,
            pending = false,
            sourceUuid = "a3",
        ),
        RenderUnit.ToolCall(
            key = "s6",
            toolUseId = "tm",
            name = "MultiEdit",
            input =
                mapOf(
                    "file_path" to "core-claude/.../MainBranch.kt",
                    "edits" to
                        listOf(
                            mapOf("old_string" to "fun resolve(records) = records", "new_string" to "fun resolve(records) = computeMainBranch(records)"),
                            mapOf("old_string" to "// TODO dedup", "new_string" to "val seen = HashSet<String>()"),
                        ),
                ),
            resultText = "已应用 2 处编辑。",
            isError = false,
            pending = false,
            sourceUuid = "a3b",
        ),
        RenderUnit.ToolCall(
            key = "s7",
            toolUseId = "t3",
            name = "Read",
            input = mapOf("file_path" to "/home/pi/project/foo.kt", "limit" to 100),
            resultText = null,
            isError = false,
            pending = true,
            sourceUuid = "a4",
        ),
        RenderUnit.ToolCall(
            key = "s8",
            toolUseId = "t4",
            name = "Bash",
            input = mapOf("command" to "cat /nonexistent"),
            resultText = "cat: /nonexistent: No such file or directory",
            isError = true,
            pending = false,
            sourceUuid = "a5",
        ),
        RenderUnit.AssistantMarkdown("s9", "全部测试通过 ✅，已经把守卫加上并验证了边界用例。", "a6"),
        // LaTeX 渲染样本。块级 `$$` 必须独占一行；单 `$` 不渲染（markwon 4.6.2 的限制）。
        RenderUnit.AssistantMarkdown(
            "s10",
            """
            LaTeX 块级数学（markwon 4.6.2 仅认独占行的成对美元号）：

            $$
            E = mc^2
            $$

            $$
            x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}
            $$

            行内单美元号不渲染（库限制，留 backlog）：价格 $5 到 $10 按字面显示。
            """.trimIndent(),
            "a7",
        ),
    )
