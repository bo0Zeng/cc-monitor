package com.ccmonitor.mobile.core.claude.command

/**
 * 附件的落点与引用，纯函数，不碰网络。
 *
 * 一律上传后在消息里写 `@路径`，不内联：官方移动端也是把文件下到机器上再以 `@` 引用交给 Claude；
 * 而上行 `content` 是字符串，内联图片要改成 content-block 数组，那个形状没验证过。
 */
object ChatAttachment {
    /**
     * 附件在会话目录下的子目录。下行溢出体（`att/tr-<id>.json`）也在这里，都是这个对话的附带文件，
     * 清理只记一处；靠前缀（[UPLOAD_PREFIX]）区分。
     */
    const val DIR = "att"

    /** 上行附件的文件名前缀，与下行溢出体（`tr-…`）区分开。 */
    const val UPLOAD_PREFIX = "up-"

    /**
     * 净化用户选来的文件名。名字要进远端路径（防 `../` 穿越）、shell 命令（防 word-split 与元字符）、
     * `@引用`（空格会把引用截断）。白名单：字母数字与 `.` `-` `_`，其余（含 `/` `\`）换成 `_`。
     *
     * @return 净化后的名字；为空时给 [FALLBACK_NAME]，空名会拼出以 `/` 结尾的路径。
     */
    fun safeName(raw: String): String {
        val cleaned =
            raw
                .takeLast(MAX_NAME_CHARS)
                .map { if (it.isLetterOrDigit() || it in ".-_") it else '_' }
                .joinToString("")
                // 前导点会造出隐藏文件；连续点是 `..` 的原料。两者都压掉。
                .trimStart('.')
                .replace(Regex("\\.{2,}"), ".")
        return cleaned.ifBlank { FALLBACK_NAME }
    }

    /**
     * 附件在远端的完整路径，相对登录 cwd。必须与会话目录用同一个推导（[PipeCommands.sessionDir]），否则两端指着不同目录。
     */
    fun remotePath(
        sessionId: String,
        rawName: String,
    ): String = "${PipeCommands.sessionDir(sessionId)}/$DIR/$UPLOAD_PREFIX${safeName(rawName)}"

    /**
     * 放进消息里的那句引用。
     *
     * `@<路径>` 是 Claude Code 认的文件引用写法（同官方客户端）。
     * 末尾留一个空格：用户接着打字时不会黏在路径上把它撑坏。
     */
    fun reference(
        sessionId: String,
        rawName: String,
    ): String = "@${remotePath(sessionId, rawName)} "

    /** 建附件目录的命令（`sftpUpload` 不建父目录）。写动词由网关出，这里只答建哪个目录。 */
    fun mkdirCommand(sessionId: String): String =
        PipeCommands.mkdirCommand("${PipeCommands.sessionDir(sessionId)}/$DIR")

    /** 文件名截多长。取尾部：扩展名在尾巴上，Claude 靠它判文件类型。 */
    const val MAX_NAME_CHARS = 80

    /** 净化后为空时的兜底名，不能是空串。 */
    const val FALLBACK_NAME = "file"
}
