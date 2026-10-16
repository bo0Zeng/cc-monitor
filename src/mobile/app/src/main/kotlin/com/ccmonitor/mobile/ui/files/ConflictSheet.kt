package com.ccmonitor.mobile.ui.files

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import com.ccmonitor.mobile.core.claude.link.RemotePath
import com.ccmonitor.mobile.core.claude.link.wire.Conflict
import com.ccmonitor.mobile.core.copy.SizeFormat
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.monoSmall

/**
 * 同名冲突那一问（底部抽屉，问一次）：标题「{动作} {n} 项到 {dir}」· 那台写的那一句 · 每项「要放过去的 → 已有的」（大小 · 那台写好的时刻）；
 * 有目录 ⇒ 写目录不合并。按钮只给 [answers] 里那几种答（线上的字：`overwrite` · `keep_both`），「覆盖」危险色、碰到目录不给。
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ConflictSheet(
    action: String,
    dir: RemotePath,
    said: String,
    conflicts: List<Conflict>,
    answers: List<String>,
    onAnswer: (String?) -> Unit,
) {
    val tokens = LocalAppTokens.current
    val dirInto = conflicts.any { it.dirInto }
    ModalBottomSheet(onDismissRequest = { onAnswer(null) }) {
        Column(
            Modifier.fillMaxWidth().padding(horizontal = tokens.spacing.lg).padding(bottom = tokens.spacing.lg),
            verticalArrangement = Arrangement.spacedBy(tokens.spacing.sm),
        ) {
            Text(
                copyText("rsFilewinBatch.ask.title", "action" to action, "n" to conflicts.size, "dir" to dir.shown),
                style = MaterialTheme.typography.titleMedium,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            Text(said, style = monoSmall, color = tokens.textFaint)
            conflicts.forEach { c ->
                Column {
                    Text(RemotePath.of(c.name)?.shown.orEmpty(), maxLines = 1, overflow = TextOverflow.Ellipsis, color = MaterialTheme.colorScheme.onSurface)
                    Text(
                        copyText("rsFilewinBatch.meta.vs", "mine" to side(c.size, c.mtimeText), "there" to side(c.thereSize, c.thereMtimeText)),
                        style = monoSmall,
                        color = tokens.textFaint,
                    )
                }
            }
            if (dirInto) Text(copyText("rsFilewinBatch.ask.dirNoMerge"), style = monoSmall, color = tokens.textFaint)
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm, Alignment.End)) {
                SecondaryButton(onClick = { onAnswer(null) }) { Text(copyText("rsFilewinBatch.btn.cancel")) }
                answers.filter { !(dirInto && it == OVERWRITE) }.forEach { a ->
                    SecondaryButton(onClick = { onAnswer(a) }, danger = a == OVERWRITE) { Text(button(a)) }
                }
            }
        }
    }
}

/** 一头那一项：大小 · 时刻（缺的那一格不写）。 */
private fun side(
    size: Long?,
    mtime: String?,
): String = listOfNotNull(size?.let(SizeFormat::text), mtime).joinToString(" · ")

private const val OVERWRITE = "overwrite"

/** 线上那一个答 ⇒ 按钮字（同桌面冲突框那几颗）。 */
private fun button(answer: String): String =
    when (answer) {
        OVERWRITE -> copyText("rsFilewinBatch.btn.overwrite")
        "keep_both" -> copyText("rsFilewinBatch.btn.keepBoth")
        else -> answer
    }
