package com.ccmonitor.mobile.ui

import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.SessionFace
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

/** 测试里手拼的回包：线上必有的那几格都在（列表空 · 可空的是 null），调用方盖上要测的那几格。结构夹具，不取自真机器。 */
internal object Wires {
    /** `accounts-list` 的一行。 */
    fun accountRow(vararg set: Pair<String, Any?>): Map<String, Any?> =
        mapOf(
            "keyMasked" to null,
            "baseUrl" to null,
            "badge" to mapOf("text" to "已登录", "warn" to false, "title" to ""),
            "plate" to "",
            "quota" to null,
            "unselectable" to null,
            "login" to false,
            "newSession" to true,
            "name" to "",
            "label" to "",
            "email" to "",
            "configDir" to null,
            "configDirShown" to null,
            "isDefault" to false,
            "mode" to "subscription",
            "exists" to true,
            "loggedIn" to true,
            "authKind" to "subscription",
            "authReady" to true,
            "selectable" to true,
            "color" to 0,
        ) + set

    /** `accounts-list` 的 `meta`。 */
    fun accountsMeta(vararg set: Pair<String, Any?>): Map<String, Any?> =
        mapOf(
            "enabled" to true,
            "acctsDir" to "/h/.cc",
            "manifestPath" to "/h/.cc/accounts.json",
            "updatedAt" to null,
            "sharedStore" to null,
            "count" to 0,
            "error" to null,
            "unsupported" to null,
            "nextDefault" to null,
            "effectiveDefault" to null,
        ) + set

    /** `history-facts` 的回包。 */
    fun facts(vararg set: Pair<String, Any?>): Map<String, Any?> =
        listOf("touchedFiles", "writers", "pending", "handedBack", "retries", "bgTasks", "mcp", "children").associateWith { emptyList<Any>() } +
            listOf(
                "forkedFrom",
                "usage",
                "modelText",
                "modelPick",
                "projectDir",
                "agent",
                "lastSay",
                "needs",
                "permissionMode",
                "permissionModeText",
                "tokens",
                "cost",
                "background",
                "answerOff",
                "autoAnswer",
                "autoAnswered",
                "startedBy",
                "line",
                "quietUnread",
            ).associateWith { null } + set

    /** `prefs-read` 的 `prefs`。 */
    fun prefs(vararg set: Pair<String, Any?>): Map<String, Any?> =
        mapOf(
            "launchers" to emptyMap<String, Any>(),
            "resumeInTmux" to true,
            "models" to emptyMap<String, Any>(),
            "permissionModes" to emptyMap<String, Any>(),
            "contextLimits" to emptyMap<String, Any>(),
            "agentHomes" to emptyMap<String, Any>(),
            "pbDir" to null,
            "childTab" to "show",
            "bgTab" to "show",
        ) + set
}
