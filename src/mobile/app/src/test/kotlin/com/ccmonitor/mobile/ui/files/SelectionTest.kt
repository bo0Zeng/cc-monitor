package com.ccmonitor.mobile.ui.files

import com.ccmonitor.mobile.core.claude.link.FilesLs
import com.ccmonitor.mobile.core.claude.link.RemotePath
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SelectionTest {
    private fun row(name: String) = FilesLs.Entry(RemotePath.of("/h/$name"), RemotePath.of(name), isDir = false, size = null, sizeText = null, mtime = null)

    private val a = row("a")
    private val b = row("b")
    private val c = row("c")

    @Test
    fun `点一下切换，去掉最后一个就退出`() {
        val s = Selection().toggle(a).toggle(b)
        assertEquals(listOf(a, b), s.of(listOf(a, b, c)))
        assertTrue(s.active)
        assertFalse(s.toggle(a).toggle(b).active)
    }

    @Test
    fun `按名字那一段的字节认：同名不同路径的两行是一项`() {
        val s = Selection().toggle(a)
        val again = FilesLs.Entry(RemotePath.of("/elsewhere/a"), RemotePath.of("a"), isDir = true, size = 1, sizeText = null, mtime = null)
        assertFalse(s.toggle(again).active)
    }

    @Test
    fun `重列：没了的去掉、数字跟着变；全没了就退出`() {
        val s = Selection().toggle(a).toggle(b).toggle(c)
        assertEquals(listOf(a, c), s.kept(listOf(c, a)).of(listOf(a, c)))
        assertEquals(2, s.kept(listOf(c, a)).size)
        assertFalse(s.kept(emptyList()).active)
    }

    @Test
    fun `全选是当前列表全部，照列表的先后`() {
        val s = Selection().toggle(c).all(listOf(a, b, c))
        assertEquals(listOf(a, b, c), s.of(listOf(a, b, c)))
        assertEquals(3, s.size)
    }
}
