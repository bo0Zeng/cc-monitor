package com.ccmonitor.mobile.core.ui.dialog

import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton

/**
 * 危险操作的确认框（删除 · 忘记 · 覆盖）：确认键红；一出来焦点就在「取消」那一键上，回车按的是取消、返回键收框，不误删。
 * 触屏下按钮默认拿不到焦点 ⇒ 那一键明写可聚焦。各屏的危险确认只调它（判据 `DangerConfirmDialogTest`）。
 */
@Composable
fun DangerConfirmDialog(
    title: @Composable () -> Unit,
    text: @Composable () -> Unit,
    confirm: String,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
    cancel: String = copyText("kit.dialog.cancel"),
    onCancel: () -> Unit = onDismiss,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = title,
        text = text,
        confirmButton = { SecondaryButton(onClick = onConfirm, danger = true) { Text(confirm) } },
        dismissButton = {
            val focus = remember { FocusRequester() }
            LaunchedEffect(Unit) { focus.requestFocus() }
            SecondaryButton(onClick = onCancel, modifier = Modifier.focusProperties { canFocus = true }.focusRequester(focus)) { Text(cancel) }
        },
    )
}
