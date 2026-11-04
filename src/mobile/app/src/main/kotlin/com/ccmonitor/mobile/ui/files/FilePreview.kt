package com.ccmonitor.mobile.ui.files

import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.ProseHl
import com.ccmonitor.mobile.core.claude.link.RemotePath
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.wire.gen.HlCode
import com.ccmonitor.mobile.core.wire.gen.PreviewGate
import com.ccmonitor.mobile.core.wire.gen.TextPreview

/**
 * 文件屏的预览，照 `files-ls` 那一行的预览门（`gates.preview`，那台判的）：
 * 当文本读 ⇒ 照门的上限读；门带语言 ⇒ 整份交那台上色（`prose-hl`，同聊天里代码块那一条），拉不到 ⇒ 照纯字；
 * 当图读 ⇒ 经核心逐块读进内存（循环同下载，过门的上限就停）；不预览 ⇒ 那台那一句。手机不认扩展名、不自己上色、不另设上限。
 */
class FilePreview(
    private val feed: CoreLink,
    private val machine: suspend () -> String,
) {
    /** 预览出的那一份。 */
    sealed interface Shown

    /** 文本：[hl] 是那台上好的色（三个一组：从上一段末尾起的偏移 · 长度 · 类，UTF-16，照 `MdDoc.highlighted` 画）；没色 ⇒ 空，照纯字。 */
    data class Text(
        val text: String,
        val hl: List<Long>,
    ) : Shown

    /** 一张图的原样字节（解码在画的那一处）；[undecodable] 是解不开时屏上那一句（核心写的）。 */
    class Image(
        val bytes: ByteArray,
        val undecodable: String,
    ) : Shown

    /** 照 [gate] 预览；门不许 · 门不明（那一行不在这一屏）⇒ 那一句，不读；读不成 ⇒ 那台 / 核心那一句（[FileOps.Failed]）。 */
    suspend fun show(
        path: RemotePath,
        gate: PreviewGate?,
    ): Result<Shown> =
        when (gate) {
            is PreviewGate.Text -> text(path, gate.value)
            is PreviewGate.Image -> FileOps(feed, machine).readBytes(path, gate.value).map { Image(it.bytes, it.undecodable) }
            is PreviewGate.No -> Result.failure(IllegalStateException(gate.value))
            null -> Result.failure(IllegalStateException(copyText("rsFilewinSource.gates.unlisted")))
        }

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
