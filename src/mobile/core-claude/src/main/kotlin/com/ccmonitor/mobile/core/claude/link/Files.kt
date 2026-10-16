package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.BatchItemState
import com.ccmonitor.mobile.core.claude.link.wire.BatchReply
import com.ccmonitor.mobile.core.claude.link.wire.FileKind
import com.ccmonitor.mobile.core.claude.link.wire.FilesPathReply
import com.ccmonitor.mobile.core.claude.link.wire.LsEntry
import com.ccmonitor.mobile.core.claude.link.wire.LsReply
import com.ccmonitor.mobile.core.claude.link.wire.ReadChunkReply
import com.ccmonitor.mobile.core.claude.link.wire.ReadTextReply
import com.ccmonitor.mobile.core.claude.link.wire.StatReply
import com.ccmonitor.mobile.core.claude.link.wire.Wire
import java.nio.ByteBuffer
import java.nio.charset.CharacterCodingException
import java.nio.charset.CodingErrorAction

/*
 * 那台的文件：列 · 读 · 存回 · 新建 · 改名 · 删 · 改权限 · 分块上传 · 分块下载。都是那台后端的帧命令，
 * 桌面文件窗口问的同一族；能不能做 · 出错那一句 · 冲突都由那台判、那台写。手机只拼入参，回包由嵌的 Rust 核心按 `cmd-wire` 那一份解过、
 * 装进 `link/wire/` 的声明类。写的那几问照桌面的切法：`root` ＝ 当前目录，`rel` ＝ 一段名字（敲的名字带 `single`，合不合法那台判）。
 */

/**
 * 那台的一个路径，按原始字节存（`cmd-wire` 的 `WirePath`）：合法 UTF-8 上线是那一串，否则 `{"b16": …}`。
 * 末段 · 上一级 · 面包屑由那台按自己的分隔符切好交来（`files-ls` 的 `name` · `up` · `crumbs`），这里不切。
 */
class RemotePath(
    bytes: ByteArray,
) {
    private val raw = bytes.copyOf()

    val bytes: ByteArray get() = raw.copyOf()

    /** 合法 UTF-8 ⇒ 那一串；否则 `null`。 */
    val text: String? = strict(raw)

    /** 屏上那一串（认不出的字节换成替换符）。 */
    val shown: String = text ?: String(raw, Charsets.UTF_8)

    /** 上线那一形。 */
    val wire: Any = text ?: mapOf(HEX_KEY to hex(raw))

    override fun equals(other: Any?): Boolean = other is RemotePath && raw.contentEquals(other.raw)

    override fun hashCode(): Int = raw.contentHashCode()

    override fun toString(): String = shown

    companion object {
        private const val HEX_KEY = "b16"

        fun of(text: String): RemotePath = RemotePath(text.toByteArray(Charsets.UTF_8))

        /** 线上那一形 ⇒ 路径；认不出 ⇒ `null`。 */
        fun of(wire: Any?): RemotePath? =
            when (wire) {
                is String -> of(wire)
                is Map<*, *> -> (wire[HEX_KEY] as? String)?.let(::unhex)?.let(::RemotePath)
                else -> null
            }

        private fun strict(b: ByteArray): String? =
            try {
                Charsets.UTF_8
                    .newDecoder()
                    .onMalformedInput(CodingErrorAction.REPORT)
                    .onUnmappableCharacter(CodingErrorAction.REPORT)
                    .decode(ByteBuffer.wrap(b))
                    .toString()
            } catch (_: CharacterCodingException) {
                null
            }
    }
}

/** 字节 ⇒ 小写十六进制（`{"b16": …}` 那一形）。 */
internal fun hex(
    b: ByteArray,
    n: Int = b.size,
): String {
    val s = StringBuilder(n * 2)
    for (i in 0 until n) s.append(HEX[(b[i].toInt() shr 4) and 0xf]).append(HEX[b[i].toInt() and 0xf])
    return s.toString()
}

