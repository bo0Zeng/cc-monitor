package com.ccmonitor.mobile.ui.claude

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.diff.DiffResult
import com.ccmonitor.mobile.core.claude.diff.DiffRow
import com.ccmonitor.mobile.core.claude.diff.DiffType
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.monoSmall

/** 行级 diff 渲染：add/del 实心左边框为主信号，底色为辅。 */
@Composable
fun DiffView(
    result: DiffResult,
    modifier: Modifier = Modifier,
) {
    val tokens = LocalAppTokens.current
    Column(
        modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(tokens.radius.l))
            .background(tokens.bg2),
    ) {
        // 表头：+adds / -dels
        Row(Modifier.padding(horizontal = tokens.spacing.sm, vertical = tokens.spacing.xs)) {
            Text("+${result.addCount}", color = tokens.success, style = monoSmall)
            Spacer(Modifier.width(tokens.spacing.sm))
            Text("−${result.delCount}", color = tokens.errorText, style = monoSmall)
        }
        val rows = Modifier.fillMaxWidth().horizontalScroll(rememberScrollState())
        Column(rows) {
            result.rows.forEach { row -> DiffRowView(row, tokens.diffAddBorder, tokens.diffDelBorder, tokens.diffAddBg, tokens.diffDelBg, MaterialTheme.colorScheme.onSurfaceVariant) }
        }
        if (result.truncated) {
            Text(
                copyText("mobReading.diff.truncated"),
                style = monoSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(horizontal = tokens.spacing.sm, vertical = tokens.spacing.xs),
            )
        }
    }
}

@Composable
private fun DiffRowView(
    row: DiffRow,
    addBorder: Color,
    delBorder: Color,
    addBg: Color,
    delBg: Color,
    gutterColor: Color,
) {
    val (border, bg, sigil) =
        when (row.type) {
            DiffType.ADD -> Triple(addBorder, addBg, "+")
            DiffType.DEL -> Triple(delBorder, delBg, "−")
            DiffType.CTX -> Triple(Color.Transparent, Color.Transparent, " ")
        }
    val lineNo = row.newNo ?: row.oldNo
    Row(
        Modifier
            .fillMaxWidth()
            .height(IntrinsicSize.Min)
            .background(bg),
    ) {
        Spacer(Modifier.width(3.dp).fillMaxHeight().background(border))
        Text(
            text = (lineNo?.toString() ?: "").padStart(4),
            style = monoSmall,
            color = gutterColor,
            textAlign = TextAlign.End,
            modifier = Modifier.width(34.dp).padding(start = 4.dp, end = 6.dp),
        )
        Text("$sigil ${row.text}", style = monoSmall, maxLines = 1)
    }
}
