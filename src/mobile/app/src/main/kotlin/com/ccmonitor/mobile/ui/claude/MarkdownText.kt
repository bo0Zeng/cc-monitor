package com.ccmonitor.mobile.ui.claude

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.core.wire.gen.Doc
import com.ccmonitor.mobile.core.wire.gen.HlCode
import com.ccmonitor.mobile.core.wire.gen.SegHl
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch

/**
 * 一段的代码高亮怎么拉（聊天屏给，见 `ChatSession.segHl`）：交那一段的代码块（先序），回 `hl[n]` 对第 n 块；问不到 ⇒ `null`。
 * 没给 ⇒ 代码照纯字。
 */
val LocalSegHl = staticCompositionLocalOf<suspend (List<HlCode>) -> SegHl?> { { null } }

/**
 * 助手正文：照后端解析好的块树排（[MdDocState] · [MdDocView]），手机不解析 Markdown。
 * 只排第 0 段；树分了段 ⇒ 下面一颗「展开 {size}」，点了余下几段一帧接一段补完（同桌面 `buildProseBlock`）。
 * `size` 是整块正文的大小（`Block.Prose.bytes` 经核心写成的那一格）。
 * 每段先出字、后上色：建好一段就去拉它的高亮（[LocalSegHl]），拉到了那一段带色重排；拉不到照纯字。
 */
@Composable
fun MarkdownText(
    doc: Doc,
    size: String,
    modifier: Modifier = Modifier,
    color: Color = MaterialTheme.colorScheme.onSurface,
) {
    val state = rememberMdDocState(doc)
    val pull = LocalSegHl.current
    LaunchedEffect(state, pull) { state.drive({ withFrameNanos { } }, pull) }
    Column(modifier) {
        MdDocView(state, Modifier.fillMaxWidth(), color)
        if (state.more) {
            SecondaryButton(onClick = state::expand) { Text(copyText("cards.text.showAll", "size" to size), style = monoSmall) }
        }
    }
}

/** 段怎么往下接：第 0 段拉高亮；等点了「展开」，一帧（[frame]）建一段，建好一段就拉它的高亮（[pull]）。 */
suspend fun MdDocState.drive(
    frame: suspend () -> Unit,
    pull: suspend (List<HlCode>) -> SegHl?,
) = coroutineScope {
    for (i in 0 until segCount) {
        if (i >= built) {
            awaitExpand()
            frame()
            buildNext()
        }
        val codes = codesOf(doc, i)
        if (codes.isNotEmpty()) launch { pull(codes)?.let { paint(i, it) } }
    }
}
