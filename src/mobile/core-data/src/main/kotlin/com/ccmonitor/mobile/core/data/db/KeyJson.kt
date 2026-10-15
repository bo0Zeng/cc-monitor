package com.ccmonitor.mobile.core.data.db

import com.squareup.moshi.JsonReader
import com.squareup.moshi.JsonWriter
import okio.Buffer

/*
 * 按键那几格的小 JSON（流式读写，不走反射）。
 *
 * 触发：`{"input":[…]}` 或 `{"app":"<动作>"}`（切到某一组再带 `"group":"<组 id>"`）。
 * 一下：`{"text":"ls -la"}` · `{"key":"enter"}` · `{"key":"tab","mods":["shift"]}` · `{"char":"b","mods":["ctrl"]}`。
 * 没转过来的几处：`[{"issue":"tap_unparsed","original":"…"}]`。
 * 读不懂（缺格 · 名字不认得）⇒ 整格当 null，不猜。
 */
object KeyJson {
    fun encodeBinding(b: KeyBinding): String =
        write { w ->
            w.beginObject()
            when (b) {
                is KeyBinding.Input -> {
                    w.name("input").beginArray()
                    b.presses.forEach { writePress(w, it) }
                    w.endArray()
                }
                is KeyBinding.App -> {
                    w.name("app").value(b.action.wire)
                    b.groupId?.let { w.name("group").value(it) }
                }
            }
            w.endObject()
        }

    fun decodeBinding(s: String): KeyBinding? =
        read(s) { r ->
            var presses: List<KeyPress?>? = null
            var app: String? = null
            var group: String? = null
            r.beginObject()
            while (r.hasNext()) {
                when (r.nextName()) {
                    "input" -> presses = readArray(r) { readPress(it) }
                    "app" -> app = r.nextString()
                    "group" -> group = r.nextString()
                    else -> r.skipValue()
                }
            }
            r.endObject()
            when {
                presses != null && app == null -> presses.takeIf { p -> p.none { it == null } }?.let { KeyBinding.Input(it.filterNotNull()) }
                app != null && presses == null -> AppAction.fromWire(app)?.let { KeyBinding.App(it, group) }
                else -> null
            }
        }

    fun encodeNotes(notes: List<UnconvertedNote>): String =
        write { w ->
            w.beginArray()
            notes.forEach { n ->
                w.beginObject()
                w.name("issue").value(n.issue.wire)
                w.name("original").value(n.original)
                w.endObject()
            }
            w.endArray()
        }

    fun decodeNotes(s: String): List<UnconvertedNote>? =
        read(s) { r ->
            readArray(r) { rr ->
                var issue: String? = null
                var original = ""
                rr.beginObject()
                while (rr.hasNext()) {
                    when (rr.nextName()) {
                        "issue" -> issue = rr.nextString()
                        "original" -> original = rr.nextString()
                        else -> rr.skipValue()
                    }
                }
                rr.endObject()
                issue?.let(UnconvertedNote.Issue::fromWire)?.let { UnconvertedNote(it, original) }
            }.filterNotNull()
        }

    private fun writePress(
        w: JsonWriter,
        p: KeyPress,
    ) {
        w.beginObject()
        when (p) {
            is KeyPress.Text -> w.name("text").value(p.text)
            is KeyPress.Special -> {
                w.name("key").value(p.key.wire)
                writeMods(w, p.mods)
            }
            is KeyPress.Chord -> {
                w.name("char").value(p.char)
                writeMods(w, p.mods)
            }
        }
        w.endObject()
    }

    private fun writeMods(
        w: JsonWriter,
        mods: Set<KeyMod>,
    ) {
        if (mods.isEmpty()) return
        w.name("mods").beginArray()
        // 照枚举先后写，同一组修饰写出来的字一样。
        KeyMod.entries.filter { it in mods }.forEach { w.value(it.wire) }
        w.endArray()
    }

    /** 一下里读到的几格（还没核对能不能拼成一下）。 */
    private class PressFields(
        var text: String? = null,
        var key: String? = null,
        var char: String? = null,
        var mods: List<KeyMod?> = emptyList(),
    )

    private fun readPress(r: JsonReader): KeyPress? {
        val f = PressFields()
        r.beginObject()
        while (r.hasNext()) {
            when (r.nextName()) {
                "text" -> f.text = r.nextString()
                "key" -> f.key = r.nextString()
                "char" -> f.char = r.nextString()
                "mods" -> f.mods = readArray(r) { KeyMod.fromWire(it.nextString()) }
                else -> r.skipValue()
            }
        }
        r.endObject()
        return pressOf(f)
    }

    /** 恰好一格 text · key · char；修饰都认得；字不带修饰、组合必带修饰且只一个码点。 */
    private fun pressOf(f: PressFields): KeyPress? {
        if (listOfNotNull(f.text, f.key, f.char).size != 1 || f.mods.any { it == null }) return null
        val mods = f.mods.filterNotNull().toSet()
        val text = f.text
        val key = f.key
        val char = f.char
        return when {
            text != null -> KeyPress.Text(text).takeIf { mods.isEmpty() }
            key != null -> SpecialKey.fromWire(key)?.let { KeyPress.Special(it, mods) }
            else -> char?.takeIf { mods.isNotEmpty() && it.codePointCount(0, it.length) == 1 }?.let { KeyPress.Chord(it, mods) }
        }
    }

    private fun <T> readArray(
        r: JsonReader,
        item: (JsonReader) -> T,
    ): List<T> {
        val out = ArrayList<T>()
        r.beginArray()
        while (r.hasNext()) out += item(r)
        r.endArray()
        return out
    }

    private fun write(block: (JsonWriter) -> Unit): String {
        val buf = Buffer()
        JsonWriter.of(buf).use(block)
        return buf.readUtf8()
    }

    private fun <T> read(
        s: String,
        block: (JsonReader) -> T?,
    ): T? =
        runCatching {
            JsonReader.of(Buffer().writeUtf8(s)).use { r ->
                val v = block(r)
                if (r.peek() != JsonReader.Token.END_DOCUMENT) null else v
            }
        }.getOrNull()
}
