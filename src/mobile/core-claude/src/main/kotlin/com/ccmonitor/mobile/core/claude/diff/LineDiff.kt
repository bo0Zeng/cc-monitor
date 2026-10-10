package com.ccmonitor.mobile.core.claude.diff

/** 行级 diff。纯函数。 */

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

object LineDiff {
    private const val DEFAULT_MAX_LINES = 400
    private const val DEFAULT_MAX_CHARS = 2000
    private const val DEFAULT_CELL_BUDGET = 4_000_000

    /** CRLF / 裸 CR → LF；剥恰好一个尾随 \n（不贪心）。 */
    private fun normalize(s: String): String {
        var t = s.replace("\r\n", "\n").replace("\r", "\n")
        if (t.endsWith("\n")) t = t.substring(0, t.length - 1)
        return t
    }

    private fun splitLines(normalized: String): List<String> =
        if (normalized == "") emptyList() else normalized.split("\n")

    private fun cap(
        text: String,
        max: Int,
    ): String =
        if (text.length > max) text.substring(0, max) + "…" else text

    private fun uniform(
        lines: List<String>,
        type: DiffType,
        maxLines: Int,
        capChars: Int,
    ): DiffResult {
        val rows = ArrayList<DiffRow>()
        var truncated = false
        for (i in lines.indices) {
            if (rows.size >= maxLines) {
                truncated = true
                break
            }
            val no = i + 1
            rows.add(
                DiffRow(
                    type = type,
                    text = cap(lines[i], capChars),
                    oldNo = if (type == DiffType.ADD) null else no,
                    newNo = if (type == DiffType.DEL) null else no,
                ),
            )
        }
        return DiffResult(
            rows = rows,
            truncated = truncated,
            addCount = if (type == DiffType.ADD) lines.size else 0,
            delCount = if (type == DiffType.DEL) lines.size else 0,
        )
    }

    /** 迭代 LCS（DP + 迭代回溯，无递归 — 防长会话栈溢出）。平局优先 add → 惯例红删在上、绿增在下。 */
    fun diff(
        oldStr: String,
        newStr: String,
        maxLines: Int = DEFAULT_MAX_LINES,
        maxCharsPerLine: Int = DEFAULT_MAX_CHARS,
        cellBudget: Int = DEFAULT_CELL_BUDGET,
    ): DiffResult {
        val oldNorm = normalize(oldStr)
        val newNorm = normalize(newStr)
        if (oldNorm == newNorm) return uniform(splitLines(oldNorm), DiffType.CTX, maxLines, maxCharsPerLine)

        val oldLines = splitLines(oldNorm)
        val newLines = splitLines(newNorm)
        val m = oldLines.size
        val n = newLines.size
        if (m == 0) return uniform(newLines, DiffType.ADD, maxLines, maxCharsPerLine)
        if (n == 0) return uniform(oldLines, DiffType.DEL, maxLines, maxCharsPerLine)

        // cell-budget 守卫：分配矩阵前判断，超限退化为整删+整增。
        if (m.toLong() * n > cellBudget) {
            val rows = ArrayList<DiffRow>()
            var truncated = false
            var i = 0
            while (i < m && !truncated) {
                if (rows.size >= maxLines) {
                    truncated = true
                    break
                }
                rows.add(DiffRow(DiffType.DEL, cap(oldLines[i], maxCharsPerLine), i + 1, null))
                i++
            }
            var j = 0
            while (j < n && !truncated) {
                if (rows.size >= maxLines) {
                    truncated = true
                    break
                }
                rows.add(DiffRow(DiffType.ADD, cap(newLines[j], maxCharsPerLine), null, j + 1))
                j++
            }
            return DiffResult(rows, truncated, addCount = n, delCount = m)
        }

        val width = n + 1
        val c = IntArray((m + 1) * width)
        for (i in 1..m) {
            val oi = oldLines[i - 1]
            val rowBase = i * width
            val prevBase = (i - 1) * width
            for (j in 1..n) {
                if (oi == newLines[j - 1]) {
                    c[rowBase + j] = c[prevBase + (j - 1)] + 1
                } else {
                    val up = c[prevBase + j]
                    val left = c[rowBase + (j - 1)]
                    c[rowBase + j] = if (up >= left) up else left
                }
            }
        }

        val ops = ArrayList<DiffRow>()
        var i = m
        var j = n
        while (i > 0 && j > 0) {
            if (oldLines[i - 1] == newLines[j - 1]) {
                ops.add(DiffRow(DiffType.CTX, oldLines[i - 1], i, j))
                i--
                j--
            } else if (c[i * width + (j - 1)] >= c[(i - 1) * width + j]) {
                ops.add(DiffRow(DiffType.ADD, newLines[j - 1], null, j))
                j--
            } else {
                ops.add(DiffRow(DiffType.DEL, oldLines[i - 1], i, null))
                i--
            }
        }
        while (i > 0) {
            ops.add(DiffRow(DiffType.DEL, oldLines[i - 1], i, null))
            i--
        }
        while (j > 0) {
            ops.add(DiffRow(DiffType.ADD, newLines[j - 1], null, j))
            j--
        }
        ops.reverse()

        var addCount = 0
        var delCount = 0
        for (op in ops) {
            if (op.type == DiffType.ADD) {
                addCount++
            } else if (op.type == DiffType.DEL) {
                delCount++
            }
        }
        val rows = ArrayList<DiffRow>()
        var truncated = false
        for (op in ops) {
            if (rows.size >= maxLines) {
                truncated = true
                break
            }
            rows.add(op.copy(text = cap(op.text, maxCharsPerLine)))
        }
        return DiffResult(rows, truncated, addCount, delCount)
    }
}
