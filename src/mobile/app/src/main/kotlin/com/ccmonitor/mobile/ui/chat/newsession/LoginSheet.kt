package com.ccmonitor.mobile.ui.chat.newsession

import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ccmonitor.mobile.core.claude.link.wire.PasteKind
import com.ccmonitor.mobile.core.copy.copyText

/**
 * 登录抽屉（需手动与登录稿 §2，手机那一侧，底部抽屉）：标题 · 目标行（号牌 ＋ 号名 · 机器）·［打开登录页］·［复制链接］·
 * 粘贴那一格（名字照 `paste`，定义在下面一行灰字）·［粘贴］·［提交］· 底部［取消］（作业没了是［关闭］）。
 * 打开登录页用系统浏览器，不内嵌网页（密码不经界面）。收起一律交 `login-cancel`（[LoginFlow.cancel]）。
 */
@Suppress("DEPRECATION") // LocalClipboard 是新 API 但只有 suspend 接口
@Composable
fun LoginSheet(
    flow: LoginFlow,
    color: Int?,
    machine: String,
    onClosed: () -> Unit,
) {
    val s by flow.state.collectAsStateWithLifecycle()
    LaunchedEffect(s.done) { if (s.done) onClosed() }
    val close = {
        flow.cancel()
        onClosed()
    }
    Sheet(onDismiss = close) {
        SheetHead(copyText("login.sheet.account"), null)
        Row(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            AccountBadge(s.account, color)
            Text(s.account, style = MaterialTheme.typography.bodyLarge)
            Text(machine, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        s.failed?.let { Failed(it, onRestart = flow::start) } ?: Body(flow, s)
        Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp), horizontalArrangement = Arrangement.End) {
            TextButton(onClick = close) { Text(if (s.failed != null) copyText("kit.drawer.close") else copyText("kit.dialog.cancel")) }
        }
    }
}

@Suppress("DEPRECATION")
@Composable
private fun Body(
    flow: LoginFlow,
    s: LoginUi,
) {
    val context = LocalContext.current
    val clipboard = LocalClipboardManager.current
    var pasted by remember { mutableStateOf("") }
    val started = s.started
    Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(onClick = { started?.let { open(context, it.url) } }, enabled = started != null, modifier = Modifier.heightIn(min = 48.dp)) { Text(copyText("login.open")) }
            OutlinedButton(onClick = { started?.let { clipboard.setText(AnnotatedString(it.url)) } }, enabled = started != null, modifier = Modifier.heightIn(min = 48.dp)) {
                Text(copyText("login.copyLink"))
            }
        }
        HorizontalDivider(Modifier.padding(vertical = 4.dp))
        val code = started?.paste != PasteKind.REDIRECT_URL
        Text(if (code) copyText("login.paste.code") else copyText("login.paste.redirectUrl"), style = MaterialTheme.typography.labelLarge)
        Text(if (code) copyText("login.paste.codeHelp") else copyText("login.paste.redirectUrlHelp"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        OutlinedTextField(value = pasted, onValueChange = { pasted = it }, enabled = started != null && !s.submitting, singleLine = true, modifier = Modifier.fillMaxWidth())
        s.pasteError?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error) }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = { clipboard.getText()?.text?.let { pasted = it } }, enabled = started != null && !s.submitting) { Text(copyText("login.pasteClip")) }
            Button(onClick = { flow.submit(pasted) }, enabled = started != null && pasted.isNotBlank() && !s.submitting) { Text(copyText("login.submit")) }
            if (s.pasteError != null) TextButton(onClick = flow::start) { Text(copyText("login.restart")) }
        }
    }
}

@Composable
private fun Failed(
    text: String,
    onRestart: () -> Unit,
) {
    Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(text, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.error)
        OutlinedButton(onClick = onRestart) { Text(copyText("login.restart")) }
    }
}

/** 系统浏览器开那条登录链接（不内嵌网页）。没有能开的 ⇒ 不动（［复制链接］还在）。 */
private fun open(
    context: android.content.Context,
    url: String,
) {
    try {
        context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    } catch (_: ActivityNotFoundException) {
        // 没有浏览器：照留［复制链接］。
    }
}
