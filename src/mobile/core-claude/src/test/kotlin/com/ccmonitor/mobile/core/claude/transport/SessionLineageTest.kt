package com.ccmonitor.mobile.core.claude.transport

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SessionLineageTest {
    private fun ids(nodes: List<SessionLineage.Node>) = nodes.map { it.sessionId }

    /** 常态：没有任何分支的对话就是平的一行，不画空的树枝。 */
    @Test
    fun withoutAnyForkEveryRowIsFlat() {
        val out = SessionLineage.arrange(listOf("a", "b", "c"), emptyMap())
        assertEquals(listOf("a", "b", "c"), ids(out))
        assertTrue("depth 全 0 —— UI 不该为「树」预留缩进槽位", out.all { it.depth == 0 })
    }

    /** 子紧随父之后，并缩进一级。 */
    @Test
    fun aForkIsPlacedRightUnderItsParent() {
        val out = SessionLineage.arrange(listOf("root", "other", "fork"), mapOf("fork" to "root"))
        assertEquals(listOf("root", "fork", "other"), ids(out))
        assertEquals(listOf(0, 1, 0), out.map { it.depth })
    }

    @Test
    fun nestedForksKeepStacking() {
        val out =
            SessionLineage.arrange(
                listOf("a", "b", "c"),
                mapOf("b" to "a", "c" to "b"),
            )
        assertEquals(listOf("a", "b", "c"), ids(out))
        assertEquals(listOf(0, 1, 2), out.map { it.depth })
    }

    /**
     * 父不在本次列表里（已归档 / 被 `/clear` 顶替）⇒ 子按根处理，
     * 不是丢掉：那条对话仍然真实存在。
     */
    @Test
    fun aChildWhoseParentIsAbsentIsShownAsARootNotDropped() {
        val out = SessionLineage.arrange(listOf("child"), mapOf("child" to "gone-parent"))
        assertEquals(listOf("child"), ids(out))
        assertEquals(0, out.single().depth)
    }

    /**
     * 环要能活着走出去。`/branch` 不该造环，但 `forkedFrom` 是文件里的数据，
     * 可能被手工改坏。成环时宁可树画得不对，不可挂死或丢条目。
     */
    @Test
    fun aCycleInTheDataDoesNotHangOrLoseRows() {
        val out = SessionLineage.arrange(listOf("x", "y"), mapOf("x" to "y", "y" to "x"))
        assertEquals("一条都不许丢", setOf("x", "y"), ids(out).toSet())
        assertEquals("也不许重复", 2, out.size)
    }

    /** 自环同理。 */
    @Test
    fun aSelfLoopIsSurvivable() {
        val out = SessionLineage.arrange(listOf("s"), mapOf("s" to "s"))
        assertEquals(listOf("s"), ids(out))
    }

    /**
     * 输入的每一条 id 都必须出现在输出里，且只出现一次：
     * 这是整个函数唯一不能破的性质（丢一条 = 用户的对话在总览里消失）。
     */
    @Test
    fun everyInputIdAppearsExactlyOnceWhateverTheShape() {
        val order = listOf("a", "b", "c", "d", "e")
        val shapes =
            listOf(
                emptyMap(),
                mapOf("b" to "a", "c" to "a", "d" to "c"),
                mapOf("a" to "b", "b" to "c", "c" to "a"), // 环
                mapOf("e" to "不存在的父"),
                mapOf("a" to "a", "b" to "c", "c" to "b"), // 自环 + 互环
            )
        for (parents in shapes) {
            val out = SessionLineage.arrange(order, parents)
            assertEquals("形状=$parents 时数量对不上：${ids(out)}", order.size, out.size)
            assertEquals("形状=$parents 时 id 集合对不上", order.toSet(), ids(out).toSet())
        }
    }

    @Test
    fun anEmptyListStaysEmpty() {
        assertTrue(SessionLineage.arrange(emptyList(), mapOf("a" to "b")).isEmpty())
    }
}
