package com.ccmonitor.mobile.ui.chat

import android.net.Uri
import android.provider.OpenableColumns
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import com.ccmonitor.mobile.core.claude.command.ChatAttachment
import com.ccmonitor.mobile.core.remote.RemoteExecutor
import com.ccmonitor.mobile.core.ssh.SshConnectionManager
import com.ccmonitor.mobile.core.ssh.commandExecutor
import com.ccmonitor.mobile.ssh.HostConnector
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.koin.compose.koinInject
import java.io.File

/**
 * 挑一个文件，传到那台机器上，给回一句 `@路径` 引用。
 *
 * 附件不内联进消息，而是先落到远端再以 `@` 文件引用交给 Claude，与官方移动端一致。
 * 上传形状与 `SftpViewModel.upload` 相同：URI → 临时文件 → `sftpUpload` → 删临时文件。
 *
 * @return 挂起函数：弹选择器、等用户选、传完给回引用；取消或失败给 `null`。
 *   失败已经通过 [ChatViewModel.reportStartFailure] 上屏。
 */
@Composable
fun rememberAttachmentPicker(
    hostId: String,
    sessionId: String,
    vm: ChatViewModel,
): suspend () -> String? {
    val context = LocalContext.current
    // 连接句柄经网关 [HostConnector.connection] 取；manager 只用来造命令通道。
    val manager = koinInject<SshConnectionManager>()
    val connector = koinInject<HostConnector>()
    // 选择器是异步回调，调用方要的是挂起函数，用 Deferred 接起来；每次点击换一个新的。
    val pending = remember { arrayOfNulls<CompletableDeferred<Uri?>>(1) }
    val launcher =
        rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
            // 注意：取消时 uri 是 null，也必须 complete，否则调用方永远挂着。
            pending[0]?.complete(uri)
            pending[0] = null
        }

    return remember(hostId, sessionId, vm) {
        {
            val deferred = CompletableDeferred<Uri?>()
            pending[0] = deferred
            launcher.launch("*/*")
            val uri = deferred.await()
            if (uri == null) {
                null
            } else {
                uploadAndReference(context, manager.commandExecutor(hostId), connector, hostId, sessionId, uri, vm)
            }
        }
    }
}

/** 真正干活的那段：读名字 → 落临时文件 → 建远端目录 → 上传 → 给引用。 */
private suspend fun uploadAndReference(
    context: android.content.Context,
    executor: RemoteExecutor,
    connector: HostConnector,
    hostId: String,
    sessionId: String,
    uri: Uri,
    vm: ChatViewModel,
): String? =
    withContext(Dispatchers.IO) {
        runCatching {
            val name = displayName(context, uri)
            val tmp = File.createTempFile("aterm-att-", null, context.cacheDir)
            context.contentResolver.openInputStream(uri)?.use { input ->
                tmp.outputStream().use(input::copyTo)
            } ?: error("读不出这个文件")

            // `sftpUpload` 不建父目录，先建好。
            executor.execCapture(ChatAttachment.mkdirCommand(sessionId)).orThrow("准备附件目录")
            val remote = ChatAttachment.remotePath(sessionId, name)
            val conn = connector.connection(hostId) ?: error("这台主机现在用不了")
            conn.sftpUpload(tmp, remote)
            tmp.delete()
            ChatAttachment.reference(sessionId, name)
        }.getOrElse { e ->
            // 失败必须上屏，走状态行那条唯一通道，不另开横幅。
            vm.reportStartFailure("附件没传上去：${e.message ?: e.javaClass.simpleName}")
            null
        }
    }

/** 从 URI 问文件名，问不到给兜底；净化在 `ChatAttachment.safeName` 里做。 */
private fun displayName(
    context: android.content.Context,
    uri: Uri,
): String =
    context.contentResolver
        .query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
        ?.use { c ->
            if (c.moveToFirst()) c.getString(0) else null
        }
        ?: uri.lastPathSegment
        ?: ChatAttachment.FALLBACK_NAME
