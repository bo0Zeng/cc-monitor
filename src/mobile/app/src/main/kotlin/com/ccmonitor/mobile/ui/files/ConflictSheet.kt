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
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.core.wire.gen.BatchAnswer
import com.ccmonitor.mobile.core.wire.gen.ClashRow
import com.ccmonitor.mobile.core.wire.gen.Decide

/**
 * 同名那一问（底部抽屉，问一次；上传 · 复制到 · 下载同一张）：标题 · 那一句（有的话）· 每项名字与那一行的说明（核心写好）。
 * 整组按钮只画清单里有一项给了的答（取消 · 跳过同名 · 保留两份 · 覆盖危险色；字由核心写在每行的 choices 上）；不止一项 ⇒ ［逐项选］展开，每行照它自己给的答选，
 * 没点的那几项照跳过，按 [go]（那个动作）交。
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ConflictSheet(
    title: String,
    said: String?,
    rows: List<ClashRow>,
    go: String?,
    onAnswer: (BatchAnswer?) -> Unit,
) {
    val tokens = LocalAppTokens.current
    var each by remember(rows) { mutableStateOf(false) }
    val picks = remember(rows) { mutableStateMapOf<String, Decide>() }
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
                                c.choices.forEach { o ->
                                    FilterChip(
                                        selected = (picks[c.key] ?: Decide.SKIP) == o.decide,
                                        onClick = { picks[c.key] = o.decide },
                                        label = { Text(o.pick) },
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
                    // 字与先后照核心（每行的 choices 已按跳过 · 保留两份 · 覆盖排好）；清单里没有一项给的答不画。
                    rows.flatMap { it.choices }.distinctBy { it.decide }.forEach { o ->
                        SecondaryButton(onClick = { onAnswer(BatchAnswer.All(o.decide)) }, danger = o.decide == Decide.OVERWRITE) { Text(o.label) }
                    }
                }
            }
        }
    }
}
