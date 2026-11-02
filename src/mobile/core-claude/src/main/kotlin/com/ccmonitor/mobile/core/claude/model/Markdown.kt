package com.ccmonitor.mobile.core.claude.model

import com.ccmonitor.mobile.core.wire.gen.Doc
import com.ccmonitor.mobile.core.wire.gen.Inl
import com.ccmonitor.mobile.core.wire.gen.InlNode
import com.ccmonitor.mobile.core.wire.gen.MdBlock

// 助手正文的块树（后端解析好的 `Doc`）上取字：阅读面「复制代码块」与会话内搜索都从树上取，手机不看 Markdown 原文。

/**
 * 树里的代码块原文（`Code.x`），照先序（列表 · 引用 · 提示块 · 脚注里的也算）；超长代码切开的后几块（`cont`）接回前一块，
 * 复制拿到的是整块。
 */
fun Doc.codeBlocks(): List<String> {
    val out = ArrayList<String>()

    fun walk(bs: List<MdBlock>) {
        for (b in bs) {
            when (b) {
                is MdBlock.Code -> if (b.cont && out.isNotEmpty()) out[out.lastIndex] = out.last() + "\n" + b.x else out += b.x
                is MdBlock.List -> b.items.forEach { walk(it.blocks) }
                is MdBlock.Quote -> walk(b.blocks)
                is MdBlock.Alert -> walk(b.blocks)
                is MdBlock.Notes -> b.items.forEach { walk(it.blocks) }
                else -> Unit
            }
        }
    }
    walk(blocks)
    return out
}

/** 树上的字（搜索用）：块与块之间、表格的行之间换行，同一行的格之间空一格；行内照上屏的字（链接取字、图取替代字、公式取 TeX）。 */
fun Doc.plainText(): String = StringBuilder().also { it.blocks(blocks) }.trimEnd('\n').toString()

private fun StringBuilder.line() {
    if (isNotEmpty() && last() != '\n') append('\n')
}

private fun StringBuilder.blocks(bs: List<MdBlock>) {
    for (b in bs) {
        line()
        when (b) {
            is MdBlock.P -> inlines(b.k)
            is MdBlock.H -> inlines(b.k)
            is MdBlock.Code -> append(b.x)
            is MdBlock.Math -> append(b.tex)
            is MdBlock.List -> b.items.forEach { blocks(it.blocks) }
            is MdBlock.Quote -> blocks(b.blocks)
            is MdBlock.Alert -> blocks(b.blocks)
            is MdBlock.Table ->
                (listOf(b.head) + b.rows).forEach { row ->
                    line()
                    row.forEachIndexed { i, cell ->
                        if (i > 0) append(' ')
                        inlines(cell)
                    }
                }
            MdBlock.Rule -> Unit
            is MdBlock.Notes -> b.items.forEach { blocks(it.blocks) }
        }
    }
}

private fun StringBuilder.inlines(ks: List<Inl>) {
    for (k in ks) {
        when (k) {
            is Inl.Text -> append(k.value)
            is Inl.InlNode -> node(k.value)
        }
    }
}

private fun StringBuilder.node(n: InlNode) {
    when (n) {
        is InlNode.B -> inlines(n.k)
        is InlNode.I -> inlines(n.k)
        is InlNode.Del -> inlines(n.k)
        is InlNode.A -> inlines(n.k)
        is InlNode.C -> append(n.x)
        is InlNode.M -> append(n.tex)
        is InlNode.Img -> append(n.alt)
        InlNode.Br -> append('\n')
        is InlNode.Fn -> append(n.n)
    }
}
