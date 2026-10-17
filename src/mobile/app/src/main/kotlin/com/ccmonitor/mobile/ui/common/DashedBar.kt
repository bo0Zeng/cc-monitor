package com.ccmonitor.mobile.ui.common

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.ui.button.SecondaryButton

/** 中性灰虚线条：那一句 ＋ 一颗按钮（离线 ⇒［重新连接］；无应答 ⇒［重试］）。 */
@Composable
fun DashedBar(
    text: String,
    action: String,
    onAction: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val line = MaterialTheme.colorScheme.outline
    Row(
        modifier
            .fillMaxWidth()
            .dashedFrame(line)
            .padding(horizontal = 14.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(text, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.weight(1f))
        SecondaryButton(onClick = onAction) { Text(action) }
    }
}

/** 中性灰虚线框（离线 · 读不出那几句同一种框）。 */
fun Modifier.dashedFrame(line: Color): Modifier =
    drawBehind {
        drawRoundRect(line, style = Stroke(width = 1.dp.toPx(), pathEffect = PathEffect.dashPathEffect(floatArrayOf(DASH_ON, DASH_OFF))), cornerRadius = CornerRadius(6.dp.toPx()))
    }

private const val DASH_ON = 8f
private const val DASH_OFF = 6f
