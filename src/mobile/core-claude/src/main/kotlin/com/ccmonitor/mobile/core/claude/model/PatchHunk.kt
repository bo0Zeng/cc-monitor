package com.ccmonitor.mobile.core.claude.model

/** Edit/Write 的结构化 diff 块（`toolUseResult.structuredPatch[]`），供保真 diff 渲染。 */
data class PatchHunk(
    val oldStart: Int,
    val oldLines: Int,
    val newStart: Int,
    val newLines: Int,
    val lines: List<String>,
)
