package com.ccmonitor.mobile.core.ui.feedback

import android.content.ClipData
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.State
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.copy.copyText
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

/**
 * 全应用的操作结果反馈 + 撤销。
 *
 * 分档：
 * | 档 | 判据 | 用什么 |
 * |---|---|---|
 * | 可撤销 | 本地、可逆 | 直接做 + [AppSnackbarController.showUndo] |
 * | 不可逆但低损 | 远端、可重建 | 直接做 + [AppSnackbarController.show]（至少让失败可见） |
 * | 不可逆且高损 | 杀正在跑的活儿、删数据 | 确认框（文案说清后果），再配 [show] 报结果 |
 *
 * 确认框只留给真正不可逆的，可逆的一律走撤销，这样确认框出现时才有分量。
 * 失败不许被吞：`runCatching { … }` 后无条件刷新，失败和成功就长得一模一样。
 */
@Immutable
class AppSnackbarController internal constructor(
    private val hostState: SnackbarHostState,
    private val scope: CoroutineScope,
    private val copy: suspend (String) -> Unit,
) {
    /** 报一次结果（成功或失败）。失败必须走这里，不许静默。 */
    fun show(message: String) {
        scope.launch { hostState.showSnackbar(message, withDismissAction = true) }
    }

    /**
     * 报一次结果，带复制详情（同桌面失败应答的「句子 · 复制详情」）：屏上只有 [Notice.text] 那一句；
     * [Notice.detail]（下层的原话、远端的输出）不上屏，点「复制详情」才把「句子 ＋ 原话」放进剪贴板。没有详情 ⇒ 同 [show]。
     */
    fun show(notice: Notice) {
        val detail = notice.detail?.takeIf { it.isNotBlank() } ?: return show(notice.text)
        scope.launch {
            val r =
                hostState.showSnackbar(
                    message = notice.text,
                    actionLabel = copyText("detail.act.copy"),
                    withDismissAction = true,
                    duration = SnackbarDuration.Long,
                )
            if (r == SnackbarResult.ActionPerformed) copy(notice.text + "\n" + detail)
        }
    }

    /**
     * 先把动作做完，再报结果并给撤销。
     *
     * 与 [showUndo] 的区别：后者要求调用方自己先做完动作；调用方若用
     * `viewModelScope.launch { … }` 投出去就立刻调 [showUndo]，会在动作完成前就宣告成功。
     * 这个方法把动作也放进控制器的 root scope 里 await：报的是真结果，动作本身也不随屏死。
     */
    fun showUndoAfter(
        action: suspend () -> Unit,
        message: String,
        undoLabel: String = copyText("kit.toast.undo"),
        onUndo: suspend () -> Unit,
    ) {
        scope.launch {
            action()
            if (undoWindow(message, undoLabel) == SnackbarResult.ActionPerformed) onUndo()
        }
    }

    /**
     * 做完一个可撤销的动作后调它：显示结果 + 一个「撤销」，撤销窗恰好 [UNDO_WINDOW_MS]，窗一过提示自己收起。
     *
     * @param onUndo 点撤销时执行。调用方自己持有恢复所需的数据（如被删掉的那条记录），
     *   这个控件不替谁记任何东西，只提供入口。
     *
     * 注意：只用于真能撤销的动作。给不可撤销的动作挂撤销按钮，就是给了一个不存在的退路。
     */
    fun showUndo(
        message: String,
        undoLabel: String = copyText("kit.toast.undo"),
        onUndo: suspend () -> Unit,
    ) {
        scope.launch { if (undoWindow(message, undoLabel) == SnackbarResult.ActionPerformed) onUndo() }
    }

    /** 撤销窗：计时走协程的时钟（M3 的 SnackbarHost 只有 Short · Long · Indefinite 三档），到点回 Dismissed。 */
    private suspend fun undoWindow(
        message: String,
        undoLabel: String,
    ): SnackbarResult =
        withTimeoutOrNull(UNDO_WINDOW_MS) { hostState.showSnackbar(message, undoLabel, duration = SnackbarDuration.Indefinite) }
            ?: SnackbarResult.Dismissed

    /** 出错、且不许自己走的那种（点了才关）：带一个可选的动作（如「编辑」）。 */
    fun showSticky(
        message: String,
        actionLabel: String? = null,
        onAction: () -> Unit = {},
    ) {
        scope.launch {
            val r = hostState.showSnackbar(message, actionLabel, withDismissAction = true, duration = SnackbarDuration.Indefinite)
            if (r == SnackbarResult.ActionPerformed) onAction()
        }
    }
}

