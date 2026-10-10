package com.ccmonitor.mobile.ui.common

import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.remember
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map

/**
 * 「还不知道」是第三态，不是一个合法值。
 *
 * 一个还没读到真值的状态若被初始化成合法值（`null`、回退值），下游会拿假值先跑一趟。
 * 真值本身可以是 `null` 时（设置项「没设过」），`T?` 分不出「还没读到」与「读到了、没设过」：
 * [Unknown] 是前者，[Known]`(null)` 是后者。
 *
 * 它只保证调用方拿得到这个区分，不保证调用方用了它，也不管真值多久到。
 */
sealed interface Settled<out T> {
    /** 还没读到真值，下游什么都不做。 */
    data object Unknown : Settled<Nothing>

    /** 真值到了。[value] 可以是 `null`，那是真读数（没设过），与 [Unknown] 不同。 */
    data class Known<out T>(
        val value: T,
    ) : Settled<T>
}

/**
 * 未知不动作：[Settled.Unknown] 返回 `null`，[Settled.Known] 返回 `action(真值)`。
 *
 * @return `null` 时调用方不许继续。
 */
inline fun <T, R : Any> Settled<T>.ifKnown(action: (T) -> R?): R? =
    when (this) {
        Settled.Unknown -> null
        is Settled.Known -> action(value)
    }

/**
 * 把一条 [Flow] 采集成三值状态，初值是 [Settled.Unknown]。
 *
 * `flow` 是惰性参数：仓库的 `observe(key)` 每次调用都新造一条流，直接收会让采集每帧重起。
 * 用 [key]（通常是仓库实例）把它 `remember` 住。key 给错只会多订阅，状态不会退回 [Settled.Unknown]。
 */
@Composable
fun <T> rememberSettled(
    key: Any?,
    flow: () -> Flow<T>,
): State<Settled<T>> {
    val boxed = remember(key) { flow().map<T, Settled<T>> { Settled.Known(it) } }
    return boxed.collectAsStateWithLifecycle<Settled<T>>(Settled.Unknown)
}
