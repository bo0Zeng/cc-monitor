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
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.monoSmall

/**
 * 同名冲突那一问（底部抽屉，问一次）：标题「{动作} {n} 项到 {dir}」· 那台写的那一句 · 每项「要放过去的 → 已有的」（大小 · 那台写好的时刻）；
 * 有一项不给覆盖（碰到目录）⇒ 写目录不合并。按钮照每项那台给的答（`answers`）里各项都有的那几种画，「覆盖」危险色。
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ConflictSheet(
    action: String,
    dir: RemotePath,
    said: String,
    conflicts: List<Conflict>,
    onAnswer: (String?) -> Unit,
) {
    val tokens = LocalAppTokens.current
    val answers = conflicts.map { it.answers }.reduceOrNull { a, b -> a.filter { it in b } }.orEmpty()
    val dirInto = conflicts.any { OVERWRITE !in it.answers }
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
                        copyText("rsFilewinBatch.meta.vs", "mine" to side(c.sizeText, c.mtimeText), "there" to side(c.thereSizeText, c.thereMtimeText)),
                        style = monoSmall,
                        color = tokens.textFaint,
                    )
                }
            }
            if (dirInto) Text(copyText("rsFilewinBatch.ask.dirNoMerge"), style = monoSmall, color = tokens.textFaint)
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm, Alignment.End)) {
                SecondaryButton(onClick = { onAnswer(null) }) { Text(copyText("rsFilewinBatch.btn.cancel")) }
                // 按钮字是界面自己的骨架词（同桌面冲突框那几颗）；认不得的答不画。
                answers.forEach { a ->
                    when (a) {
                        OVERWRITE -> SecondaryButton(onClick = { onAnswer(a) }, danger = true) { Text(copyText("rsFilewinBatch.btn.overwrite")) }
                        SKIP -> SecondaryButton(onClick = { onAnswer(a) }) { Text(copyText("rsFilewinBatch.btn.skip")) }
                        KEEP_BOTH -> SecondaryButton(onClick = { onAnswer(a) }) { Text(copyText("rsFilewinBatch.btn.keepBoth")) }
                    }
                }
            }
        }
    }
}

/** 一头那一项：大小 · 时刻（那台写好的字；缺的那一格不写）。 */
private fun side(
    size: String?,
    mtime: String?,
): String = listOfNotNull(size, mtime).joinToString(" · ")

private const val OVERWRITE = "overwrite"
private const val SKIP = "skip"
private const val KEEP_BOTH = "keep_both"
