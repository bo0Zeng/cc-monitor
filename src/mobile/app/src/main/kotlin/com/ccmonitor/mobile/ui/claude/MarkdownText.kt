package com.ccmonitor.mobile.ui.claude

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import com.ccmonitor.mobile.core.wire.gen.Doc
import com.ccmonitor.mobile.core.wire.gen.HlCode
import com.ccmonitor.mobile.core.wire.gen.SegHl
import kotlinx.coroutines.launch

/**
 * 一段的代码高亮怎么拉（聊天屏给，见 `ChatSession.segHl`）：交那一段的代码块（先序），回 `hl[n]` 对第 n 块；问不到 ⇒ `null`。
 * 没给 ⇒ 代码照纯字。
 */
val LocalSegHl = staticCompositionLocalOf<suspend (List<HlCode>) -> SegHl?> { { null } }

/**
 * 助手正文：照后端解析好的块树排（[MdDocState] · [MdDocView]），手机不解析 Markdown。
 * 先排第 0 段上屏；长正文后面那几段一帧接一段往下排，不压在头一帧上。
 * 每段先出字、后上色：建好一段就去拉它的高亮（[LocalSegHl]），拉到了那一段带色重排；拉不到照纯字。
 */
@Composable
fun MarkdownText(
    doc: Doc,
    modifier: Modifier = Modifier,
    color: Color = MaterialTheme.colorScheme.onSurface,
) {
    val state = rememberMdDocState(doc)
    val pull = LocalSegHl.current
    LaunchedEffect(state, pull) {
        for (i in 0 until state.segCount) {
            if (i >= state.built) {
                withFrameNanos { }
                state.buildNext()
            }
            val codes = codesOf(state.doc, i)
            if (codes.isNotEmpty()) launch { pull(codes)?.let { state.paint(i, it) } }
        }
    }
    MdDocView(state, modifier, color)
}
