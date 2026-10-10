package com.ccmonitor.mobile.core.claude.model

/*
 * 把 [RecordClassifier] 输出里连续的非交互 ToolCall 折叠成一张 [RenderUnit.ToolGroup]。
 * 纯函数，作用在 classify 输出上，不碰 classify 的增量缓存。
 */

/**
 * 交互工具（AskUserQuestion/ExitPlanMode）在等用户决定：不入组、默认展开，否则折起来看着像 LLM 还在跑。
 */
private val INTERACTIVE_TOOLS = setOf("AskUserQuestion", "ExitPlanMode")

fun isInteractiveTool(name: String): Boolean = name in INTERACTIVE_TOOLS

/**
 * 折叠连续的非交互 ToolCall（run 长度 ≥2）成 [RenderUnit.ToolGroup]（key=首 call key、稳定）。
 * 单个 ToolCall / 交互 ToolCall / 非工具 unit 原样透传（交互 ToolCall 断开当前 run）。
 * 幂等纯函数：相同输入→相同输出；作用在增量 classify 的 units 上，增量输出==全量输出的分组一致。
 */
fun groupTools(units: List<RenderUnit>): List<RenderUnit> {
    val out = ArrayList<RenderUnit>(units.size)
    var run = ArrayList<RenderUnit.ToolCall>()

    fun flush() {
        when (run.size) {
            0 -> {}
            1 -> out.add(run[0]) // 单工具不入组，原样
            else -> out.add(RenderUnit.ToolGroup(run[0].key, run.toList(), run[0].sourceUuid))
        }
        run = ArrayList()
    }

    for (u in units) {
        if (u is RenderUnit.ToolCall && !isInteractiveTool(u.name)) {
            run.add(u)
        } else {
            flush()
            out.add(u) // 交互 ToolCall / 非工具 unit：断 run 后原样（交互工具单独 render、默认展开）
        }
    }
    flush()
    return out
}
