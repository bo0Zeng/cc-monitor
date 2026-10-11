package com.ccmonitor.mobile.ssh

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 远端命令生成纯函数（命令字符串是终端工效与启动器的地基，须逐字稳定）。
 *
 * `shQuote` / `appendAuthorizedKeyCommand` 住 `RemoteCommands.kt`；tmux 那两条的定值在 `TmuxGatewayGoldenTest`。
 */
class RemoteCommandsTest {
    @Test fun shQuoteWrapsPlain() = assertEquals("'main'", shQuote("main"))

    @Test fun shQuoteEscapesSingleQuote() = assertEquals("""'it'\''s'""", shQuote("it's"))

    @Test fun shQuoteHandlesSpaces() = assertEquals("'a b'", shQuote("a b"))

    // 公钥推送命令 —— 幂等 + 净化 + printf 防注入。
    @Test fun appendAuthorizedKeyIsIdempotentAndQuoted() {
        val cmd = appendAuthorizedKeyCommand("ssh-ed25519 AAAAKEY comment")
        assertTrue(cmd.contains("grep -qxF 'ssh-ed25519 AAAAKEY comment'")) // 整行精确匹配去重
        assertTrue(cmd.contains(">> ~/.ssh/authorized_keys")) // 追加
        assertTrue(cmd.contains("mkdir -p ~/.ssh")) // 首次建目录 + chmod
        assertTrue(cmd.contains("CCM_KEY_ADDED") && cmd.contains("CCM_KEY_ALREADY")) // 反馈标记
        assertTrue("写入应用 printf 而非 echo", cmd.contains("printf '%s\\n' 'ssh-ed25519 AAAAKEY comment'"))
    }

    @Test fun appendAuthorizedKeyStripsRealNewlinesToPreventSecondLineInjection() {
        val cmd = appendAuthorizedKeyCommand("ssh-ed25519 AAAAKEY evil\nrm -rf ~")
        assertFalse("净化后不应残留换行注入的第二行", cmd.contains("\nrm -rf ~"))
        assertTrue("真实换行应被折叠成单行并引用", cmd.contains("'ssh-ed25519 AAAAKEY evil rm -rf ~'"))
    }

    @Test fun appendAuthorizedKeyUsesPrintfSoLiteralBackslashNStaysOneLine() {
        // 字面 \n（反斜杠+字母 n，非真换行）——净化(\s)挡不住，靠 printf '%s' 不解释反斜杠闭合（dash/ash 的 echo 会展开）。
        val cmd = appendAuthorizedKeyCommand("""ssh-ed25519 KEY a\nEVIL""")
        assertFalse("不应用 echo 写公钥（dash 会把 \\n 展开成换行）", cmd.contains("echo 'ssh-ed25519"))
        assertTrue("字面反斜杠n 原样保留在引用公钥里，printf %s 不解释", cmd.contains("""printf '%s\n' 'ssh-ed25519 KEY a\nEVIL'"""))
    }
}
