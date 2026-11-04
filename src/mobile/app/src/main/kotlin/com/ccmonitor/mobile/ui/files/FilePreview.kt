package com.ccmonitor.mobile.ui.files

import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.ProseHl
import com.ccmonitor.mobile.core.claude.link.RemotePath
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.wire.gen.HlCode
import com.ccmonitor.mobile.core.wire.gen.TextPreview

/**
 * 文件屏的文本预览，照 `files-ls` 那一行的预览门（`gates.preview.text`，那台判的：许读多少 · 按哪种语法上色）：
 * 照门的上限读；门带语言 ⇒ 整份交那台上色（`prose-hl`，同聊天里代码块那一条），拉不到 ⇒ 照纯字。手机不认扩展名、不自己上色。
 */
class FilePreview(
    private val feed: CoreLink,
    private val machine: suspend () -> String,
) {
    /** 预览出的那一份：[hl] 是那台上好的色（三个一组：从上一段末尾起的偏移 · 长度 · 类，UTF-16，照 `MdDoc.highlighted` 画）；没色 ⇒ 空，照纯字。 */
    data class Text(
        val text: String,
        val hl: List<Long>,
    )

    /** 读不成 ⇒ 那台那一句（[FileOps.Failed]）；上色没问成不算读不成。 */
    suspend fun text(
        path: RemotePath,
        gate: TextPreview,
    ): Result<Text> = FileOps(feed, machine).readText(path, gate.cap).map { t -> Text(t.text, gate.lang?.let { hl(it, t.text) }.orEmpty()) }

    private suspend fun hl(
        lang: String,
        x: String,
    ): List<Long>? =
        (feed.call(ProseHl.COMMAND, ProseHl.args(listOf(HlCode(lang = lang, x = x))), HL_MS) as? Reply.Ok)
            ?.let { ProseHl.of(it.data) }
            ?.hl
            ?.firstOrNull()

    private companion object {
        /** 上色等多久（同聊天里拉一段高亮）。 */
        const val HL_MS = 15_000L
    }
}
