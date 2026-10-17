package com.ccmonitor.mobile.ui.common

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.Busy
import com.ccmonitor.mobile.core.claude.link.wire.MachineFixButton
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.link.HostBackends
import org.koin.compose.koinInject

/** ［安装］/［更新］按钮上的字：成品给的那一颗的字；没给（主机设置里那一行）⇒［核对版本］。 */
fun deployLabel(fix: MachineFixButton?): String = fix?.label ?: copyText("mobHostEdit.backend.check")

/** 正在装 / 更新：成品那一句；放字节那一段下面走一条不定长进度条（同桌面机器页那一条）。 */
@Composable
fun BusyLine(busy: Busy) {
    Column(Modifier.fillMaxWidth()) {
        Text(busy.text)
        if (busy.bar) LinearProgressIndicator(Modifier.fillMaxWidth().padding(top = 8.dp))
    }
}

/** 一颗［安装］/［更新］按钮的那一按：[press] 按下；[held] 是按过之后持着的那条连接（没按过 ⇒ `null`）。 */
class DeployPress(
    val held: HostLink?,
    val press: () -> Unit,
)

/**
 * 按下去才连那台（这一屏在就持有那条连接：装到一半屏还在，连接不许随按钮放手），连上就叫核心装；
 * 判定 · 做法 · 话都在核心。机器设置那一行与设置屏账号卡共用这一份。
 */
@Composable
fun rememberDeployPress(hostId: String): DeployPress {
    val backends = koinInject<HostBackends>()
    var presses by remember(hostId) { mutableIntStateOf(0) }
    var held: HostLink? = null
    if (presses > 0) {
        val conn by rememberHostLink(hostId, "deploy-$hostId")
        held = conn
        LaunchedEffect(conn, presses) { if (conn == HostLink.Ready) backends.deploy(hostId) }
    }
    return DeployPress(held) { presses++ }
}
