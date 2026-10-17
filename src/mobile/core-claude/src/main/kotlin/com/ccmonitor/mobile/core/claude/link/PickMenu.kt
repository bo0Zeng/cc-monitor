package com.ccmonitor.mobile.core.claude.link

/**
 * 文件页选中这几项能做什么（核心交的那一份，判法与桌面文件窗口同一张表）：顶栏 [bar] · ⋯ [more] · 单选时点一下那一行做什么 [tap]
 * （`open` · `preview` · `download`；什么都不做 ⇒ `null`）。字全由核心写好。
 */
data class PickMenu(
    val bar: List<Entry> = emptyList(),
    val more: List<Entry> = emptyList(),
    val tap: String? = null,
) {
    /** 一项：交回的词 · 字 · 能不能点 · 灰着的为什么（点了在底部说这一句）。 */
    data class Entry(
        val id: String,
        val text: String,
        val enabled: Boolean,
        val why: String?,
    )

    companion object {
        /** 核心那一行 ⇒ 菜单；认不出 ⇒ 什么都不给。 */
        fun of(line: String): PickMenu {
            val m = Json.obj(line) ?: return PickMenu()

            fun group(k: String) =
                m.objs(k).orEmpty().mapNotNull { e ->
                    val id = e.str("id") ?: return@mapNotNull null
                    Entry(id, e.str("text").orEmpty(), e.bool("enabled") == true, e.str("why"))
                }
            return PickMenu(group("bar"), group("more"), m.str("tap"))
        }
    }
}
