package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.FileKind
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** 文件那几问（`files-*`）：路径按原始字节存、上线交回原形；请求照协议的格；应答经 `link/wire/` 的声明类解。 */
class FilesTest {
    private val bad = byteArrayOf('/'.code.toByte(), 'x'.code.toByte(), 0xff.toByte())

    @Test
    fun `路径：合法 UTF-8 上线是那一串，否则 b16；读回来同一份字节`() {
        assertEquals("/home/a", RemotePath.of("/home/a").wire)
        val raw = RemotePath(bad)
        assertEquals(mapOf("b16" to "2f78ff"), raw.wire)
        assertArrayEquals(bad, RemotePath.of(mapOf("b16" to "2f78ff"))!!.bytes)
        assertEquals(raw, RemotePath.of(raw.wire))
        assertNull("认不出的形 ⇒ null", RemotePath.of(42))
        assertNull("单数位的十六进制 ⇒ null", RemotePath.of(mapOf("b16" to "2f7")))
    }

    @Test
    fun `files-ls：先后 · 末段名 · 上一级 · 面包屑都照那台交来的；目录链接算目录；按大小排交 sort`() {
        val reply =
            mapOf(
                "entries" to
                    listOf(
                        mapOf("path" to "/w/src", "name" to "src", "kind" to "dir"),
                        mapOf("path" to "/w/ln", "name" to "ln", "kind" to "symlink", "link_dir" to true, "link_to" to "dir"),
                        mapOf("path" to "/w/a.txt", "name" to "a.txt", "kind" to "file", "size" to 12.0, "mtime_text" to "10-14 09:00"),
                        mapOf("path" to mapOf("b16" to "2f772fff"), "name" to mapOf("b16" to "ff"), "kind" to "file", "size" to 1.0),
                    ),
                "up" to "/",
                "crumbs" to listOf(mapOf("name" to "/", "path" to "/"), mapOf("name" to "w", "path" to "/w")),
                "truncated" to false,
                "total" to 4.0,
                "unreadable" to 0.0,
            )
        val got = FilesLs.of(reply)!!
        assertEquals("照交来的先后，不另排", listOf("src", "ln", "a.txt", "�"), got.entries.map { it.name })
        assertEquals(listOf(true, true, false, false), got.entries.map { it.isDir })
        assertEquals("名字里那一段字节原样留着", mapOf("b16" to "ff"), got.entries[3].rel.wire)
        assertEquals(12L, got.entries[2].size)
        assertEquals("10-14 09:00", got.entries[2].mtime)
        assertEquals("/", got.up?.shown)
        assertEquals(listOf("/" to "/", "w" to "/w"), got.crumbs.map { (l, p) -> l to p.shown })
        assertEquals(mapOf("path" to "/w", "limit" to FilesLs.LIMIT), FilesLs.args(RemotePath.of("/w")))
        assertEquals(mapOf("by" to "size"), FilesLs.args(RemotePath.of("/w"), bySize = true)["sort"])
        assertNull("缺 entries ⇒ 解不出", FilesLs.of(mapOf("truncated" to false)))
    }

    @Test
    fun `写的那几问：根是当前目录，rel 是一段名字；敲的名字带 single`() {
        val dir = RemotePath.of("/w")
        assertEquals(mapOf("root" to "/w", "rel" to "new", "single" to true), FilesMkdir.args(dir, "new"))
        assertEquals(mapOf("root" to "/w", "rel" to "n.txt", "single" to true), FilesCreate.args(dir, "n.txt"))
        val from = RemotePath(byteArrayOf('x'.code.toByte(), 0xff.toByte()))
        assertEquals(mapOf("root" to "/w", "from" to mapOf("b16" to "78ff"), "to" to "ok", "single" to true), FilesRename.args(dir, from, "ok"))
        assertEquals(mapOf("root" to "/w", "items" to listOf(mapOf("rel" to "f"))), FilesDelete.args(dir, RemotePath.of("f"), recursive = false))
        assertEquals(mapOf("root" to "/w", "items" to listOf(mapOf("rel" to "d", "recursive" to true))), FilesDelete.args(dir, RemotePath.of("d"), recursive = true))
        assertEquals(mapOf("root" to "/w", "items" to listOf(mapOf("rel" to "f")), "mode" to 420L), FilesChmod.args(dir, RemotePath.of("f"), 420L))
        assertEquals(mapOf("path" to "/w/f"), FilesStat.args(RemotePath.of("/w/f")))
        assertEquals(420L, FilesStat.modeOf(mapOf("path" to "/w/f", "kind" to "file", "size" to 0.0, "readonly" to false, "mode" to 420.0, "owner" to null, "link_target" to null)))
        assertEquals(FilesStat.Place(FileKind.DIR, RemotePath.of("/w")), FilesStat.placeOf(mapOf("path" to "/w/d", "up" to "/w", "kind" to "dir", "size" to 0.0, "readonly" to false, "owner" to null, "link_target" to null)))
        assertEquals(FilesStat.Place(FileKind.DIR, null), FilesStat.placeOf(mapOf("path" to "/", "up" to null, "kind" to "dir", "size" to 0.0, "readonly" to false, "owner" to null, "link_target" to null)))
    }

