package com.ccmonitor.mobile.core.claude.command

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// 会话目录取自 `PipeCommands`（同包，无需 import）。

/** 附件的落点与引用。 */
class ChatAttachmentTest {
    private val sid = "abc123-DEF_456"

    /**
     * 路径穿越必须被挡住。
     *
     * 文件名来自系统选择器，`../` 一路上翻就能写到会话目录外面，而这个路径随后要进 shell 命令。
     */
    @Test
    fun aTraversingNameCannotEscapeTheSessionDirectory() {
        for (evil in listOf("../../etc/passwd", "..\\..\\win.ini", "a/../../b", "/etc/shadow")) {
            val p = ChatAttachment.remotePath(sid, evil)
            assertFalse("「$evil」不许穿越出去：$p", p.contains(".."))
            assertTrue(
                "必须仍在这个对话的附件目录里：$p",
                p.startsWith("${PipeCommands.sessionDir(sid)}/${ChatAttachment.DIR}/"),
            )
            // 净化后不该再有多余的路径分隔符（除了目录本身那几个）
            val name = p.substringAfterLast('/')
            assertFalse("名字里不许还有分隔符：$name", name.contains('/') || name.contains('\\'))
        }
    }

    /**
     * 名字里不许留 shell 元字符或空格。
     *
     * 它要进 shell 命令（建目录 / 上传）与 `@引用`。空格在命令那边被 word-split，
     * 在 `@引用` 那边会被截成半个路径，Claude 报「找不到文件」，看不出是文件名的问题。
     */
    @Test
    fun theNameCarriesNoShellMetacharactersOrSpaces() {
        val nasty = "my report (final); rm -rf / `id` \$HOME.png"
        val name = ChatAttachment.safeName(nasty)
        for (ch in " ;|&\$`><'\"()*?[]{}\n\t") {
            assertFalse("不许留「$ch」：$name", name.contains(ch))
        }
        assertTrue("扩展名要留住（Claude 靠它判类型）：$name", name.endsWith(".png"))
    }

    /**
     * 净化后为空不许返回空串。
     *
     * 空名会拼出以 `/` 结尾的路径，上传时报目录错误，错误信息指向 SFTP 而不是文件名。
     */
    @Test
    fun aNameThatSanitizesToNothingFallsBackInsteadOfGoingEmpty() {
        for (raw in listOf("", "///", "...", "！？【】")) {
            val name = ChatAttachment.safeName(raw)
            assertTrue("「$raw」不许净化成空：得到「$name」", name.isNotEmpty())
            assertFalse("路径不许以 / 收尾", ChatAttachment.remotePath(sid, raw).endsWith("/"))
        }
    }

    /** 扩展名要保住：截断从头部截，因为扩展名在尾巴上。 */
    @Test
    fun aVeryLongNameKeepsItsExtension() {
        val long = "x".repeat(500) + ".jpeg"
        val name = ChatAttachment.safeName(long)
        assertTrue("扩展名必须留住：$name", name.endsWith(".jpeg"))
        assertTrue("要被截短", name.length <= ChatAttachment.MAX_NAME_CHARS)
    }

    /**
     * 附件目录必须由 `sessionDir` 推导，不是另拼一份。
     *
     * 两端各拼一份路径时，一端展开 `$HOME`、一端不展开，功能完全不可用且静默。
     * 判据写成关系式而不是钉一个字面串。
     */
    @Test
    fun theAttachmentDirectoryIsDerivedFromTheOneSessionDirectory() {
        val p = ChatAttachment.remotePath(sid, "a.png")
        assertEquals(
            "必须是「会话目录 / att / 前缀+名字」",
            "${PipeCommands.sessionDir(sid)}/${ChatAttachment.DIR}/${ChatAttachment.UPLOAD_PREFIX}a.png",
            p,
        )
        assertFalse("相对路径世界里不许混进 \$HOME", p.contains("\$HOME"))
    }

    /** 上行附件与下行溢出体（`tr-…`）要能分辨：同一个目录，靠前缀分。 */
    @Test
    fun uploadsAreDistinguishableFromDownlinkOverflowBodies() {
        val name = ChatAttachment.remotePath(sid, "a.png").substringAfterLast('/')
        assertTrue("上行要带自己的前缀：$name", name.startsWith(ChatAttachment.UPLOAD_PREFIX))
        assertFalse("不许与下行的 `tr-` 撞前缀", ChatAttachment.UPLOAD_PREFIX.startsWith("tr-"))
    }

    /**
     * 引用后面接着打字不能黏进路径：`@路径` 后面要留空格。
     */
    @Test
    fun theReferenceIsSeparatedFromWhateverTheUserTypesNext() {
        val ref = ChatAttachment.reference(sid, "a.png")
        assertTrue("要以 @ 开头", ref.startsWith("@"))
        assertTrue("末尾要留分隔，否则接着打字会黏进路径", ref.endsWith(" "))
        assertTrue("引用里要含真路径", ref.contains(ChatAttachment.remotePath(sid, "a.png")))
    }

    /** 建目录的命令要 quote 住路径（`sftpUpload` 不建父目录）。 */
    @Test
    fun theDirectoryIsCreatedWithAQuotedPath() {
        val cmd = ChatAttachment.mkdirCommand(sid)
        assertTrue("要建目录：$cmd", cmd.startsWith("mkdir -p "))
        assertTrue("要 quote：$cmd", cmd.contains("'"))
        assertTrue("要指向本对话的附件目录", cmd.contains("${PipeCommands.sessionDir(sid)}/${ChatAttachment.DIR}"))
    }
}
