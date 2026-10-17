package com.ccmonitor.mobile.ui

import com.ccmonitor.mobile.core.claude.link.wire.SessionFace
import com.ccmonitor.mobile.core.claude.link.wire.Wire
import com.squareup.moshi.Moshi
import java.io.File

/** 整份脸金样（`tests/__fixtures__/session-face.golden.json`，后端 `SessionFace::of` 每一形一份）：测试里的脸都从它取，不另造。 */
internal object Faces {
    /** 金样原样：形名 ⇒ 那一份（`Map` 形）。 */
    @Suppress("UNCHECKED_CAST")
    val raw: Map<String, Any?> by lazy {
        val root =
            generateSequence(File("").absoluteFile) { it.parentFile }
                .first { File(it, "tests/__fixtures__").isDirectory && File(it, "src/backend/lib.rs").isFile }
        Moshi
            .Builder()
            .build()
            .adapter(Any::class.java)
            .fromJson(File(root, "tests/__fixtures__/session-face.golden.json").readText()) as Map<String, Any?>
    }

    fun of(key: String): SessionFace = Wire.bind<SessionFace>(raw[key]) ?: error("整份脸金样里没有 $key，或装不进")
}
