package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.FileKind
import com.ccmonitor.mobile.core.claude.link.wire.FilesDeleteReply
import com.ccmonitor.mobile.core.claude.link.wire.FilesPathReply
import com.ccmonitor.mobile.core.claude.link.wire.LsReply
import com.ccmonitor.mobile.core.claude.link.wire.ReadChunkReply
import com.ccmonitor.mobile.core.claude.link.wire.ReadTextReply
import com.ccmonitor.mobile.core.claude.link.wire.StatReply
import com.ccmonitor.mobile.core.claude.link.wire.Wire
import java.nio.ByteBuffer
import java.nio.charset.CharacterCodingException
import java.nio.charset.CodingErrorAction

/*
 * 那台的文件：列 · 读 · 存回 · 新建 · 改名 · 删 · 改权限 · 分块上传 · 分块下载。都是那台后端的帧命令（`IPC-COMMANDS.md` §4.2），
 * 桌面文件窗口问的同一族；能不能做 · 出错那一句 · 冲突都由那台判、那台写。手机只拼入参，回包由嵌的 Rust 核心按 `cmd-wire` 那一份解过、
 * 装进 `link/wire/` 的声明类。写的那几问照桌面的切法：`root` ＝ 当前目录，`rel` ＝ 一段名字（敲的名字带 `single`，合不合法那台判）。
 */

/**
 * 那台的一个路径，按原始字节存（`cmd-wire` 的 `WirePath`）：合法 UTF-8 上线是那一串，否则 `{"b16": …}`。
 * 末段 · 上一级 · 面包屑只认 `/`（同桌面 `filewin-contract` 的 `parent_dir` / `remote_basename`）。
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

    /** 末段（尾斜杠不算）。 */
    val name: RemotePath get() = RemotePath(trimmed().let { t -> t.copyOfRange(t.lastIndexOf(SLASH) + 1, t.size) })

    /** 上一级；根的上一级是根。 */
    val parent: RemotePath
        get() {
            val t = trimmed()
            val at = t.lastIndexOf(SLASH)
            return if (at <= 0) ROOT else RemotePath(t.copyOfRange(0, at))
        }

    /** 面包屑：`(那一段的字, 到那一段为止的路径)`，绝对路径首段是 `/`；不是绝对路径 ⇒ 整串一段。 */
    fun crumbs(): List<Pair<String, RemotePath>> {
        if (raw.firstOrNull() != SLASH) return listOf(shown to this)
        val out = mutableListOf("/" to ROOT)
        var at = 0
        while (at < raw.size) {
            val end = raw.indexOf(SLASH, at + 1).let { if (it < 0) raw.size else it }
            if (end > at + 1) {
                val upTo = RemotePath(raw.copyOfRange(0, end))
                out += upTo.name.shown to upTo
            }
            at = end
        }
        return out
    }

    private fun trimmed(): ByteArray {
        var n = raw.size
        while (n > 0 && raw[n - 1] == SLASH) n--
        return raw.copyOfRange(0, n)
    }

    override fun equals(other: Any?): Boolean = other is RemotePath && raw.contentEquals(other.raw)

    override fun hashCode(): Int = raw.contentHashCode()

    override fun toString(): String = shown

    companion object {
        private const val SLASH = '/'.code.toByte()
        private const val HEX_KEY = "b16"
        private val ROOT = RemotePath("/".toByteArray())

        fun of(text: String): RemotePath = RemotePath(text.toByteArray(Charsets.UTF_8))

        /** 线上那一形 ⇒ 路径；认不出 ⇒ `null`。 */
        fun of(wire: Any?): RemotePath? =
            when (wire) {
                is String -> of(wire)
                is Map<*, *> -> (wire[HEX_KEY] as? String)?.let(::unhex)?.let(::RemotePath)
                else -> null
            }

        private fun ByteArray.indexOf(
            b: Byte,
            from: Int,
        ): Int = (from until size).firstOrNull { this[it] == b } ?: -1

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
}

/** `files-ls`：一个目录的直接子项（那台读目录的顺序；排序是屏上的事）。 */
object FilesLs {
    const val COMMAND: String = "files-ls"

    /** 一屏至多列几条（同那台的缺省）。 */
    const val LIMIT: Long = 1000L

    data class Entry(
        val path: RemotePath,
        val name: String,
        /** 目录，或跟得过去的指向目录的链接。 */
        val isDir: Boolean,
        val size: Long?,
        /** 那台按它的钟写好的短写法。 */
        val mtime: String?,
    )

    data class Listing(
        val entries: List<Entry>,
        /** 截了（那台读到的比回送的多）。 */
        val truncated: Boolean,
        val total: Long,
    )

    fun args(dir: RemotePath): Map<String, Any?> = mapOf("path" to dir.wire, "limit" to LIMIT)

    fun of(data: Any?): Listing? {
        val r = Wire.bind<LsReply>(data) ?: return null
        val entries =
            r.entries.map { e ->
                val p = RemotePath.of(e.path) ?: return null
                Entry(p, p.name.shown, e.kind == FileKind.DIR || (e.kind == FileKind.SYMLINK && e.linkDir == true), e.size, e.mtimeText)
            }
        return Listing(entries, r.truncated, r.total)
    }
}

/** `files-stat`：一个路径的元数据（改权限那一框要它此刻的权限位）。 */
object FilesStat {
    const val COMMAND: String = "files-stat"

    fun args(path: RemotePath): Map<String, Any?> = mapOf("path" to path.wire)

    /** unix 权限位；那台没有这一说 ⇒ `null`；解不出 ⇒ `null`。 */
    fun modeOf(data: Any?): Long? = Wire.bind<StatReply>(data)?.mode
}

/** `files-read-text`：读一份文本（预览 · 编辑）。 */
object FilesReadText {
    const val COMMAND: String = "files-read-text"

    /** 一趟许读的上限。 */
    const val MAX_BYTES: Long = 256L * 1024

    data class Text(
        val text: String,
        /** 读到的那一份字节的 SHA-256：存回时交回去（盘上被改过 ⇒ 那台拒）。 */
        val sha256: String,
    )

    fun args(path: RemotePath): Map<String, Any?> = mapOf("path" to path.wire, "max_bytes" to MAX_BYTES)

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

/** `files-delete`：删一个文件，或一整个目录（一段一段删，回还剩几条；没剩才算删完）。 */
object FilesDelete {
    const val COMMAND: String = "files-delete"

    /** 删目录时一趟让那台删几条（同桌面）。 */
    const val SLICE: Long = 10_000L

    fun args(
        dir: RemotePath,
        name: RemotePath,
        recursive: Boolean,
    ): Map<String, Any?> =
        buildMap {
            put("root", dir.wire)
            put("rel", name.wire)
            if (recursive) {
                put("recursive", true)
                put("limit", SLICE)
            }
        }

    /** 还剩几条；解不出 ⇒ `null`。 */
    fun remaining(data: Any?): Long? = Wire.bind<FilesDeleteReply>(data)?.remaining
}

/** `files-chmod`：改 unix 权限位。 */
object FilesChmod {
    const val COMMAND: String = "files-chmod"

    fun args(
        dir: RemotePath,
        name: RemotePath,
        mode: Long,
    ): Map<String, Any?> = mapOf("root" to dir.wire, "rel" to name.wire, "mode" to mode)
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
            "overwrite" to false,
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