/** 撤得回的动作（直接做 ＋ 撤销）给的撤销窗，全 App 一个数。 */
const val UNDO_WINDOW_MS: Long = 8_000L

/** 一条反馈：屏上那一句 ＋ 只进复制详情的原话（可空）。 */
@Immutable
data class Notice(
    val text: String,
    val detail: String? = null,
)

/**
 * 取当前的反馈控制器。没装宿主就抛，这是有意的。
 *
 * 注意：不能给一个「默认控制器」。接在没有任何 `SnackbarHost` 渲染的 `SnackbarHostState` 上，
 * Material3 的 `showSnackbar` 会永久挂起并持有内部 mutex，之后所有调用排队卡死；
 * defaultFactory 的结果又被缓存，等于进程级卡死。抛异常让「忘装宿主」在第一次组合时就炸。
 *
 * 用 `staticCompositionLocalOf`：值全程不变，没必要让读取参与重组追踪。
 */
val LocalAppSnackbar =
    staticCompositionLocalOf<AppSnackbarController> {
        error("AppSnackbarHostScaffold 未安装 —— 操作反馈与撤销会静默丢失")
    }

/**
 * 在应用根部包一层：提供控制器 + 渲染 host。
 *
 * 放根部而不是每屏一个：撤销要跨屏活着。删掉一条书签后立刻返回上一屏，
 * 那个「撤销」不该跟着消失。
 */
@Composable
fun AppSnackbarHostScaffold(
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    val hostState = remember { SnackbarHostState() }
    val scope = rememberCoroutineScope()
    val clipboard = LocalClipboard.current
    val controller =
        remember(hostState, scope, clipboard) {
            AppSnackbarController(hostState, scope) { text -> clipboard.setClipEntry(ClipEntry(ClipData.newPlainText("cc-monitor", text))) }
        }

    // 提示条此刻占了底下多高（没有提示 ⇒ 0）：悬浮按钮按它往上让（[clearOfNotice]），两者同处一个排版，不互相盖。
    val band = remember { mutableStateOf(0.dp) }
    val density = LocalDensity.current
    CompositionLocalProvider(LocalAppSnackbar provides controller, LocalNoticeBand provides band) {
        Box(modifier) {
            content()
            SnackbarHost(
                hostState = hostState,
                modifier =
                    Modifier
                        .align(Alignment.BottomCenter)
                        .navigationBarsPadding()
                        .onSizeChanged { band.value = with(density) { it.height.toDp() } },
            )
        }
    }
}

/** 根部那一个提示宿主此刻占的底边高度（没装宿主 ⇒ 恒 0：那一屏没有提示）。 */
private val LocalNoticeBand = compositionLocalOf<State<Dp>> { mutableStateOf(0.dp) }

/**
 * 悬浮按钮用：提示出来时按它占的高度往上让，退了再落回（动画）。提示放在根部（撤销要跨屏活着），
 * 不在各屏 Scaffold 的 snackbarHost 里，所以由这一格把两者接到同一个排版上。每个悬浮按钮都要接（判据 `FabClearsNoticeTest`）。
 */
@Composable
fun Modifier.clearOfNotice(): Modifier {
    val band by LocalNoticeBand.current
    val lift by animateDpAsState(band, label = "clearOfNotice")
    return this.padding(bottom = lift)
}
