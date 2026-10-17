package com.ccmonitor.mobile.ui.common

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.Deploying
import com.ccmonitor.mobile.core.claude.link.wire.MachineFixButton
import com.ccmonitor.mobile.core.copy.copyText

/** ［安装］/［更新］按钮上的字：成品给的那一颗的字；没给（主机设置里那一行）⇒［核对版本］。 */
fun deployLabel(fix: MachineFixButton?): String = fix?.label ?: copyText("mobHostEdit.backend.check")

/** 装 / 更新这一步：核心写好的那一句；放字节那一步下面走一条不定长进度条（同桌面机器页那一条）。 */
@Composable
fun DeployStepLine(step: Deploying.Step) {
    Column(Modifier.fillMaxWidth()) {
        Text(step.text)
        if (step.bar) LinearProgressIndicator(Modifier.fillMaxWidth().padding(top = 8.dp))
    }
}
