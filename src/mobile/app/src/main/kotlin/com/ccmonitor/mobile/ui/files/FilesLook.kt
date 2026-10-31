package com.ccmonitor.mobile.ui.files

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.Saver
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import com.ccmonitor.mobile.core.claude.link.RemotePath
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first

/**
 * 文件页「在看什么」：哪一层 · 按不按大小排。进程被收之后照它走回去；只存路径与开关，列出来的那几行回来再问那台。
 */
data class FilesLook(
    val cwd: RemotePath? = null,
    val bySize: Boolean = false,
) {
    /** 走回这里要做的：排序要不要换（[sort]）· 进哪一层（[dir]；已经在 ⇒ `null`）。 */
    data class Back(
        val sort: Boolean,
        val dir: RemotePath?,
    )

    /** 新起的那一份此刻是 [now]（列出了那台的家）：走回这里要做的那几步。 */
    fun backFrom(now: FilesLook): Back = Back(sort = bySize != now.bySize, dir = cwd?.takeIf { it != now.cwd })

    companion object {
        /** 路径按原始字节存（不是 UTF-8 的也原样回来）。 */
        val SAVER: Saver<FilesLook, Any> =
            listSaver(
                save = { listOf(it.cwd?.bytes, it.bySize) },
                restore = { (cwd, bySize) -> FilesLook((cwd as ByteArray?)?.let(::RemotePath), bySize as Boolean) },
            )
    }
}

/**
 * 跟着 [vm] 存下「在看什么」；[vm] 是新起的（进程被收过）⇒ 列出那台的家之后先进存下的那一层（列完）、再排回那个序。
 * 进不去（那一层没了）⇒ 留在家里，那台那一句照常提示。走回去之前不改存下的那一份：半路再被收也照样回得去。
 */
@Composable
fun KeepFilesLook(vm: FilesViewModel) {
    var kept by rememberSaveable(stateSaver = FilesLook.SAVER) { mutableStateOf(FilesLook()) }
    LaunchedEffect(vm) {
        val want = kept
        if (want.cwd != null && vm.cwd.value != want.cwd) {
            vm.ui.first { it == FilesUi.Ready }
            val back = want.backFrom(FilesLook(vm.cwd.value, vm.sortBySize.value))
            back.dir?.let { dir ->
                vm.navigateTo(dir)
                vm.busy.first { !it }
            }
            if (back.sort) vm.toggleSort()
        }
        combine(vm.cwd, vm.sortBySize, ::FilesLook).collect { now -> if (now.cwd != null) kept = now }
    }
}
