package com.ccmonitor.mobile.ui.rotation

import com.ccmonitor.mobile.core.wire.gen.Tone

/*
 * 轮换屏上的字：全是那台写好的（停发那一句连「还有多久」也写好了，同桌面 `rot-words.ts`）；这里只配颜色。
 */

/** 提示的颜色照那台给的语气：出错红 · 需手动与停发琥珀 · 别的只告知。 */
fun toneOf(t: Tone): NoteTone =
    when (t) {
        Tone.FAIL -> NoteTone.ERROR
        Tone.WARN, Tone.NEED -> NoteTone.WARN
        else -> NoteTone.INFO
    }