    @Test
    fun `一组只交一项：那一项做完才算成，没成带后端那一句与那一项的原话`() {
        assertEquals(Batch.Done, Batch.fateOf(mapOf("outcomes" to listOf(mapOf("key" to "0", "state" to "done", "removed" to 3.0)), "said" to "已删除 1 项")))
        assertEquals(
            Batch.Refused("删除失败 ×1", "EACCES"),
            Batch.fateOf(mapOf("outcomes" to listOf(mapOf("key" to "0", "state" to "failed", "code" to "io_failed", "detail" to "EACCES")), "said" to "删除失败 ×1")),
        )
        assertNull("解不出 ⇒ null", Batch.fateOf(mapOf("path" to "/w/d", "removed" to 5.0, "remaining" to 3.0)))
    }

    @Test
    fun `files-ls 每行的门照那台判的画：许读多少 · 不许时那一句；当图读的手机不预览`() {
        fun gated(
            name: String,
            gates: Map<String, Any?>,
        ) = mapOf("path" to "/w/$name", "name" to name, "kind" to "file", "gates" to gates)
        val got =
            FilesLs.of(
                mapOf(
                    "entries" to
                        listOf(
                            gated("a.rs", mapOf("preview" to mapOf("text" to mapOf("cap" to 65_536.0, "lang" to "rs")), "edit" to mapOf("cap" to 8_388_608.0))),
                            gated("big.log", mapOf("preview" to mapOf("no" to "太大"), "edit" to mapOf("no" to "超编辑上限"))),
                            gated("p.png", mapOf("preview" to mapOf("image" to 4_194_304.0), "view" to mapOf("cap" to 1.0), "edit" to mapOf("cap" to 8_388_608.0))),
                            mapOf("path" to "/w/d", "name" to "d", "kind" to "dir"),
                        ),
                    "truncated" to false,
                    "total" to 4.0,
                    "crumbs" to emptyList<Any>(),
                ),
            )!!
        val e = got.entries
        assertEquals(FilesLs.Open(cap = 65_536L), e[0].preview)
        assertEquals(FilesLs.Open(cap = 8_388_608L), e[0].edit)
        assertEquals(FilesLs.Open(said = "太大"), e[1].preview)
        assertEquals(FilesLs.Open(said = "超编辑上限"), e[1].edit)
        assertNull("当图读的那一种手机不预览", e[2].preview)
        assertNull("目录没有门", e[3].preview)
        assertNull(e[3].edit)
    }

    @Test
    fun `读改写：读回文本与摘要，存回时交回那个摘要`() {
        assertEquals(mapOf("path" to "/w/f", "max_bytes" to 65_536L), FilesReadText.args(RemotePath.of("/w/f"), 65_536L))
        assertEquals(FilesReadText.Text("hi", "ab"), FilesReadText.of(mapOf("path" to "/w/f", "text" to "hi", "bytes" to 2.0, "sha256" to "ab")))
        assertEquals(
            mapOf("root" to "/w", "rel" to "f", "content" to "hi!", "expect" to mapOf("sha256" to "ab")),
            FilesWriteText.args(RemotePath.of("/w"), RemotePath.of("f"), "hi!", "ab"),
        )
        assertEquals(RemotePath.of("/home/me"), FilesHome.of(mapOf("path" to "/home/me")))
    }

    @Test
    fun `上传：一块一块进暂存区（b16），整份摘要与块数交给提交；同名怎么办不带（那台问）`() {
        assertEquals(mapOf("key" to "k", "seq" to 2L, "content" to mapOf("b16" to "00ff")), FilesStageChunk.args("k", 2L, byteArrayOf(0, -1), 2))
        assertEquals(
            mapOf(
                "root" to "/w",
                "rel" to "up.bin",
                "key" to "k",
                "expect" to mapOf("sha256" to "ab"),
                "chunks" to 3L,
                "bytes" to 9L,
            ),
            FilesCommitUpload.args(RemotePath.of("/w"), "up.bin", "k", "ab", chunks = 3L, bytes = 9L),
        )
    }

    @Test
    fun `下载：按字节分块读回`() {
        assertEquals(mapOf("path" to "/w/f", "offset" to 7L, "len" to FilesReadChunk.LEN), FilesReadChunk.args(RemotePath.of("/w/f"), 7L))
        val c = FilesReadChunk.of(mapOf("path" to "/w/f", "offset" to 0.0, "size" to 2.0, "eof" to true, "content" to mapOf("b16" to "00ff")))!!
        assertArrayEquals(byteArrayOf(0, -1), c.bytes)
        assertEquals(true, c.eof)
        assertNull("内容不是十六进制 ⇒ 解不出", FilesReadChunk.of(mapOf("path" to "/w/f", "offset" to 0.0, "size" to 2.0, "eof" to true, "content" to mapOf("b16" to "zz"))))
    }
}
