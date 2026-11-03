package com.ccmonitor.mobile.core.data.db

import com.ccmonitor.mobile.core.rs.TermKeys

/** 一下 ⇒ 交核心的记号（[TermKeys]；字节由核心那一张 xterm 键码表算）。 */
fun KeyPress.token(): String =
    when (this) {
        is KeyPress.Text -> TermKeys.text(text)
        is KeyPress.Special -> TermKeys.special(key.wire, mods.map { it.wire })
        is KeyPress.Chord -> TermKeys.chord(char, mods.map { it.wire })
    }

/** 核心交回的记号 ⇒ 一下；键名 · 修饰名不认得 ⇒ null。 */
fun keyPressOf(token: String): KeyPress? {
    val t = TermKeys.parse(token) ?: return null
    val mods = t.mods.map { KeyMod.fromWire(it) ?: return null }.toSet()
    return when (t.kind) {
        't' -> KeyPress.Text(t.value)
        's' -> KeyPress.Special(SpecialKey.fromWire(t.value) ?: return null, mods)
        else -> if (mods.isEmpty()) KeyPress.Text(t.value) else KeyPress.Chord(t.value, mods)
    }
}

/** 几下 ⇒ 送进终端的字节，一下接一下拼起来（核心算）。 */
fun keyBytes(presses: List<KeyPress>): ByteArray = checkNotNull(TermKeys.bytes(presses.map { it.token() }.toTypedArray()))
