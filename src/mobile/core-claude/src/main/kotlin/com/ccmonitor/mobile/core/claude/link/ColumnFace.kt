package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.ChatsColumn
import com.ccmonitor.mobile.core.wire.gen.ChatsColumnAsk

/**
 * 会话屏勾上的那几台并成一列：并法是 `face_core::column`（本机后端并历史页那一份），经交进来的那一口
 * （生产上是 app 的 `CoreResident.chatsColumn`，直通嵌的核心），这里只装运。交回的是「第几台第几项」，屏照它摆各台那一行。
 */
object ColumnFace {
    /** 核心读不动 ＝ 同一份构建里两头的形对不上（入出两形都是生成的）⇒ 照实抛，不画一列假的。 */
    fun of(
        ask: ChatsColumnAsk,
        column: (String) -> String,
    ): ChatsColumn {
        val line = column(Wire.line(ask))
        return checkNotNull(Wire.read<ChatsColumn>(line)) { "核心交的会话屏那一列认不出：${line.take(SNIPPET)}" }
    }

    private const val SNIPPET = 200
}
