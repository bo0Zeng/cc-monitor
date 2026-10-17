package com.ccmonitor.mobile.core.claude.link

/**
 * 一台在做的几组落盘的那一份（进程没了再开，核心照它带同一张票再问）。[put] 的 `plan` 是核心交的一串字：原样存、原样交回，这里不认里面的格。
 */
interface KeptBatches {
    /** 票 ⇒ 那一串。 */
    suspend fun all(): List<Pair<String, String>>

    suspend fun put(
        ticket: String,
        plan: String,
    )

    suspend fun drop(ticket: String)
}
