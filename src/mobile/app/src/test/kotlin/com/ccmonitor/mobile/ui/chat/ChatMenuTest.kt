package com.ccmonitor.mobile.ui.chat

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 上屏文案共用的内部词表 [CHAT_MENU_BANNED_WORDS] 的自检。
 *
 * ⋮ 菜单本身已经没有项（换对话、换服务器、设置归抽屉，见 `DrawerTest`）；
 * 抽屉的文案过的也是这张表（`DrawerTest.theDrawerWordingFollowsTheCopyRules`）。
 */
class ChatMenuTest {
    /**
     * 「会话」「连接」两个裸词不在 [CHAT_MENU_BANNED_WORDS] 里，词表对它们的技术义还抓不抓得住。
     *
     * 不禁裸词的全部理由是一条可证伪的推论：技术义复合词都带着一个已禁的英文 token
     * （`tmux 会话` / `SSH 会话` / `daemon 连接` …）。没有这条判据，那条推论就只是注释里的一句话；
     * 哪天有人从词表里把 `tmux` 或 `ssh` 删掉，这条当场红。
     *
     * 两头断：技术义必须仍被拦，产品义必须真放行（把表塞满会让「产品义放行」那半红）。
     */
    @Test
    fun theUnbannedWordsStillGetCaughtInTheirTechnicalSenses() {
        fun banned(text: String) = CHAT_MENU_BANNED_WORDS.any { text.lowercase().contains(it.lowercase()) }

        for (technical in listOf("tmux 会话", "SSH 会话", "daemon 连接", "bridge 会话", "pipe 连接")) {
            assertTrue(
                "「$technical」是技术义，必须仍被词表拦住。不禁「会话」「连接」两个裸词的" +
                    "全部理由就是「技术义都带着一个已禁的英文 token」：这条不成立的话，裸词就是个漏洞。",
                banned(technical),
            )
        }

        for (product in listOf("结束会话", "连接失败", "这是桌面端自己用的会话")) {
            assertTrue(
                "「$product」是产品话，不许被拦。后端术语表把「会话」定为 `session` 的正名，" +
                    "我们自己的文案也在用：拦它等于逼我们违反自己的文案口径。",
                !banned(product),
            )
        }
    }

    /**
     * 词表自检：没有这一条，上面那条可以用两种方式假绿：
     * ① 把 [CHAT_MENU_BANNED_WORDS] 清空；② 比对写成大小写敏感（`Settings` 就混过去了）。
     *
     * 每个扫描器都要自检（同 `AgentArchetypeTest.theScannerReadsRealTextArgumentsAndNothingElse` 的用意）。
     */
    @Test
    fun theBannedWordCheckerActuallyCatchesThings() {
        assertTrue("词表不能被清空（清空 ⇒ 词表判据恒绿）：实得 ${CHAT_MENU_BANNED_WORDS.size} 个", CHAT_MENU_BANNED_WORDS.size >= 20)
        // 三类各钉一个代表：有人删掉一整类的话这里红。代表词要选在表里的词（黑话那一类是「管道」）；
        //    代表词被撤时本条会红，逼人来看一眼是误撤还是该换代表。
        for (w in listOf("settings", "screen", "管道")) {
            assertTrue("词表里少了代表词「$w」—— 是不是有人删掉了一整类？", w in CHAT_MENU_BANNED_WORDS)
        }
        assertTrue("词表里不许有空串（空串会在任何文案里命中 ⇒ 全红）", CHAT_MENU_BANNED_WORDS.none { it.isBlank() })

        // 扫描器真的抓得到：这三个是「把内部词塞进文案」的三种典型形
        assertEquals("路由名混进文案", listOf("settings"), bannedWordsIn("去 settings"))
        assertEquals("大小写不敏感：`Settings` 与 `settings` 是同一个内部词", listOf("settings"), bannedWordsIn("打开 Settings 屏"))
        assertEquals("黑话混进文案", listOf("管道"), bannedWordsIn("管道设置"))
        // 「会话」「连接」是产品正名，裸词不许命中。
        //    技术义仍被拦这件事由 [theUnbannedWordsStillGetCaughtInTheirTechnicalSenses] 两头断。
        assertEquals("「会话」是产品正名，裸词不许命中", emptyList<String>(), bannedWordsIn("结束会话"))
        // 真正上屏的那三个字一个都不许命中（否则扫描器是「全红」而不是「能分辨」）

        assertEquals("扫描器不许误伤正常文案", emptyList<String>(), bannedWordsIn("设置"))
        assertEquals("扫描器不许误伤正常文案", emptyList<String>(), bannedWordsIn("对话"))
        assertEquals("扫描器不许误伤正常文案", emptyList<String>(), bannedWordsIn("服务器 · devbox"))
    }

    /** [text] 里命中的内部词（小写、去重、排序）。空表 = 干净。 */
    private fun bannedWordsIn(text: String): List<String> {
        val lower = text.lowercase()
        return CHAT_MENU_BANNED_WORDS.filter { it.lowercase() in lower }.distinct().sorted()
    }
}