/** 十六进制 ⇒ 字节；不是 ⇒ `null`。 */
internal fun unhex(s: String): ByteArray? {
    if (s.length % 2 != 0) return null
    return ByteArray(s.length / 2) { i ->
        val hi = Character.digit(s[2 * i], 16)
        val lo = Character.digit(s[2 * i + 1], 16)
        if (hi < 0 || lo < 0) return null
        ((hi shl 4) or lo).toByte()
    }
}

private const val HEX = "0123456789abcdef"

/** `files-home`：那台这个用户的家（文件屏的起点）。 */
object FilesHome {
    const val COMMAND: String = "files-home"

    fun of(data: Any?): RemotePath? = Wire.bind<FilesPathReply>(data)?.let { RemotePath.of(it.path) }

    /** 新建会话的目录 chip 要的那一串：家目录读不成字 ⇒ `null`。 */
    fun text(data: Any?): String? = of(data)?.text
}

/** `files-ls`：一个目录的直接子项（那台读目录的顺序；排序是屏上的事）。 */
object FilesLs {
    const val COMMAND: String = "files-ls"

    /** 一屏至多列几条（同那台的缺省）。 */
    const val LIMIT: Long = 1000L

    data class Entry(
        val path: RemotePath,
        /** 末段（那台切好的，原样字节：写的那几问的 `rel`）。 */
        val rel: RemotePath,
        /** 目录，或跟得过去的指向目录的链接。 */
        val isDir: Boolean,
        val size: Long?,
        /** 大小那一格（那台写好的）。 */
        val sizeText: String?,
        /** 那台按它的钟写好的短写法。 */
        val mtime: String?,
        /** 预览那道门（那台判的）；目录 · 手机不画的那几种（图）⇒ `null`。 */
        val preview: Open? = null,
        /** 编辑那道门（那台判的）；目录 ⇒ `null`。 */
        val edit: Open? = null,
    ) {
        /** 屏上那一串名字。 */
        val name: String get() = rel.shown
    }

    /** 一道门：许读 [cap] 以内；不许 ⇒ [said]（那台写好的那一句）。 */
    data class Open(
        val cap: Long? = null,
        val said: String? = null,
    )

    data class Listing(
        /** 那台排好的先后，照画。 */
        val entries: List<Entry>,
        /** 截了（那台读到的比回送的多）。 */
        val truncated: Boolean,
        val total: Long,
        /** 上一级；到顶 ⇒ `null`。 */
        val up: RemotePath? = null,
        /** 面包屑：根到这一级每一段 `(字, 路径)`，根在最前。 */
        val crumbs: List<Pair<String, RemotePath>> = emptyList(),
    )

    /** 列一层；[bySize] ⇒ 按大小排（大的在前），否则按名称。目录恒在前（那台排）。 */
    fun args(
        dir: RemotePath,
        bySize: Boolean = false,
    ): Map<String, Any?> =
        buildMap {
            put("path", dir.wire)
            put("limit", LIMIT)
            if (bySize) put("sort", mapOf("by" to "size"))
        }

    /** 有一格路径读不出 ⇒ 整份不认（`null`）。 */
    fun of(data: Any?): Listing? {
        val r = Wire.bind<LsReply>(data) ?: return null
        val entries = r.entries.map(::entryOf)
        val crumbs = r.crumbs.map { c -> RemotePath.of(c.name)?.let { n -> RemotePath.of(c.path)?.let { n.shown to it } } }
        // 上一级缺 ＝ 到顶；给了却读不出 ⇒ 同那几格读不出。
        val up = r.up?.let { RemotePath.of(it) ?: return null }
        if (null in entries || null in crumbs) return null
        return Listing(entries.filterNotNull(), r.truncated, r.total, up, crumbs.filterNotNull())
    }

