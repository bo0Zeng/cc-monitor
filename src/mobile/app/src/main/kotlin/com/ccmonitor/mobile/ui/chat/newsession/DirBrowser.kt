package com.ccmonitor.mobile.ui.chat.newsession

import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.FilesHome
import com.ccmonitor.mobile.core.claude.link.FilesLs
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** 浏览屏此刻：在哪一层 · 这一层的文件夹 · 长按选中的那个 · 在读 · 读不出那一句。 */
data class BrowseUi(
    val path: String? = null,
    val dirs: List<FilesLs.Dir> = emptyList(),
    val selected: String? = null,
    val loading: Boolean = false,
    val failed: String? = null,
)

/**
 * 选目录的浏览屏（能用门槛稿 §2）：走那台的 `files-ls`（只列文件夹），和文件屏同一个后端面。
 * 点一行 ＝ 进去 · ↑ ＝ 上一级 · 长按 ＝ 选中它；「选这个目录」选的是选中的那个，没有就是这一层（[picked]）。
 */
class DirBrowser(
    private val link: CoreLink,
    private val machine: String,
    private val scope: CoroutineScope,
) {
    private val _state = MutableStateFlow(BrowseUi())
    val state: StateFlow<BrowseUi> = _state.asStateFlow()
    private var reading: Job? = null

    /** 从 [start] 起（`~` 开头的照那台的家目录展开）。 */
    fun open(start: String) {
        go { if (start == HOME || start.startsWith("$HOME/")) home()?.let { it + start.removePrefix(HOME) } else start }
    }

    fun enter(path: String) = go { path }

    fun up() {
        val p = _state.value.path ?: return
        FilesLs.parent(p)?.let { parent -> go { parent } }
    }

    /** 长按：选中它；再长按同一个 ⇒ 不选。 */
    fun select(path: String) = _state.update { it.copy(selected = if (it.selected == path) null else path) }

    fun picked(): String? = _state.value.let { it.selected ?: it.path }

    private suspend fun home(): String? {
        val r = link.call(FilesHome.COMMAND, emptyMap(), READ_MS)
        return (r as? Reply.Ok)?.let { FilesHome.of(it.data) }.also { if (it == null) fail(r, FilesHome.COMMAND) }
    }

    private fun go(where: suspend () -> String?) {
        reading?.cancel()
        _state.update { it.copy(loading = true, failed = null, selected = null) }
        reading =
            scope.launch {
                val path = where() ?: return@launch
                val r = link.call(FilesLs.COMMAND, FilesLs.dirs(path), READ_MS)
                val dirs = (r as? Reply.Ok)?.let { FilesLs.of(it.data) }
                if (dirs == null) {
                    _state.update { it.copy(path = path) }
                    fail(r, FilesLs.COMMAND)
                } else {
                    _state.value = BrowseUi(path = path, dirs = dirs.sortedBy { it.name.lowercase() })
                }
            }
    }

    private fun fail(
        r: Reply,
        command: String,
    ) {
        val why = (r.problem(machine, command) ?: unreadableReply(machine, command, r.toString())).text
        _state.update { it.copy(dirs = emptyList(), loading = false, failed = why) }
    }

    private companion object {
        const val HOME = "~"
        const val READ_MS = 15_000L
    }
}
