package com.ccmonitor.mobile.ui.common

import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.dialog.DangerConfirmDialog
import com.ccmonitor.mobile.core.ui.feedback.LocalAppSnackbar
import com.ccmonitor.mobile.ssh.OldHostKeys
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.koin.compose.koinInject

/**
 * 主机指纹迁移对不上那一句（核心说的，此时拨哪台都拒）：挂在根部不自己走，带一个需手动的动作；
 * 点了先确认（说清会删什么、之后怎样），确认了才交核心清（[OldHostKeys.forgetAll]）。没清成 ⇒ 那一句再挂出来。
 */
@Composable
fun UnmigratedHostKeys() {
    val old = koinInject<OldHostKeys>()
    val said by old.unmigrated.collectAsState()
    val snackbar = LocalAppSnackbar.current
    val scope = rememberCoroutineScope()
    var asking by remember { mutableStateOf(false) }
    var round by remember { mutableIntStateOf(0) }
    LaunchedEffect(said, round) {
        said?.let { snackbar.showSticky(it, copyText("mobSsh.forgetAll.action")) { asking = true } }
    }
    if (!asking) return
    val back: () -> Unit = {
        asking = false
        round++
    }
    DangerConfirmDialog(
        title = { Text(copyText("mobSsh.forgetAll.title")) },
        text = { Text(copyText("mobSsh.forgetAll.body")) },
        confirm = copyText("mobSsh.forgetAll.ok"),
        onConfirm = {
            asking = false
            scope.launch {
                if (withContext(Dispatchers.IO) { old.forgetAll() }) snackbar.show(copyText("mobSsh.forgetAll.done")) else round++
            }
        },
        onDismiss = back,
    )
}
