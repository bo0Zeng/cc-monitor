package com.ccmonitor.mobile.ui.claude

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontStyle
import com.ccmonitor.mobile.core.wire.gen.Doc

/** 一段原文怎么解析成块树（聊天屏给，见 `ChatSession.mdDoc`）；问不到 ⇒ `null`。没给 ⇒ 照原文的字。 */
val LocalMdDoc = staticCompositionLocalOf<suspend (String) -> Doc?> { { null } }

/** 问树那一下走到哪了。 */
private sealed interface Asked {
    data object Pending : Asked

    data class Tree(
        val doc: Doc,
    ) : Asked

    data object Failed : Asked
}

/**
 * 思考正文：展开那一刻把原文交那台解析（[LocalMdDoc]），回来照树整份排（同桌面 `buildThinkingBody`：不分段、不出「展开」，
 * 高亮照段拉）；没问成 ⇒ 照原文的字。
 */
@Composable
fun ThinkingText(
    text: String,
    modifier: Modifier = Modifier,
    color: Color = MaterialTheme.colorScheme.onSurfaceVariant,
) {
    val ask = LocalMdDoc.current
    val asked by produceState<Asked>(Asked.Pending, text, ask) { value = ask(text)?.let { Asked.Tree(it) } ?: Asked.Failed }
    when (val a = asked) {
        Asked.Pending -> Unit
        is Asked.Tree -> WholeDoc(a.doc, modifier, color)
        Asked.Failed -> Text(text, style = MaterialTheme.typography.bodyMedium.copy(fontStyle = FontStyle.Italic), color = color, modifier = modifier)
    }
}

/** 整份排：余下的段不等「展开」，一帧接一段建完。 */
@Composable
private fun WholeDoc(
    doc: Doc,
    modifier: Modifier,
    color: Color,
) {
    val state = rememberMdDocState(doc)
    val pull = LocalSegHl.current
    LaunchedEffect(state, pull) {
        state.expand()
        state.drive({ withFrameNanos { } }, pull)
    }
    MdDocView(state, modifier, color)
}
