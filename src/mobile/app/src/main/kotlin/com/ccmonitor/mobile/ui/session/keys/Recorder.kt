package com.ccmonitor.mobile.ui.session.keys

import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.db.KeyMod
import com.ccmonitor.mobile.core.data.db.KeyPress
import com.ccmonitor.mobile.core.data.db.SpecialKey
import com.ccmonitor.mobile.core.data.db.pressCap
import com.ccmonitor.mobile.ui.session.PressVerdict
import com.ccmonitor.mobile.ui.session.pressVerdict

/**
 * 录入抽屉：修饰键点亮后带给下一下；特殊键点一下录一颗；软键盘打的字照原样录（连着打的字并成一段）。
 * 软键盘的退格是改字：删掉最后录进去的那一下（一段字就删最后一个字）。
 *
 * 带修饰的一下录之前先判（[pressVerdict]，照真正送出去的字节比）：修饰全不起作用 ⇒ 不录、说一句；与少几个修饰的那一下送出相同 ⇒ 录进去、也说一句。
 * 那一句只管刚录的那一下，下一次按下就换掉。
 */
data class Recorder(
    val presses: List<KeyPress> = emptyList(),
    val armed: Set<KeyMod> = emptySet(),
    val note: RecNote? = null,
) {
    fun toggle(m: KeyMod): Recorder = copy(armed = if (m in armed) armed - m else armed + m, note = null)

    fun special(k: SpecialKey): Recorder = record(KeyPress.Special(k, armed))

    /** 软键盘打进来的一段（可能不止一个字）：换行录成回车；带着点亮的修饰时，第一个字录成组合。 */
    fun type(text: String): Recorder {
        var r = this
        var i = 0
        while (i < text.length) {
            val cp = text.codePointAt(i)
            val ch = String(Character.toChars(cp))
            i += ch.length
            r =
                when {
                    ch == "\n" -> r.special(SpecialKey.ENTER)
                    r.armed.isNotEmpty() -> r.record(KeyPress.Chord(ch, r.armed))
                    else -> r.appendText(ch)
                }
        }
        return r
    }

    /** 退一步 · 软键盘退格：删掉最后录进去的那一下；一段字就删最后一个字。 */
    fun undo(): Recorder {
        val last = presses.lastOrNull() ?: return copy(note = null)
        if (last is KeyPress.Text && last.text.codePointCount(0, last.text.length) > 1) {
            val cut = last.text.offsetByCodePoints(last.text.length, -1)
            return copy(presses = presses.dropLast(1) + KeyPress.Text(last.text.substring(0, cut)), note = null)
        }
        return copy(presses = presses.dropLast(1), note = null)
    }

    fun clear(): Recorder = Recorder()

    private fun appendText(ch: String): Recorder {
        val last = presses.lastOrNull()
        val next = if (last is KeyPress.Text) presses.dropLast(1) + KeyPress.Text(last.text + ch) else presses + KeyPress.Text(ch)
        return copy(presses = next, note = null)
    }

    private fun record(p: KeyPress): Recorder =
        when (val v = pressVerdict(p)) {
            PressVerdict.Sendable -> copy(presses = presses + p, armed = emptySet(), note = null)
            is PressVerdict.SameAs -> copy(presses = presses + p, armed = emptySet(), note = RecNote.Same(pressCap(p), pressCap(v.press)))
            PressVerdict.Unsendable -> copy(armed = emptySet(), note = RecNote.Cannot(pressCap(p)))
        }
}

/** 刚录的那一下要说的那一句。 */
sealed interface RecNote {
    val text: String

    data class Same(
        val a: String,
        val b: String,
    ) : RecNote {
        override val text: String get() = copyText("mobKeys.rec.same", "a" to a, "b" to b)
    }

    data class Cannot(
        val a: String,
    ) : RecNote {
        override val text: String get() = copyText("mobKeys.rec.cannot", "a" to a)
    }
}

/** 录入那排特殊键（就是内置清单）：常用的一排在前，F1–F12 收在最后一颗里。 */
val RECORDER_SPECIALS: List<SpecialKey> =
    listOf(
        SpecialKey.ESC,
        SpecialKey.TAB,
        SpecialKey.ENTER,
        SpecialKey.BACKSPACE,
        SpecialKey.DELETE,
        SpecialKey.UP,
        SpecialKey.DOWN,
        SpecialKey.LEFT,
        SpecialKey.RIGHT,
        SpecialKey.HOME,
        SpecialKey.END,
        SpecialKey.PAGE_UP,
        SpecialKey.PAGE_DOWN,
    )

val RECORDER_FKEYS: List<SpecialKey> = SpecialKey.entries.filter { it !in RECORDER_SPECIALS }
