package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.ui.chat.FakeCoreLink.Companion.human
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * 一条会话的目录（⋯「打开工作目录」去的那一处，在它里面「新建会话」也起在那里）是会话帧上的项目目录 `projectDir`，
 * 不是进程起在哪的 `cwd`（那一格只拿来认「刚起的那条起来了」）。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ChatOpenDirTest {
    private val file = FakeCoreLink.File(mutableListOf(human("u1", "你好")))

    @Test
    fun `会话帧上进程目录与项目目录不同 ⇒ 会话的目录是项目目录`() =
        runTest {
            val link = FakeCoreLink()
            link.report("s1", cwd = "/h/proj/sub", projectDir = "/h/proj")
            link.answer = { cmd, args -> if (cmd == "history-facts") Reply.LinkDown else file.answer(cmd, args) ?: Reply.LinkDown }
            val s = ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope).also { it.open() }
            runCurrent()
            assertEquals("/h/proj", s.state.value.cwd)
        }
}
