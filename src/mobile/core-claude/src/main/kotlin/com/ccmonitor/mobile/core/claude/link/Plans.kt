package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.gen.PlanCell
import com.ccmonitor.mobile.core.wire.gen.PlanRead
import com.ccmonitor.mobile.core.wire.gen.PlanSlice

/*
 * 计划三屏读的成品：`plan-list` · `plan-read` 的回包照生成的那条命令解（`Commands.PlanList` · `Commands.PlanRead`）。
 * 字都是那台后端写好的（`said` · `labels` · `end` · 需手动那几条连负责人那一行）；这里只按名字 · 编号找。
 */

/** 一片（照名字）；不在 ⇒ `null`。 */
fun PlanRead.slice(name: String): PlanSlice? = slices.firstOrNull { it.name == name }

/** 一格（照编号）；不在这一片 ⇒ `null`。 */
fun PlanSlice.cell(id: String): PlanCell? = cells.firstOrNull { it.id == id }

/** 「编号 标题」：后端拼好的那一份；没有 ⇒ 编号。 */
fun PlanSlice.label(id: String): String = labels[id] ?: id

/** 这几个编号里在这一片的那几格，照先后。 */
fun PlanSlice.cellsOf(ids: List<String>): List<PlanCell> = ids.mapNotNull(::cell)

/** 这一格底下有没有在这一片的子格（有 ⇒ 行尾一个箭头）。 */
fun PlanSlice.hasKids(c: PlanCell): Boolean = c.children.any { cell(it) != null }
