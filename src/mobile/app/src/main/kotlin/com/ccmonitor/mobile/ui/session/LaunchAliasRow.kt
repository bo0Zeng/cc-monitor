package com.ccmonitor.mobile.ui.session

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.Profiles
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.core.ui.theme.monoSmall

/** 终端屏「用哪个启动」那一排：这台的别名（名字照那台给的），点一个 ⇒ 这个 shell 里敲那台写好的那一行。 */
@Composable
internal fun LaunchAliasRow(
    names: List<Profiles.Alias>,
    onPick: (Profiles.Alias) -> Unit,
) {
    if (names.isEmpty()) return
    Row(
        Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 8.dp, vertical = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        names.forEach { a ->
            AssistChip(
                onClick = { onPick(a) },
                label = { Text(a.name, style = monoSmall) },
                leadingIcon = { PhIcon(Ph.Play, contentDescription = null, size = Touch.iconInline) },
            )
        }
    }
}
