package com.ccmonitor.mobile.ui.files

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.BatchEvent
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.ui.common.CopyDetailButton

/**
 * 一组的底部浮卡（不抢焦点，列表照常可点）：标题 · 手上那一项 · 进度条 ·［停］；断开 ⇒ 虚线框写断开那一行；
 * 收场了有没成的 ⇒ 结局句 ＋［看失败的］［重试失败的］；没成（整组）⇒ 那一句 ＋ 复制详情。字全由核心写好。
 */
@Composable
fun BatchCard(
    e: BatchEvent,
    onStop: () -> Unit,
    onRetry: () -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val tokens = LocalAppTokens.current
    var showFailed by remember(e.id) { mutableStateOf(false) }
    Surface(
        modifier = modifier.fillMaxWidth().padding(12.dp),
        shape = MaterialTheme.shapes.medium,
        tonalElevation = 3.dp,
        // 断开 ⇒ 边框换成提示那一色（虚线那一态，等那台连回来同一张票再问）。
        border = if (e.phase == BatchEvent.CUT) BorderStroke(1.dp, tokens.borderStrong) else null,
    ) {
        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(e.title, style = MaterialTheme.typography.titleSmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
            when (e.phase) {
                BatchEvent.RUNNING, BatchEvent.ASKING -> {
                    e.current?.let { Text(it, color = tokens.textFaint, maxLines = 1, overflow = TextOverflow.Ellipsis) }
                    val f = e.frac
                    if (f != null) LinearProgressIndicator(progress = { f }, modifier = Modifier.fillMaxWidth()) else LinearProgressIndicator(Modifier.fillMaxWidth())
                    Row { SecondaryButton(onClick = onStop) { Text(copyText("rsFilewinProgress.action.stop")) } }
                }
                BatchEvent.CUT -> {
                    Text(e.line.orEmpty(), color = tokens.textFaint)
                    Row { SecondaryButton(onClick = onStop) { Text(copyText("rsFilewinProgress.action.stop")) } }
                }
                BatchEvent.ENDED -> {
                    Text(e.said.orEmpty())
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (e.failed.isNotEmpty()) SecondaryButton(onClick = { showFailed = true }) { Text(copyText("mobFiles.failedShow")) }
                        if (e.retry) SecondaryButton(onClick = onRetry) { Text(copyText("rsFilewinBatch.btn.retry")) }
                        SecondaryButton(onClick = onDismiss) { Text(copyText("kit.toast.close")) }
                    }
                }
                else -> {
                    Text(e.said.orEmpty())
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        CopyDetailButton(e.said.orEmpty(), e.details.orEmpty())
                        SecondaryButton(onClick = onDismiss) { Text(copyText("kit.toast.close")) }
                    }
                }
            }
        }
    }
    if (showFailed) FailedSheet(e) { showFailed = false }
}

/** 看失败的（抽屉）：标题（核心写好的「失败 ×{n}」）· 没成的那几行（核心写好）·［复制详情］。 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun FailedSheet(
    e: BatchEvent,
    onClose: () -> Unit,
) {
    ModalBottomSheet(onDismissRequest = onClose) {
        Column(Modifier.padding(16.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(e.failedTitle.orEmpty(), style = MaterialTheme.typography.titleMedium)
            e.failed.forEach { Text(it) }
            e.details?.let { CopyDetailButton(e.title, it.removePrefix(e.title + "\n")) }
        }
    }
}
