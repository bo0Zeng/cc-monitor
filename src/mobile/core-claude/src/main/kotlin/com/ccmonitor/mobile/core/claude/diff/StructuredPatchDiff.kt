package com.ccmonitor.mobile.core.claude.diff

import com.ccmonitor.mobile.core.claude.model.PatchHunk

enum class DiffType { ADD, DEL, CTX }

data class DiffRow(
    val type: DiffType,
    val text: String,
    val oldNo: Int?,
    val newNo: Int?,
)

/** rows 受 maxLines 截断；addCount/delCount 为全量（不受截断影响）。 */
data class DiffResult(
    val rows: List<DiffRow>,
    val truncated: Boolean,
    val addCount: Int,
    val delCount: Int,
)

/** 单行字符上限（防 minified 单行几十万字符撑爆布局）。 */
private const val MAX_CHARS_PER_LINE = 2000

/** 剥尾随 CR（CRLF 源按 `\n` 切行后每行残留的 `\r`）再截长行——保真但不让不可见控制符/超长行进渲染。 */
private fun cleanCell(text: String): String {
    val noCr = if (text.endsWith("\r")) text.substring(0, text.length - 1) else text
    return if (noCr.length > MAX_CHARS_PER_LINE) noCr.substring(0, MAX_CHARS_PER_LINE) + "…" else noCr
}

/**
 * `structuredPatch`（Edit/Write 的权威 hunk）→ [DiffResult]（喂 `DiffView` 渲染；patch 是核心给的，手机不自己算 diff）。
 * 真行号从各 hunk 的 oldStart/newStart 起算；`lines` 首字符 `+`/`-`/` ` 定 ADD/DEL/CTX，
 * `\`（统一 diff 的 `\ No newline at end of file` 标记）跳过不占行，其余无前缀行当 CTX 原样。
 */
fun structuredPatchToDiff(
    hunks: List<PatchHunk>,
    maxLines: Int = 400,
): DiffResult {
    val rows = ArrayList<DiffRow>()
    var adds = 0
    var dels = 0
    for (h in hunks) {
        // hunk 头（CTX 行、无行号）——多 hunk 视觉分隔 + 显跳段。
        rows.add(DiffRow(DiffType.CTX, "@@ -${h.oldStart},${h.oldLines} +${h.newStart},${h.newLines} @@", null, null))
        var oldNo = h.oldStart
        var newNo = h.newStart
        for (line in h.lines) {
            when (line.firstOrNull()) {
                '+' -> {
                    rows.add(DiffRow(DiffType.ADD, cleanCell(line.substring(1)), null, newNo))
                    newNo++
                    adds++
                }
                '-' -> {
                    rows.add(DiffRow(DiffType.DEL, cleanCell(line.substring(1)), oldNo, null))
                    oldNo++
                    dels++
                }
                ' ' -> {
                    rows.add(DiffRow(DiffType.CTX, cleanCell(line.substring(1)), oldNo, newNo))
                    oldNo++
                    newNo++
                }
                '\\' -> {
                    // 统一 diff `\ No newline at end of file` 标记：零行——不出 row、不推进行号、不计数。
                }
                else -> {
                    // 无前缀（含空行）→ 上下文原样。
                    rows.add(DiffRow(DiffType.CTX, cleanCell(line), oldNo, newNo))
                    oldNo++
                    newNo++
                }
            }
        }
    }
    return if (rows.size > maxLines) {
        DiffResult(rows.take(maxLines), truncated = true, addCount = adds, delCount = dels)
    } else {
        DiffResult(rows, truncated = false, addCount = adds, delCount = dels)
    }
}
