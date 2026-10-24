package com.ccmonitor.mobile.core.claude.link

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/*
 * 书签归那台后端（`bookmarks-read` · `bookmarks-edit`，本体在后端 `control/bookmarks.rs`）：手机只按那台问、照回包画，
 * 与桌面文件窗口问的同一族、同一形。切换 · 删 · 加怎么比（归一）都由那台判；回包是改完的整份。
 */

/** 两条命令的名字与入参。 */
object Bookmarks {
    const val READ: String = "bookmarks-read"
    const val EDIT: String = "bookmarks-edit"

    /** 那台交来的 `{dirs: [...]}` ⇒ 那几个目录；认不出 ⇒ `null`。 */
    fun dirs(data: Any?): List<String>? {
        val raw = (data as? Map<*, *>)?.get("dirs") as? List<*> ?: return null
        return raw.map { it as? String ?: return null }
    }

    internal fun edit(
        op: String,
        dirs: List<String>,
    ): Map<String, Any?> = mapOf("op" to op, "dirs" to dirs)
}

/**
 * 一台的书签（一屏一份）。[call] 是问那台的那一下（`CoreLink.call`）。
 * 几问同时在路上：只认最后发出的那一问的回包；问不成 ⇒ [said] 是那一句，[dirs] 不动。
 */
class BookmarkShelf(
    private val call: suspend (String, Map<String, Any?>) -> Reply,
) {
    private val _dirs = MutableStateFlow<List<String>>(emptyList())
    val dirs: StateFlow<List<String>> = _dirs.asStateFlow()

    private val _said = MutableStateFlow<String?>(null)
    val said: StateFlow<String?> = _said.asStateFlow()

    private var sent = 0L
    private var took = 0L

    /** 现问一次（进屏 · 换目录时）。 */
    suspend fun refresh() {
        ask(Bookmarks.READ, emptyMap())
    }

    /** 切换 [dir]（☆ / ★）。 */
    suspend fun toggle(dir: String) {
        ask(Bookmarks.EDIT, Bookmarks.edit("toggle", listOf(dir)))
    }

    /** 删掉 [dir]（长按那一条）。 */
    suspend fun remove(dir: String) {
        ask(Bookmarks.EDIT, Bookmarks.edit("remove", listOf(dir)))
    }

    /** 加上这几条（在的不动）：撤销删除 · 旧书签迁过来。回 `true` ＝ 那台答了 ok（迁移照它才删旧的）。 */
    suspend fun adopt(dirs: List<String>): Boolean = ask(Bookmarks.EDIT, Bookmarks.edit("add", dirs))

    /** [dir] 在不在（那台存的是归一之后的形；手机上的目录就是那台 `files-ls` 交来的路径，同一形，照字比）。 */
    fun has(dir: String): Boolean = dir in _dirs.value

    private suspend fun ask(
        cmd: String,
        args: Map<String, Any?>,
    ): Boolean {
        val me = synchronized(this) { ++sent }
        val r = call(cmd, args)
        val got = (r as? Reply.Ok)?.let { Bookmarks.dirs(it.data) }
        synchronized(this) {
            if (me < took) return got != null
            took = me
            if (got != null) {
                _dirs.value = got
                _said.value = null
            } else if (r is Reply.Failed) {
                // 那台写好的那一句；断着 · 超时不在这里说（文件屏顶上那条离线带说）。
                _said.value = r.failure.message
            }
        }
        return got != null
    }
}
