package com.ccmonitor.mobile.core.rs

import org.json.JSONArray
import org.json.JSONObject

/** 文件页选中的一项交给核心的那几格（照 `files-ls` 那一行填）。 */
data class PickFacts(
    val name: String,
    val isDir: Boolean,
    val link: Boolean,
    val linkDir: Boolean,
    val linkBroken: Boolean,
    /** 名字不是合法 UTF-8。 */
    val lossy: Boolean,
    /** 有损名带着原始字节（寻址得到）。 */
    val raw: Boolean,
    /** `files-ls` 的 `class`（线上那个词）。 */
    val fileClass: String,
)

/** 顶栏 / ⋯ 里的一项：交回的词 · 字 · 能不能点 · 灰着的为什么。 */
data class PickEntry(
    val id: String,
    val text: String,
    val enabled: Boolean,
    val why: String?,
)

/**
 * 选中这几项 ⇒ 顶栏一组 ＋ ⋯ 一组。判法与桌面文件窗口同一份（`pick-core`，经 [Core.pickMenu]），这里只换形。要那份 .so。
 */
object FilePicks {
    fun menu(picks: List<PickFacts>): List<List<PickEntry>> {
        val req =
            JSONArray(
                picks.map {
                    JSONObject()
                        .put("name", it.name)
                        .put("isDir", it.isDir)
                        .put("link", it.link)
                        .put("linkDir", it.linkDir)
                        .put("linkBroken", it.linkBroken)
                        .put("lossy", it.lossy)
                        .put("raw", it.raw)
                        .put("class", it.fileClass)
                },
            )
        val groups = JSONArray(Core.pickMenu(req.toString()))
        return (0 until groups.length()).map { g ->
            val row = groups.getJSONArray(g)
            (0 until row.length()).map { i ->
                val e = row.getJSONObject(i)
                PickEntry(
                    id = e.getString("id"),
                    text = e.getString("text"),
                    enabled = e.getBoolean("enabled"),
                    why = e.optString("why").takeUnless { e.isNull("why") },
                )
            }
        }
    }
}
