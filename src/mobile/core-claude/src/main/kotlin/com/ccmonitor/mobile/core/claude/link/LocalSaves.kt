package com.ccmonitor.mobile.core.claude.link

import java.io.IOException

/**
 * 下到本机那一侧（一组下载的全过程在核心里：同名一问 · 逐项 · 断了接着 · 结局 · 落盘接回）：核心每项要本机那一份时经 `batch_local` 来要，
 * 这里只开 · 删。[folder] 是起那一组时交的那一棵（原样一串，核心落盘那一份里也是它，进程没了再开照它开）。
 */
interface LocalSaves {
    /** 那一棵里此刻有的名字（同名那一问照它问）；读不了 ⇒ 空。 */
    fun names(folder: String): List<String>

    /** 写一份：[over] ＝ 盖里面叫这个名的那一份（同名答了覆盖）；`null` ⇒ 新建（同名时系统另起名）。开不了 ⇒ [IOException]。 */
    fun create(
        folder: String,
        name: String,
        over: String?,
    ): LocalFile
}

/** 本机开好的那一份：[fd] 现开（从头写，所有权交给核心）· [drop] 没下成时删掉。 */
class LocalFile(
    val fd: () -> Int,
    val drop: () -> Unit,
)
