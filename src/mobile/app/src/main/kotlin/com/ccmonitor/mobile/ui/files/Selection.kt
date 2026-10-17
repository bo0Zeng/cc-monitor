package com.ccmonitor.mobile.ui.files

import com.ccmonitor.mobile.core.claude.link.FilesLs
import com.ccmonitor.mobile.core.claude.link.RemotePath

/** 选择模式里选中了哪几项：按那一行名字那一段的原样字节认（同一个目录里）。一项都没有 ⇒ 不在选择模式。 */
data class Selection(
    private val picked: Set<RemotePath> = emptySet(),
) {
    val active: Boolean get() = picked.isNotEmpty()

    val size: Int get() = picked.size

    operator fun contains(e: FilesLs.Entry): Boolean = e.rel in picked

    fun toggle(e: FilesLs.Entry): Selection = Selection(if (e.rel in picked) picked - e.rel else picked + e.rel)

    /** 全选：当前列表全部。 */
    fun all(entries: List<FilesLs.Entry>): Selection = Selection(entries.mapTo(LinkedHashSet()) { it.rel })

    /** 重列之后：只留还在的那几项。 */
    fun kept(entries: List<FilesLs.Entry>): Selection = Selection(picked intersect entries.mapTo(HashSet()) { it.rel })

    /** 选中的那几行（照列表的先后）。 */
    fun of(entries: List<FilesLs.Entry>): List<FilesLs.Entry> = entries.filter { it.rel in picked }
}
