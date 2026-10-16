package com.ccmonitor.mobile.link

/** 一问的三态。 */
sealed interface Fetched<out T> {
    data object Loading : Fetched<Nothing>

    data class Got<T>(
        val value: T,
    ) : Fetched<T>

    data class Failed(
        val problem: Problem,
    ) : Fetched<Nothing>
}
