package com.ccmonitor.mobile.ui.chat.newsession

import com.ccmonitor.mobile.core.claude.link.CoreFailure
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** 浏览屏：只列文件夹（files-ls dirs_only）；点 ＝ 进去，↑ ＝ 上一级，长按 ＝ 选中它；「选这个目录」选的是选中的那个、没有就是这一层。 */
@OptIn(ExperimentalCoroutinesApi::class)
class DirBrowserTest {
    /** 那台答的一层（整份，带键集合那一形）：`up` 是它给的上一级（替身照路径切，手机不切）。 */
    private fun dirs(
        up: String?,
        vararg p: String,
    ): Map<String, Any?> {
        val keys = p.map { it.substringAfterLast('/') }
        val info = mapOf("up" to up, "crumbs" to emptyList<Any>(), "truncated" to false, "total" to p.size.toDouble(), "unreadable" to 0.0)
        return mapOf(
            "held" to "h1",
            "whole" to true,
            "same" to false,
            "order" to keys,
            "up" to keys.zip(p.map { mapOf("path" to it, "name" to it.substringAfterLast('/'), "kind" to "dir") }).toMap(),
            "head" to mapOf("total" to p.size.toDouble(), "info" to info),
        )
    }

    private fun link() =
        FakeCoreLink().also { l ->
            l.answer = { cmd, args ->
                when (cmd) {
                    "files-home" -> Reply.Ok(mapOf("path" to "/h"))
                    "files-ls" ->
                        when (args?.get("path")) {
                            "/h/work" -> Reply.Ok(dirs("/h", "/h/work/orders", "/h/work/billing"))
                            "/h/work/billing" -> Reply.Ok(dirs("/h/work"))
                            "/h" -> Reply.Ok(dirs("/", "/h/work"))
                            else -> Reply.Failed(CoreFailure("not_found", "路径不存在", "d"))
                        }
                    else -> Reply.LinkDown
                }
            }
        }

    @Test
    fun `从选中的目录起（~ 照那台的家目录展开）；进去 · 上一级 · 选这个`() =
        runTest {
            val l = link()
            val b = DirBrowser(l, "smokebox", backgroundScope)
            b.open("~/work")
            runCurrent()
            assertEquals(mapOf("path" to "/h/work", "dirs_only" to true), l.calls("files-ls").last())
            assertEquals(
                "照那台排好的先后，不另排",
                listOf("orders", "billing"),
                b.state.value.dirs
                    .map { it.name },
            )
            assertEquals("/h/work", b.picked())

            b.select("/h/work/orders")
            assertEquals("长按选中的那一个", "/h/work/orders", b.picked())
            b.select("/h/work/orders")
            assertEquals("再长按一次 ⇒ 不选、回到这一层", "/h/work", b.picked())

            b.enter("/h/work/billing")
            runCurrent()
            assertEquals("/h/work/billing", b.state.value.path)
            assertEquals("/h/work/billing", b.picked())
            b.up()
            runCurrent()
            assertEquals("/h/work", b.state.value.path)
        }

    @Test
    fun `读不出那一层 ⇒ 核心那一句，旧的不留`() =
        runTest {
            val b = DirBrowser(link(), "smokebox", backgroundScope)
            b.open("/nope")
            runCurrent()
            assertEquals("路径不存在", b.state.value.failed)
            assertEquals(emptyList<Any>(), b.state.value.dirs)
            b.open("/h")
            runCurrent()
            assertNull(b.state.value.failed)
        }
}
