package com.ccmonitor.mobile.ui.claude

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import com.ccmonitor.mobile.core.wire.gen.Doc

/**
 * 助手正文：照后端解析好的块树排（[MdDocState] · [MdDocView]），手机不解析 Markdown。
 * 先排第 0 段上屏；长正文后面那几段一帧接一段往下排，不压在头一帧上。
 * 代码高亮按段另取（`SegHl`），手上没有 ⇒ 代码照纯字排。
 */
@Composable
fun MarkdownText(
    doc: Doc,
    modifier: Modifier = Modifier,
    color: Color = MaterialTheme.colorScheme.onSurface,
) {
    val state = rememberMdDocState(doc)
    LaunchedEffect(state) {
        while (state.built < state.segCount) {
            withFrameNanos { }
            state.buildNext(null)
        }
    }
    MdDocView(state, modifier, color)
}
