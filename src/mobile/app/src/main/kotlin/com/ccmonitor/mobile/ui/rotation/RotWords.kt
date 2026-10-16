package com.ccmonitor.mobile.ui.rotation

import com.ccmonitor.mobile.core.claude.link.wire.RotHintKind
import com.ccmonitor.mobile.core.claude.link.wire.RotNote
import com.ccmonitor.mobile.core.claude.link.wire.Tone
import com.ccmonitor.mobile.core.copy.DurationFormat
import com.ccmonitor.mobile.core.copy.copyText

/*
 * 轮换屏上的字：全是那台写好的；这里只填会走的钟（停发那一句的「还有多久」，同桌面 `rot-words.ts`）与轴上的钟面。
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
    val until = n.hint.until.takeIf { n.hint.k == RotHintKind.HOLD } ?: return n.text
    return DurationFormat.clock(n.text, now * MS, until * MS, now * MS)
}

/** 一天里的分钟 ⇒ `HH:MM`（24:00 写成 24:00）。 */
fun dayClock(min: Long): String = if (min >= DAY_MIN) copyText("rot.day.end") else String.format(java.util.Locale.ROOT, "%02d:%02d", min / MIN_PER_H, min % MIN_PER_H)

private const val MS = 1000L
private const val MIN_PER_H = 60L
