package com.ccmonitor.mobile.core.ui.feedback

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

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
) {
    /** 报一次结果（成功或失败）。失败必须走这里，不许静默。 */
    fun show(message: String) {
        scope.launch { hostState.showSnackbar(message, withDismissAction = true) }
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
        undoLabel: String = "撤销",
        onUndo: suspend () -> Unit,
    ) {
        scope.launch {
            action()
            val r = hostState.showSnackbar(message, undoLabel, duration = SnackbarDuration.Long)
            if (r == SnackbarResult.ActionPerformed) onUndo()
        }
    }

    /**
     * 做完一个可撤销的动作后调它：显示结果 + 一个「撤销」。
     *
     * @param onUndo 点撤销时执行。调用方自己持有恢复所需的数据（如被删掉的那条记录），
     *   这个控件不替谁记任何东西，只提供入口。
     *
     * 注意：只用于真能撤销的动作。给不可撤销的动作挂撤销按钮，就是给了一个不存在的退路。
     */
    fun showUndo(
        message: String,
        undoLabel: String = "撤销",
        onUndo: suspend () -> Unit,
    ) {
        scope.launch {
            val r =
                hostState.showSnackbar(
                    message = message,
                    actionLabel = undoLabel,
                    duration = SnackbarDuration.Long, // 撤销要给足反应时间
                )
            if (r == SnackbarResult.ActionPerformed) onUndo()
        }
    }
}

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
    val controller = remember(hostState, scope) { AppSnackbarController(hostState, scope) }

    CompositionLocalProvider(LocalAppSnackbar provides controller) {
        Box(modifier) {
            content()
            SnackbarHost(
                hostState = hostState,
                modifier = Modifier.align(Alignment.BottomCenter).navigationBarsPadding(),
            )
        }
    }
}
