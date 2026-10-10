package com.ccmonitor.mobile.core.claude.model

/*
 * Claude Code JSONL 里 type:"user" 记录混着大量非真实用户输入：命令回显 XML、本地命令 stdout、系统注入、
 * ESC 中断标记、/compact 续接、!bash 输入输出。这里是识别它们的唯一一处，RecordClassifier 调用。
 *
 * 真实用户输入 = 走到 [buildUserText] 兜底（非 meta、剥噪音后非空、又不是 slash/bash/compact）。
 */

/** HTML 实体单趟解码（`&lt;`→`<` 等）；单趟 + amp 同批 → 不二次解码。 */
private val ENTITY_MAP = mapOf("lt" to "<", "gt" to ">", "quot" to "\"", "#39" to "'", "amp" to "&")
private val ENTITY_RE = Regex("&(lt|gt|quot|#39|amp);")

fun unescapeEntities(s: String): String = ENTITY_RE.replace(s) { m -> ENTITY_MAP[m.groupValues[1]] ?: m.value }

// --- stripInternalNoise：剥纯噪音（非 slash/bash——那两类由专门 parser 消费） ---

private val NOISE_TASK_NOTIF = Regex("<task-notification>[\\s\\S]*?</task-notification>")
private val NOISE_SYS_REMINDER = Regex("<system-reminder>[\\s\\S]*?</system-reminder>")
private val NOISE_CMD_CAVEAT = Regex("<local-command-caveat>[\\s\\S]*?</local-command-caveat>")
private val NOISE_CMD_STDOUT = Regex("<local-command-stdout>[\\s\\S]*?</local-command-stdout>")
private val NOISE_CONTINUE = Regex("^continue from where you left off\\.?$", setOf(RegexOption.IGNORE_CASE, RegexOption.MULTILINE))
private val NOISE_NO_RESPONSE = Regex("^no response requested\\.?$", setOf(RegexOption.IGNORE_CASE, RegexOption.MULTILINE))

// ESC 中断：非全局非多行（CLI 把中断标记作为整条 user message 唯一文本；剥完其他噪音后整文本若正好是它就归零）。
private val NOISE_INTERRUPT = Regex("^\\[Request interrupted by user[^\\]]*\\]\\s*$")

/**
 * 剥内部噪音，返回 trim 过的文本。空串 = 整条都是噪音，调用方应跳过。
 * 非空时下游 slash/bash/compact/兜底都用这份剥过的文本（`/compact` 后跟的 stdout 不拖累识别）。
 */
fun stripInternalNoise(text: String): String {
    var t = NOISE_TASK_NOTIF.replace(text, "")
    t = NOISE_SYS_REMINDER.replace(t, "")
    t = NOISE_CMD_CAVEAT.replace(t, "")
    t = NOISE_CMD_STDOUT.replace(t, "")
    t = NOISE_CONTINUE.replace(t, "")
    t = NOISE_NO_RESPONSE.replace(t, "")
    t = NOISE_INTERRUPT.replace(t, "")
    return t.trim()
}

// --- /compact 续接摘要 ---

private const val COMPACT_PREFIX = "This session is being continued from a previous conversation"

/** /compact 后 Claude Code 写回的 role:user 续接摘要（正文固定以该前缀开头）→ 折叠卡。 */
fun isCompactSummary(text: String): Boolean = text.trimStart().startsWith(COMPACT_PREFIX)

// --- 斜杠命令 /xxx（三标签，顺序随 CLI 版本漂移，各自独立提取） ---

data class ParsedSlashCommand(
    val name: String,
    val args: String,
)

private val SLASH_NAME_RE = Regex("<command-name>([\\s\\S]*?)</command-name>")
private val SLASH_MSG_RE = Regex("<command-message>[\\s\\S]*?</command-message>")
private val SLASH_ARGS_RE = Regex("<command-args>([\\s\\S]*?)</command-args>")

/**
 * `<command-name>/x</command-name><command-message>…</command-message><command-args>…</command-args>`。
 * name 必需；三标签各剥一次后 leftover 必须纯空白（否则判非命令、走兜底，防误伤含标签的正文）。
 * `/clear /help /model` 等 CLI-only 命令不写 JSONL，识别不到。
 */
fun parseSlashCommand(text: String): ParsedSlashCommand? {
    val name = SLASH_NAME_RE.find(text) ?: return null
    val args = SLASH_ARGS_RE.find(text)
    val leftover =
        SLASH_ARGS_RE
            .replaceFirst(
                SLASH_MSG_RE.replaceFirst(SLASH_NAME_RE.replaceFirst(text, ""), ""),
                "",
            ).trim()
    if (leftover.isNotEmpty()) return null
    val cmdName = unescapeEntities(name.groupValues[1]).trim()
    if (cmdName.isEmpty()) return null
    return ParsedSlashCommand(cmdName, unescapeEntities(args?.groupValues?.get(1) ?: "").trim())
}

// --- !bash 模式 ---

data class ParsedBashInput(
    val command: String,
)

data class ParsedBashOutput(
    val stdout: String,
    val stderr: String,
)

/** 整段仅为一个 `<bash-input>…</bash-input>` → 命中；否则 null 回退原样。 */
fun parseBashInput(text: String): ParsedBashInput? {
    val t = text.trim()
    val open = "<bash-input>"
    val close = "</bash-input>"
    if (!t.startsWith(open) || !t.endsWith(close) || t.length <= open.length + close.length) return null
    val inner = t.substring(open.length, t.length - close.length)
    if (inner.contains(close)) return null // 畸形嵌套
    val command = unescapeEntities(inner).trim()
    if (command.isEmpty()) return null
    return ParsedBashInput(command)
}

/** 从 text 开头取一个 `<tag>…</tag>` 段（indexOf 定位闭标签，内容可含 `<`）。 */
private fun takeLeadingTag(
    text: String,
    tag: String,
): Pair<String, String>? {
    val open = "<$tag>"
    val close = "</$tag>"
    if (!text.startsWith(open)) return null
    val end = text.indexOf(close, open.length)
    if (end < 0) return null
    val content = text.substring(open.length, end)
    val rest = text.substring(end + close.length)
    return content to rest
}

/** 整段仅由 `<bash-stdout>`/`<bash-stderr>` 段组成 → 命中（段序/缺段宽容）；残余非这两类 → null 回退。 */
fun parseBashOutput(text: String): ParsedBashOutput? {
    var rest = text.trim()
    if (!rest.startsWith("<bash-stdout>") && !rest.startsWith("<bash-stderr>")) return null
    val stdout = StringBuilder()
    val stderr = StringBuilder()
    while (rest.isNotEmpty()) {
        val out = takeLeadingTag(rest, "bash-stdout")
        val err = if (out == null) takeLeadingTag(rest, "bash-stderr") else null
        val taken = out ?: err ?: return null // 残余不是这两类标签 → 整体回退
        (if (out != null) stdout else stderr).append(taken.first)
        rest = taken.second.trim()
    }
    return ParsedBashOutput(unescapeEntities(stdout.toString()), unescapeEntities(stderr.toString()))
}
