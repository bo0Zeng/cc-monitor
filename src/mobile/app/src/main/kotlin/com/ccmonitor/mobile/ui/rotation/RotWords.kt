package com.ccmonitor.mobile.ui.rotation

import com.ccmonitor.mobile.core.claude.link.wire.gen.RotHint
import com.ccmonitor.mobile.core.claude.link.wire.gen.RotNote
import com.ccmonitor.mobile.core.claude.link.wire.gen.Tone
import com.ccmonitor.mobile.core.copy.DurationFormat

/*
 * 轮换屏上的字：全是那台写好的；这里只填会走的钟（停发那一句的「还有多久」，同桌面 `rot-words.ts`）。
 */

/** 提示的颜色照那台给的语气：出错红 · 需手动与停发琥珀 · 别的只告知。 */
fun toneOf(t: Tone): NoteTone =
    when (t) {
        Tone.FAIL -> NoteTone.ERROR
        Tone.WARN, Tone.NEED -> NoteTone.WARN
        else -> NoteTone.INFO
    }

/** 一条提示此刻的那一句：那台写好的；停发那一句里 `{dur}` 照回包的 `now` 与 `until` 填（会走的钟，同桌面 `clockNow`）。 */
fun hintText(
    n: RotNote,
    now: Long,
): String {
    val until = (n.hint as? RotHint.Hold)?.until ?: return n.text
    return DurationFormat.clock(n.text, now * MS, until * MS, now * MS)
}

private const val MS = 1000L
