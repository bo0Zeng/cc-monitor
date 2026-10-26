package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.gen.BatchLine

/**
 * 一组一条命令走到哪：核心交的 `batch` 那一行就是 [BatchLine]（`cmd-wire` 的 `phone` 那一份生成的类，字全由核心写好）。
 * 收场了（结局 · 没成 · 取消）。
 */
val BatchLine.over: Boolean get() = this is BatchLine.Ended || this is BatchLine.Failed || this is BatchLine.Dropped
