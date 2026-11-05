package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.gen.LOCAL
import java.io.File

/**
 * 读本仓的跨语言金样（`tests/__fixtures__/`，后端真序列化器写的）。手机的解码读同一份，不另抄一份样本。
 * 测试的工作目录是模块目录（`src/mobile/<模块>`）。
 */
internal object Fixtures {
    val repoRoot: File =
        generateSequence(File("").absoluteFile) { it.parentFile }
            .firstOrNull { File(it, "tests/__fixtures__").isDirectory && File(it, "src/backend/lib.rs").isFile }
            ?: error("从 ${File("").absolutePath} 往上找不到仓根（tests/__fixtures__ ＋ src/backend/lib.rs）")

    fun text(name: String): String = File(repoRoot, "tests/__fixtures__/$name").readText()

    fun json(name: String): Map<String, Any?> = Json.obj(text(name)) ?: error("$name 不是 JSON 对象")

    /** `history-facts` 的回包：线上必有的那几格都在（列表空 · 可空的是 null），[set] 盖上要测的那几格。结构夹具，不取自真会话。 */
    fun facts(vararg set: Pair<String, Any?>): Map<String, Any?> =
        mapOf(
            "forkedFrom" to null,
            "touchedFiles" to emptyList<Any>(),
            "usage" to null,
            "modelText" to null,
            "modelPick" to null,
            "projectDir" to null,
            "agent" to null,
            "writers" to emptyList<Any>(),
            "pending" to emptyList<Any>(),
            "lastSay" to null,
            "needs" to null,
            "retries" to emptyList<Any>(),
            "notices" to emptyList<Any>(),
            "permissionMode" to null,
            "permissionModeText" to null,
            "tokens" to null,
            "cost" to null,
            "background" to null,
            "autoAnswer" to null,
            "autoAnswerChoices" to emptyList<Any>(),
            "askEnd" to null,
            "mcp" to emptyList<Any>(),
            "startedBy" to null,
            "children" to emptyList<Any>(),
            "line" to null,
            "quietUnread" to null,
        ) + set

    /** `history-list` 的一行：线上必有的那几格都在，[set] 盖上要测的那几格。结构夹具。 */
    fun historyRow(vararg set: Pair<String, Any?>): Map<String, Any?> =
        mapOf(
            "agent" to "claude",
            "sessionId" to "s",
            "projectPath" to "/w",
            "projectName" to "w",
            "aiTitle" to null,
            "firstUserExcerpt" to "",
            "title" to "l",
            "startedAt" to 0,
            "updatedAt" to 0,
            "jsonlPath" to "/w/s.jsonl",
            "messageCountApprox" to 0,
            "isBg" to false,
            "starred" to false,
            "customTitle" to null,
            "hidden" to null,
            "projectDir" to "/w",
            "group" to "g",
            "label" to "l",
            "untitled" to false,
            "face" to face("ended"),
            "can" to mapOf("resume" to "yes", "accounts" to false, "fork" to false, "delete" to "yes"),
            "agentTag" to null,
            "at" to 0,
            "atText" to "",
            "sectionText" to "",
            "spanText" to "",
            "origin" to LOCAL,
            "reveal" to false,
            "deleteAsk" to "",
        ) + set

    /** 整份脸金样（后端 `SessionFace::of` 每一形一份）里 [key] 那一形，原样（`Map` 形）。 */
    @Suppress("UNCHECKED_CAST")
    fun face(key: String): Map<String, Any?> = json("session-face.golden.json")[key] as? Map<String, Any?> ?: error("整份脸金样里没有 $key")
}

// 金样是 `Any` 形读进来的（[Json.obj]）：判据里往下取一格用这几样（生产代码照线上类型解，不手取）。

fun Map<String, Any?>.str(key: String): String? = this[key] as? String

fun Map<String, Any?>.num(key: String): Long? = (this[key] as? Number)?.toLong()

@Suppress("UNCHECKED_CAST")
fun Map<String, Any?>.obj(key: String): Map<String, Any?>? = this[key] as? Map<String, Any?>

@Suppress("UNCHECKED_CAST")
fun Map<String, Any?>.objs(key: String): List<Map<String, Any?>>? = (this[key] as? List<*>)?.map { it as Map<String, Any?> }

/** 金样里该有的那一格没有 ⇒ 判据当场红。 */
fun <T> T?.need(): T = this ?: error("金样里缺这一格")
