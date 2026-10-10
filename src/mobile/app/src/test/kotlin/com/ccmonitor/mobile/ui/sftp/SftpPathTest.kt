package com.ccmonitor.mobile.ui.sftp

import org.junit.Assert.assertEquals
import org.junit.Test

/** SFTP 远端路径纯函数的各边界。远端恒 POSIX `/`。 */
class SftpPathTest {
    @Test
    fun parentOfCases() {
        assertEquals("根的上级=根", "/", parentOf("/"))
        assertEquals("一级的上级=根", "/", parentOf("/a"))
        assertEquals("/a", parentOf("/a/b"))
        assertEquals("/a/b", parentOf("/a/b/c"))
        assertEquals("尾斜杠归一后取父", "/a", parentOf("/a/b/"))
    }

    @Test
    fun bookmarkLabelCases() {
        assertEquals("根 → /", "/", bookmarkLabel("/"))
        assertEquals("a", bookmarkLabel("/a"))
        assertEquals("末段", "b", bookmarkLabel("/a/b"))
        assertEquals("尾斜杠归一后取末段", "b", bookmarkLabel("/a/b/"))
    }

    @Test
    fun joinPathCases() {
        assertEquals("根下拼避双斜杠", "/x", joinPath("/", "x"))
        assertEquals("/a/x", joinPath("/a", "x"))
        assertEquals("/a/b/x", joinPath("/a/b", "x"))
        assertEquals("父尾斜杠归一", "/a/x", joinPath("/a/", "x"))
    }

    @Test
    fun buildCrumbsCases() {
        assertEquals(listOf("/" to "/"), buildCrumbs("/"))
        assertEquals(listOf("/" to "/", "a" to "/a"), buildCrumbs("/a"))
        assertEquals(listOf("/" to "/", "a" to "/a", "b" to "/a/b"), buildCrumbs("/a/b"))
        // 非绝对路径：原样单段（防御分支）
        assertEquals(listOf("rel" to "rel"), buildCrumbs("rel"))
        // 双斜杠：空段过滤，累积路径正常
        assertEquals(listOf("/" to "/", "a" to "/a", "b" to "/a/b"), buildCrumbs("/a//b"))
    }
}
