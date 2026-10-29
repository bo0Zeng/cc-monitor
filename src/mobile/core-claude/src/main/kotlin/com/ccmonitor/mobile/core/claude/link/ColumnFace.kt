package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.ChatsColumn
import com.ccmonitor.mobile.core.wire.gen.ChatsColumnAsk

/**
 * 会话屏勾上的那几台并成一列：`libmobcore.so` 里那一个口（`core-rs/rust/lib.rs`），并法是 `resident_client::column`
 * （本机后端并历史页那一份），这里只装运。交回的是「第几台第几项」，屏照它摆各台那一行。
 */
object ColumnFace {
    init {
        System.loadLibrary("mobcore")
    }

    /** 核心读不动 ＝ 同一份构建里两头的形对不上（入出两形都是生成的）⇒ 照实抛，不画一列假的。 */
    fun of(ask: ChatsColumnAsk): ChatsColumn {
        val line = column(Wire.line(ask))
        return checkNotNull(Wire.read<ChatsColumn>(line)) { "核心交的会话屏那一列认不出：${line.take(SNIPPET)}" }
    }

    /** `ChatsColumnAsk`（JSON）⇒ `ChatsColumn`（JSON）；读不动 ⇒ 空串。 */
    external fun column(ask: String): String

    private const val SNIPPET = 200
}
