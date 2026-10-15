package com.ccmonitor.mobile.ui.session.keys

import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.db.KeyMod
import com.ccmonitor.mobile.core.data.db.KeyPress
import com.ccmonitor.mobile.core.data.db.SpecialKey
import com.ccmonitor.mobile.core.data.db.pressCap
import com.ccmonitor.mobile.ui.session.pressBytes

/**
 * 录入抽屉：修饰键点亮后带给下一下；特殊键点一下录一颗；软键盘打的字照原样录（连着打的字并成一段）。
 * 软键盘的退格是改字：删掉最后录进去的那一下（一段字就删最后一个字）。
 *
 * 带修饰的一下录之前先比字节（[verdictOf]）：修饰全不起作用 ⇒ 不录、说一句；与少几个修饰的那一下送出相同 ⇒ 录进去、也说一句。
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
        when (val v = verdictOf(p)) {
            Verdict.Ok -> copy(presses = presses + p, armed = emptySet(), note = null)
            is Verdict.Same -> copy(presses = presses + p, armed = emptySet(), note = RecNote.Same(pressCap(p), pressCap(v.like)))
            Verdict.Cannot -> copy(armed = emptySet(), note = RecNote.Cannot(pressCap(p)))
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

sealed interface Verdict {
    data object Ok : Verdict

    /** 修饰一个都不起作用（送出去和不带修饰一样）。 */
    data object Cannot : Verdict

    /** 和少几个修饰的 [like] 送出相同。 */
    data class Same(
        val like: KeyPress,
    ) : Verdict
}

/** 判一下带修饰的：照真正送出去的字节比，不另记一张表（字节怎么编只在 [pressBytes] 一处）。 */
fun verdictOf(p: KeyPress): Verdict {
    val mods = modsOf(p)
    if (mods.isEmpty()) return Verdict.Ok
    val bytes = pressBytes(p)
    if (bytes.contentEquals(pressBytes(withMods(p, emptySet())))) return Verdict.Cannot
    // 先比修饰多的（Ctrl-Alt-Shift-A 先找 Ctrl-Alt-A），找到最贴近的那一个。
    val subsets = powerSet(mods).filter { it.isNotEmpty() && it != mods }.sortedByDescending { it.size }
    return subsets.firstOrNull { bytes.contentEquals(pressBytes(withMods(p, it))) }?.let { Verdict.Same(withMods(p, it)) } ?: Verdict.Ok
}

private fun modsOf(p: KeyPress): Set<KeyMod> =
    when (p) {
        is KeyPress.Text -> emptySet()
        is KeyPress.Special -> p.mods
        is KeyPress.Chord -> p.mods
    }

private fun withMods(
    p: KeyPress,
    mods: Set<KeyMod>,
): KeyPress =
    when (p) {
        is KeyPress.Text -> p
        is KeyPress.Special -> p.copy(mods = mods)
        is KeyPress.Chord -> if (mods.isEmpty()) KeyPress.Text(p.char) else p.copy(mods = mods)
    }

private fun powerSet(s: Set<KeyMod>): List<Set<KeyMod>> =
    s.fold(listOf(emptySet())) { acc, m -> acc + acc.map { it + m } }

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
