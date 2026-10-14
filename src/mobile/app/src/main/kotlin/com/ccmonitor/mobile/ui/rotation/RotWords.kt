package com.ccmonitor.mobile.ui.rotation

import com.ccmonitor.mobile.core.claude.link.wire.Fact
import com.ccmonitor.mobile.core.claude.link.wire.RotHintKind
import com.ccmonitor.mobile.core.claude.link.wire.RotNote
import com.ccmonitor.mobile.core.claude.link.wire.RotTryLaneSeg
import com.ccmonitor.mobile.core.copy.DurationFormat
import com.ccmonitor.mobile.core.copy.copyText
import kotlin.math.roundToLong

/*
 * 轮换屏上的字：全是那台写好的（提示 · 钟面 · 原因），这里只照文案表的模板把数排进去（同桌面 `rot-words.ts` · `rot-board.ts`）。
 */

/** 提示的颜色（[NoteTone]）照 `k`：出错红 · 需手动与停发琥珀 · 别处被改了蓝 · 只告知灰（同桌面那张表）。 */
fun toneOf(k: RotHintKind): NoteTone =
    when (k) {
        RotHintKind.STRATEGY_ERROR, RotHintKind.BOOK_UNREADABLE -> NoteTone.ERROR
        RotHintKind.HOLD, RotHintKind.TZ_UNKNOWN, RotHintKind.OLD_SCRIPTS -> NoteTone.WARN
        RotHintKind.NEWER -> NoteTone.INFO
        RotHintKind.WORDS_BAD -> NoteTone.PLAIN
    }

/** 要人动手的那几种（前面加「需手动」小标）。 */
fun needsHand(k: RotHintKind): Boolean = k == RotHintKind.TZ_UNKNOWN || k == RotHintKind.OLD_SCRIPTS

/** 一条提示此刻的那一句：那台写好的；停发那一句里 `{dur}` 照回包的 `now` 与 `until` 填（会走的钟，同桌面 `clockNow`）。 */
fun hintText(
    n: RotNote,
    now: Long,
): String {
    val until = n.hint.until.takeIf { n.hint.k == RotHintKind.HOLD } ?: return n.text
    return DurationFormat.clock(n.text, now * MS, until * MS, now * MS)
}

/** 号展开头一行：`5h {u5}% · 7d {u7}%`（· `↻{重置钟面}`，那台写好的）。 */
fun acctLine(f: Fact): String =
    listOfNotNull(
        copyText("rot.acct.line", "u5" to f.w5h.used.roundToLong(), "u7" to f.w7d.used.roundToLong()),
        f.w5h.resetsText?.let { copyText("rot.acct.reset", "at" to it) },
    ).joinToString(copyText("kit.text.sep"))

/** 号牌长按看的那一句：两窗用量。 */
fun cardHover(f: Fact): String = copyText("rot.card.hover", "u5" to f.w5h.used.roundToLong(), "u7" to f.w7d.used.roundToLong())

/** 时间轴每号一行里一段的字：发不出去 ⇒ 空；重置那段 `↻{fromText}`；此刻那段 `剩 {left}%`（同桌面）。 */
fun laneText(
    g: RotTryLaneSeg,
    first: Boolean,
): String =
    when {
        !g.sendable -> ""
        g.reset -> copyText("rot.sim.reset", "at" to g.fromText)
        first -> copyText("rot.sim.left", "n" to g.left.roundToLong())
        else -> ""
    }

/** 一天里的分钟 ⇒ `HH:MM`（24:00 写成 24:00）。 */
fun dayClock(min: Long): String = if (min >= DAY_MIN) copyText("rot.day.end") else String.format(java.util.Locale.ROOT, "%02d:%02d", min / MIN_PER_H, min % MIN_PER_H)

/** 24 小时条上一段的字：0 ⇒「0% · 不用」；否则 `{v}%`。 */
fun daySegText(v: Double): String = if (v == 0.0) copyText("rot.day.zero") else copyText("rot.day.v", "v" to numText(v))

private const val MS = 1000L
private const val MIN_PER_H = 60L
