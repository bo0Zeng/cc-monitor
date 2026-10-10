package com.ccmonitor.mobile.core.ui.copy

/** 文案表里的一条：模板与登记的插值点。 */
internal data class CopyEntry(
    val zh: String,
    val args: List<String>,
)

/**
 * 手机取文的唯一入口，和桌面 TS `copyText` · Rust `copy_text` 读同一张 `src/shared/copy/table.json`。
 *
 * 键必须是字面量（构建时按调用点抄表、`copy-table.vitest.ts` 按调用点判两向相等）；参数是 `"名" to 值` 的对，
 * 名与表里 `args` 两向相等，否则抛（程序员错误，同 TS 读口）。核心来的句子（`message` · `…Text`）原样显示，不经这里。
 */
fun copyText(
    key: String,
    vararg args: Pair<String, Any>,
): String {
    val e = CopyEntries.of(key) ?: error("copyText: 表里没有 $key")
    val given = args.map { it.first }
    require(given.sorted() == e.args.sorted()) { "copyText($key)：表里要 ${e.args}，给了 $given" }
    return CopyRender.render(e.zh, args.associate { it.first to it.second.toString() })
}

/**
 * 插值：具名占位 `{name}` 换成值，接缝上按汉字 / ASCII 补或拿掉一个空格（与 TS `joinSeams` · Rust `copy_core::join_seams`
 * 同形，三侧各对 `tests/__fixtures__/copy-interpolation.golden.json`）。
 */
internal object CopyRender {
    private val PH = Regex("\\{([A-Za-z][A-Za-z0-9]*)\\}")

    fun render(
        zh: String,
        args: Map<String, String>,
    ): String {
        val parts = mutableListOf<String>()
        var at = 0
        for (m in PH.findAll(zh)) {
            parts += zh.substring(at, m.range.first)
            parts += args[m.groupValues[1]] ?: m.value
            at = m.range.last + 1
        }
        parts += zh.substring(at)
        return joinSeams(parts)
    }

    private fun isHan(cp: Int): Boolean = cp in 0x3400..0x4dbf || cp in 0x4e00..0x9fff || cp in 0xf900..0xfaff

    private fun isAsciiVisible(cp: Int): Boolean = cp in '!'.code..'~'.code

    private enum class Seam { SPACE, NONE }

    private fun seamWants(
        a: Int,
        b: Int,
    ): Seam? {
        val ha = isHan(a)
        val hb = isHan(b)
        val mixed = (ha && isAsciiVisible(b)) || (isAsciiVisible(a) && hb)
        return when {
            mixed -> Seam.SPACE
            ha && hb -> Seam.NONE
            else -> null
        }
    }

    private fun cps(s: String): List<Int> = s.codePoints().toArray().toList()

    private fun str(cps: List<Int>): String = buildString { cps.forEach { appendCodePoint(it) } }

    /** 值左边那段字面的尾巴：恰一个空格 ＋ 前面一个非空白字，或直接一个非空白字。 */
    private fun leftSeam(
        lit: String,
        first: Int,
    ): String {
        val l = cps(lit)
        val spaced = l.size >= 2 && l[l.size - 1] == ' '.code && l[l.size - 2] != ' '.code
        val c = (if (spaced) l[l.size - 2] else l.lastOrNull()) ?: return lit
        return when (seamWants(c, first)) {
            Seam.SPACE -> if (spaced) lit else "$lit "
            Seam.NONE -> if (spaced) str(l.dropLast(1)) else lit
            null -> lit
        }
    }

    /** 值右边那段字面的开头。 */
    private fun rightSeam(
        last: Int,
        lit: String,
    ): String {
        val r = cps(lit)
        val spaced = r.size >= 2 && r[0] == ' '.code && r[1] != ' '.code
        val c = (if (spaced) r[1] else r.firstOrNull()) ?: return lit
        return when (seamWants(last, c)) {
            Seam.SPACE -> if (spaced) lit else " $lit"
            Seam.NONE -> if (spaced) str(r.drop(1)) else lit
            null -> lit
        }
    }

    private fun joinSeams(parts: List<String>): String {
        val lit = parts.mapIndexed { i, p -> if (i % 2 == 0) p else "" }.toMutableList()
        for (i in 1 until parts.size step 2) {
            val v = cps(parts[i])
            if (v.isEmpty()) continue
            lit[i - 1] = leftSeam(lit[i - 1], v.first())
            lit[i + 1] = rightSeam(v.last(), lit[i + 1])
        }
        return parts.mapIndexed { i, p -> if (i % 2 == 0) lit[i] else p }.joinToString("")
    }
}
