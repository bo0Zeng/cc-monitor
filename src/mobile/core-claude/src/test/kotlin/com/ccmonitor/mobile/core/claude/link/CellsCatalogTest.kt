package com.ccmonitor.mobile.core.claude.link

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test

class CellsCatalogTest {
    private val catalog = CellsCatalog.of(Fixtures.json("cells-catalog.golden.json"))

    @Test
    fun `格目录金样解得出（后端 cells_catalog_tests 写的那一份）`() {
        assertNotNull(catalog)
    }

    @Test
    fun `★ 手机读的每一格都在格目录里`() {
        assertEquals(emptyList<CellsCatalog.Cell>(), catalog!!.missing(MobileCells.READS))
    }

    @Test
    fun `正控：目录里没有的格逮得住`() {
        val fake = CellsCatalog.Cell("needs_row", "needs.notACell")
        assertEquals(listOf(fake), catalog!!.missing(MobileCells.READS + fake))
        assertEquals(listOf(CellsCatalog.Cell("no_such_product", "x")), catalog.missing(listOf(CellsCatalog.Cell("no_such_product", "x"))))
    }

    @Test
    fun `形状不对 ⇒ 解不出，不当空目录`() {
        assertNull(CellsCatalog.of(mapOf("products" to "x")))
        assertNull(CellsCatalog.of(listOf<Any>()))
    }
}