    private fun entryOf(e: LsEntry): Entry? {
        val path = RemotePath.of(e.path) ?: return null
        val rel = RemotePath.of(e.name) ?: return null
        val g = e.gates
        val preview = g?.preview?.let { p -> p.text?.let { Open(cap = it.cap) } ?: p.no?.let { Open(said = it) } }
        val edit = g?.edit?.let { Open(it.cap, it.no) }
        val isDir = e.kind == FileKind.DIR || (e.kind == FileKind.SYMLINK && e.linkDir == true)
        return Entry(path, rel, isDir, e.size, e.sizeText, e.mtimeText, preview, edit)
    }

    /** 新建会话的目录浏览：只列文件夹（含指向目录的链接）。路径是那台写好的全路径。 */
    fun dirs(path: String): Map<String, Any?> = mapOf("path" to path, "dirs_only" to true)

    data class Dir(
        val path: String,
        val name: String,
    )

    /** [dirs] 那一问的一层：那台排好的文件夹 ＋ 上一级（到顶 ⇒ `null`）。 */
    data class Dirs(
        val dirs: List<Dir>,
        val up: String?,
    )

    /** [dirs] 那一问的回包；路径或名字读不成字（那台用 `b16` 交的）的那一行不列。形状对不上 ⇒ `null`。 */
    fun dirsOf(data: Any?): Dirs? {
        val r = Wire.bind<LsReply>(data) ?: return null
        val dirs = r.entries.mapNotNull { e -> (e.path as? String)?.let { p -> (e.name as? String)?.let { Dir(p, it) } } }
        return Dirs(dirs, r.up as? String)
    }
}

/** `files-stat`：一个路径的元数据（改权限那一框要它此刻的权限位 · 打开一个路径要它是不是目录）。 */
object FilesStat {
    const val COMMAND: String = "files-stat"

    fun args(path: RemotePath): Map<String, Any?> = mapOf("path" to path.wire)

    /** unix 权限位；那台没有这一说 ⇒ `null`；解不出 ⇒ `null`。 */
    fun modeOf(data: Any?): Long? = Wire.bind<StatReply>(data)?.mode

    /** 是什么（跟链接，那台判的）；解不出 ⇒ `null`。 */
    fun kindOf(data: Any?): FileKind? = Wire.bind<StatReply>(data)?.kind
}

/** `files-read-text`：读一份文本（预览 · 编辑）。 */
object FilesReadText {
    const val COMMAND: String = "files-read-text"

    data class Text(
        val text: String,
        /** 读到的那一份字节的 SHA-256：存回时交回去（盘上被改过 ⇒ 那台拒）。 */
        val sha256: String,
    )

    /** [cap] ＝ 列表那一行上那台给的那道门许读多少。 */
    fun args(
        path: RemotePath,
        cap: Long,
    ): Map<String, Any?> = mapOf("path" to path.wire, "max_bytes" to cap)

    fun of(data: Any?): Text? = Wire.bind<ReadTextReply>(data)?.let { Text(it.text, it.sha256) }
}

/** `files-write-text`：覆盖写一份已经在的文本（带读时的摘要）。 */
object FilesWriteText {
    const val COMMAND: String = "files-write-text"

    fun args(
        dir: RemotePath,
        name: RemotePath,
        text: String,
        sha256: String,
    ): Map<String, Any?> = mapOf("root" to dir.wire, "rel" to name.wire, "content" to text, "expect" to mapOf("sha256" to sha256))
}

/** `files-mkdir`：在当前目录新建一个目录（敲的名字）。 */
object FilesMkdir {
    const val COMMAND: String = "files-mkdir"

    fun args(
        dir: RemotePath,
        typed: String,
    ): Map<String, Any?> = mapOf("root" to dir.wire, "rel" to typed, "single" to true)
}

/** `files-create`：在当前目录新建一份空文件（敲的名字）。 */
object FilesCreate {
    const val COMMAND: String = "files-create"

    fun args(
        dir: RemotePath,
        typed: String,
    ): Map<String, Any?> = mapOf("root" to dir.wire, "rel" to typed, "single" to true)
}

