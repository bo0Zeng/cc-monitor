package com.ccmonitor.mobile.ui.files

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.BatchAnswer
import com.ccmonitor.mobile.core.claude.link.BatchEvent
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.monoSmall

/**
 * 同名那一问（底部抽屉，问一次；上传 · 复制到 · 下载同一张）：标题 · 那一句（有的话）· 每项名字与那一行的说明（核心写好）。
 * 整组按钮只画清单里有一项给了的答（取消 · 跳过同名 · 保留两份 · 覆盖危险色）；不止一项 ⇒ ［逐项选］展开，每行照它自己给的答选，
 * 没点的那几项照跳过，按 [go]（那个动作）交。
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ConflictSheet(
    title: String,
    said: String?,
    rows: List<BatchEvent.Clash>,
    go: String?,
    onAnswer: (BatchAnswer?) -> Unit,
) {
    val tokens = LocalAppTokens.current
    var each by remember(rows) { mutableStateOf(false) }
    val picks = remember(rows) { mutableStateMapOf<String, String>() }
    ModalBottomSheet(onDismissRequest = { onAnswer(null) }) {
        Column(
            Modifier.fillMaxWidth().padding(horizontal = tokens.spacing.lg).padding(bottom = tokens.spacing.lg),
            verticalArrangement = Arrangement.spacedBy(tokens.spacing.sm),
        ) {
            Text(title, style = MaterialTheme.typography.titleMedium, maxLines = 2, overflow = TextOverflow.Ellipsis)
            said?.let { Text(it, style = monoSmall, color = tokens.textFaint) }
            Column(Modifier.heightIn(max = 360.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(tokens.spacing.sm)) {
                rows.forEach { c ->
                    Column {
                        Text(c.name, maxLines = 1, overflow = TextOverflow.Ellipsis, color = MaterialTheme.colorScheme.onSurface)
                        if (each) {
                            Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(tokens.spacing.xs)) {
                                c.answers.forEach { a ->
                                    FilterChip(
                                        selected = (picks[c.key] ?: SKIP) == a,
                                        onClick = { picks[c.key] = a },
                                        label = { Text(pickText(a)) },
                                    )
                                }
                            }
                        } else if (c.meta.isNotEmpty()) {
                            Text(c.meta, style = monoSmall, color = tokens.textFaint)
                        }
                    }
                }
            }
            if (rows.size > 1) {
                FilterChip(selected = each, onClick = { each = !each }, label = { Text(copyText("rsFilewinBatch.btn.each")) })
            }
            Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm, Alignment.End)) {
                SecondaryButton(onClick = { onAnswer(null) }) { Text(copyText("rsFilewinBatch.btn.cancel")) }
                if (each) {
                    SecondaryButton(onClick = { onAnswer(BatchAnswer.Each(picks.toMap())) }) { Text(go.orEmpty()) }
                } else {
                    // 按钮字是界面自己的骨架词（同桌面冲突框那几颗）；清单里没有一项给的答不画。
                    WHOLE.filter { a -> rows.any { a in it.answers } }.forEach { a ->
                        SecondaryButton(onClick = { onAnswer(BatchAnswer.All(a)) }, danger = a == OVERWRITE) { Text(wholeText(a)) }
                    }
                }
            }
        }
    }
}

private const val OVERWRITE = "overwrite"
private const val SKIP = "skip"

/** 整组那几颗的先后（跳过在前、覆盖在最后）。 */
private val WHOLE = listOf(SKIP, "keep_both", OVERWRITE)

private fun wholeText(a: String): String =
    when (a) {
        SKIP -> copyText("rsFilewinBatch.btn.skip")
        OVERWRITE -> copyText("rsFilewinBatch.btn.overwrite")
        else -> copyText("rsFilewinBatch.btn.keepBoth")
    }

private fun pickText(a: String): String =
    when (a) {
        SKIP -> copyText("rsFilewinBatch.pick.skip")
        OVERWRITE -> copyText("rsFilewinBatch.pick.overwrite")
        else -> copyText("rsFilewinBatch.pick.keepBoth")
    }
