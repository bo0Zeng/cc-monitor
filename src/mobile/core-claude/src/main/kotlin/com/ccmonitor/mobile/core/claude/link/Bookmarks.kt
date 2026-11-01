package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.Wire
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import com.ccmonitor.mobile.core.wire.gen.Bookmarks as BookmarkList

/*
 * 书签归那台后端（`bookmarks-read` · `bookmarks-edit`，本体在后端 `control/bookmarks.rs`）：手机只按那台问、照回包画，
 * 与桌面文件窗口问的同一族、同一形。切换 · 删 · 加怎么比（归一）、此刻那个目录在不在书签里都由那台判；回包是改完的整份。
 */

/** 两条命令的名字与入参。 */
object Bookmarks {
    const val READ: String = "bookmarks-read"
    const val EDIT: String = "bookmarks-edit"

    /** 那台交来的回包（核心按 `cmd-wire` 那一份解过）；认不出 ⇒ `null`。 */
    fun of(data: Any?): BookmarkList? = Wire.bind<BookmarkList>(data)

    /** 问的时候带上此刻那个目录（[at]；还没有 ⇒ 不带）：回包的 `here` 答它是第几条。 */
    internal fun at(at: String?): Map<String, Any?> = at?.let { mapOf("at" to listOf(it)) } ?: emptyMap()

    internal fun edit(
        op: String,
        dirs: List<String>,
        at: String? = null,
    ): Map<String, Any?> = mapOf("op" to op, "dirs" to dirs) + at(at)
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

    private val _here = MutableStateFlow<Map<String, Long>>(emptyMap())

    /** 此刻那个目录（[refresh] 交的）⇒ 它是第几条；不在书签里 ⇒ 不列（那台判的）。 */
    val here: StateFlow<Map<String, Long>> = _here.asStateFlow()

    private val _said = MutableStateFlow<String?>(null)
    val said: StateFlow<String?> = _said.asStateFlow()

    /** 此刻那个目录（每一问都带上）。 */
    @Volatile private var at: String? = null

    private var sent = 0L
    private var took = 0L

    /** 现问一次（进屏 · 换目录时）：[dir] 是此刻那个目录（名字不是合法 UTF-8 ⇒ `null`，不问它在不在）。 */
    suspend fun refresh(dir: String?) {
        at = dir
        ask(Bookmarks.READ, Bookmarks.at(dir))
    }

    /** 切换 [dir]（☆ / ★）。 */
    suspend fun toggle(dir: String) {
        ask(Bookmarks.EDIT, Bookmarks.edit("toggle", listOf(dir), at))
    }

    /** 删掉 [dir]（长按那一条）。 */
    suspend fun remove(dir: String) {
        ask(Bookmarks.EDIT, Bookmarks.edit("remove", listOf(dir), at))
    }

    /** 加上这几条（在的不动）：撤销删除 · 旧书签迁过来。回 `true` ＝ 那台答了 ok（迁移照它才删旧的）。 */
    suspend fun adopt(dirs: List<String>): Boolean = ask(Bookmarks.EDIT, Bookmarks.edit("add", dirs, at))

    private suspend fun ask(
        cmd: String,
        args: Map<String, Any?>,
    ): Boolean {
        val me = synchronized(this) { ++sent }
        val r = call(cmd, args)
        val got = (r as? Reply.Ok)?.let { Bookmarks.of(it.data) }
        synchronized(this) {
            if (me < took) return got != null
            took = me
            if (got != null) {
                _dirs.value = got.dirs
                _here.value = got.here.orEmpty()
                _said.value = null
            } else if (r is Reply.Failed) {
                // 那台写好的那一句；断着 · 超时不在这里说（文件屏顶上那条离线带说）。
                _said.value = r.failure.said
            }
        }
        return got != null
    }
}