/** `files-rename`：同一目录里改名（[from] 是那一项的末段，原样字节）。 */
object FilesRename {
    const val COMMAND: String = "files-rename"

    fun args(
        dir: RemotePath,
        from: RemotePath,
        typed: String,
    ): Map<String, Any?> = mapOf("root" to dir.wire, "from" to from.wire, "to" to typed, "single" to true)
}

/** `files-delete`：删一项（交成一组里的一项）；目录连同里面全部（`recursive`）。回包是一组的结局（[Batch]）。 */
object FilesDelete {
    const val COMMAND: String = "files-delete"

    fun args(
        dir: RemotePath,
        name: RemotePath,
        recursive: Boolean,
    ): Map<String, Any?> =
        mapOf(
            "root" to dir.wire,
            "items" to
                listOf(
                    buildMap {
                        put("rel", name.wire)
                        if (recursive) put("recursive", true)
                    },
                ),
        )
}

/** `files-chmod`：改 unix 权限位（交成一组里的一项）。回包是一组的结局（[Batch]）。 */
object FilesChmod {
    const val COMMAND: String = "files-chmod"

    fun args(
        dir: RemotePath,
        name: RemotePath,
        mode: Long,
    ): Map<String, Any?> = mapOf("root" to dir.wire, "items" to listOf(mapOf("rel" to name.wire)), "mode" to mode)
}

/** 一组的回包（手机只交一项）：那一项做完 ⇒ [Done]；没成 ⇒ [Refused] 带后端写的那一句与那一项的原话；解不出 ⇒ `null`。 */
object Batch {
    sealed interface Fate

    data object Done : Fate

    data class Refused(
        val said: String,
        val detail: String,
    ) : Fate

    fun fateOf(data: Any?): Fate? {
        val r = Wire.bind<BatchReply>(data) ?: return null
        val one = r.outcomes.singleOrNull() ?: return null
        return if (one.state == BatchItemState.DONE) Done else Refused(r.said, one.detail.orEmpty())
    }
}

/** `files-stage-chunk`：上传的一块进那台的暂存区（同一份上传同一个 `key`，`seq` 从 0 数）。 */
object FilesStageChunk {
    const val COMMAND: String = "files-stage-chunk"

    fun args(
        key: String,
        seq: Long,
        buf: ByteArray,
        n: Int,
    ): Map<String, Any?> = mapOf("key" to key, "seq" to seq, "content" to mapOf("b16" to hex(buf, n)))
}

/** `files-commit-upload`：暂存区里传完的那一份挪进当前目录（整份摘要那台照核；不覆盖：目标在就拒，那一句那台写）。 */
object FilesCommitUpload {
    const val COMMAND: String = "files-commit-upload"

    fun args(
        dir: RemotePath,
        name: String,
        key: String,
        sha256: String,
        chunks: Long,
        bytes: Long,
    ): Map<String, Any?> =
        mapOf(
            "root" to dir.wire,
            "rel" to name,
            "key" to key,
            "expect" to mapOf("sha256" to sha256),
            "chunks" to chunks,
            "bytes" to bytes,
        )
}

/** `files-read-chunk`：按字节分块读回（下载）。 */
object FilesReadChunk {
    const val COMMAND: String = "files-read-chunk"

    /** 一块多大（同那台一块的上限 `READ_CHUNK_MAX_BYTES`）。 */
    const val LEN: Long = 256L * 1024

    class Chunk(
        val bytes: ByteArray,
        val size: Long,
        val eof: Boolean,
    )

    fun args(
        path: RemotePath,
        offset: Long,
    ): Map<String, Any?> = mapOf("path" to path.wire, "offset" to offset, "len" to LEN)

    fun of(data: Any?): Chunk? = Wire.bind<ReadChunkReply>(data)?.let { r -> unhex(r.content.b16)?.let { Chunk(it, r.size, r.eof) } }
}
