package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.rawPatch
import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.Commands
import com.ccmonitor.mobile.core.wire.gen.FileClass
import com.ccmonitor.mobile.core.wire.gen.FileKind
import com.ccmonitor.mobile.core.wire.gen.FilesCommitUploadReq
import com.ccmonitor.mobile.core.wire.gen.FilesCreateReq
import com.ccmonitor.mobile.core.wire.gen.FilesDeleteSessionReq
import com.ccmonitor.mobile.core.wire.gen.FilesMkdirReq
import com.ccmonitor.mobile.core.wire.gen.FilesRenameReq
import com.ccmonitor.mobile.core.wire.gen.FilesWriteTextReq
import com.ccmonitor.mobile.core.wire.gen.FindReply
import com.ccmonitor.mobile.core.wire.gen.FindReq
import com.ccmonitor.mobile.core.wire.gen.FindScope
import com.ccmonitor.mobile.core.wire.gen.Gate
import com.ccmonitor.mobile.core.wire.gen.GrepReply
import com.ccmonitor.mobile.core.wire.gen.GrepReq
import com.ccmonitor.mobile.core.wire.gen.LsEntry
import com.ccmonitor.mobile.core.wire.gen.LsHead
import com.ccmonitor.mobile.core.wire.gen.LsReq
import com.ccmonitor.mobile.core.wire.gen.LsSort
import com.ccmonitor.mobile.core.wire.gen.LsSortBy
import com.ccmonitor.mobile.core.wire.gen.OnExists
import com.ccmonitor.mobile.core.wire.gen.PreviewGate
import com.ccmonitor.mobile.core.wire.gen.ReadTextReq
import com.ccmonitor.mobile.core.wire.gen.Sha256Expect
import com.ccmonitor.mobile.core.wire.gen.Staged
import com.ccmonitor.mobile.core.wire.gen.StatReq
import com.ccmonitor.mobile.core.wire.gen.WirePath
import com.ccmonitor.mobile.core.wire.gen.WirePathObjValue
import java.nio.ByteBuffer
import java.nio.charset.CharacterCodingException
import java.nio.charset.CodingErrorAction

/*
 * 那台的文件：列 · 读 · 存回 · 新建 · 改名 · 删 · 改权限 · 上传的提交（分块那一圈在核心里，[CoreLink.transfer]）。都是那台后端的帧命令，
 * 桌面文件窗口问的同一族；能不能做 · 出错那一句 · 冲突都由那台判、那台写。手机只拼入参，回包由嵌的 Rust 核心按 `cmd-wire` 那一份解过、
 * 照 `core-wire` 生成的类解。写的那几问照桌面的切法：`root` ＝ 当前目录，`rel` ＝ 一段名字（敲的名字带 `single`，合不合法那台判）。
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

    /** 上线那一形，生成的线上类型（拼命令入参用）。 */
    val typed: WirePath get() = text?.let { WirePath.Text(it) } ?: WirePath.Obj(WirePathObjValue(hex(raw)))

    override fun equals(other: Any?): Boolean = other is RemotePath && raw.contentEquals(other.raw)

    override fun hashCode(): Int = raw.contentHashCode()

    override fun toString(): String = shown

    companion object {
        private const val HEX_KEY = "b16"

        fun of(text: String): RemotePath = RemotePath(text.toByteArray(Charsets.UTF_8))

        /** 线上那一形 ⇒ 路径；认不出 ⇒ `null`。 */
        fun of(wire: WirePath): RemotePath? =
            when (wire) {
                is WirePath.Text -> of(wire.value)
                is WirePath.Obj -> unhex(wire.value.b16)?.let(::RemotePath)
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
internal fun hex(b: ByteArray): String {
    val s = StringBuilder(b.size * 2)
    for (i in b.indices) s.append(HEX[(b[i].toInt() shr 4) and 0xf]).append(HEX[b[i].toInt() and 0xf])
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
    const val COMMAND: String = Commands.FilesHome.NAME

    fun of(data: Any?): RemotePath? = Commands.FilesHome.reply(data)?.let { RemotePath.of(it.path) }

    /** 那台的家目录写成的那一串：原样交那台（`files-ls` · 新会话的 `cwd`），由那台按它的家目录展开，手机不拼。 */
    const val TILDE: String = "~"
}

/** `files-ls`：一个目录的直接子项（那台读目录的顺序；排序是屏上的事）。 */
object FilesLs {
    const val COMMAND: String = Commands.FilesLs.NAME

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
        /** 预览那道门（那台判的：当文本读 · 当图读 · 不预览那一句），原样；目录 ⇒ `null`。 */
        val preview: PreviewGate? = null,
        /** 编辑那道门（那台判的）；目录 ⇒ `null`。 */
        val edit: Open? = null,
        /** 那台交来的这一行原样（「能做什么」交核心认：[CoreLink.picks]）。 */
        val row: Any? = null,
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
    ): Map<String, Any?> = Commands.FilesLs.args(LsReq(path = dir.typed, limit = LIMIT, sort = LsSort(LsSortBy.SIZE).takeIf { bySize }))

    /**
     * 不带记号那一问的回包（整份，带键集合那一形：`order` 是那台排好的先后、`up` 按键给每一行、`head.info` 是这一问的头）
     * ⇒ 那几行照先后摊开。手机不存目录副本，恒问整份；有一格路径读不出 ⇒ 整份不认（`null`）。
     */
    fun of(data: Any?): Listing? {
        val (rows, head) = whole(data) ?: return null
        val entries = rows.map { (raw, e) -> entryOf(e)?.copy(row = raw) }
        val crumbs = head.crumbs.map { c -> RemotePath.of(c.name)?.let { n -> RemotePath.of(c.path)?.let { n.shown to it } } }
        // 上一级缺 ＝ 到顶；给了却读不出 ⇒ 同那几格读不出。
        val up = head.up?.let { RemotePath.of(it) ?: return null }
        if (null in entries || null in crumbs) return null
        return Listing(entries.filterNotNull(), head.truncated, head.total, up, crumbs.filterNotNull())
    }

    /** 整份那一形的回包 ⇒ 照先后的每一行（原行 ＋ 解好的那一份）＋ 头；不是整份 · 有一行解不出 ⇒ `null`。 */
    private fun whole(data: Any?): Pair<List<Pair<Any?, LsEntry>>, LsHead>? {
        val p = rawPatch(data)?.takeIf { it.whole && it.order != null } ?: return null
        val head = Wire.bind<LsHead>(p.head?.info) ?: return null
        val order = p.order.orEmpty()
        val rows = order.mapNotNull { k -> p.up[k].let { raw -> Wire.bind<LsEntry>(raw)?.let { raw to it } } }
        return if (rows.size == order.size) rows to head else null
    }

    private fun entryOf(e: LsEntry): Entry? {
        val path = RemotePath.of(e.path) ?: return null
        val rel = RemotePath.of(e.name) ?: return null
        val g = e.gates
        val edit =
            when (val x = g?.edit) {
                is Gate.Cap -> Open(cap = x.value)
                is Gate.No -> Open(said = x.value)
                null -> null
            }
        // 点不点得进去：那台画成文件夹的那几行（目录 · 指向目录的链接，`files/class.rs::class_of_entry`）。
        return Entry(path, rel, e.`class` == FileClass.FOLDER, e.size, e.sizeText, e.mtimeText, g?.preview, edit)
    }

    /** 新建会话的目录浏览：只列文件夹（含指向目录的链接）。路径是那台写好的全路径，或 `~` 开头的（那台展开）。 */
    fun dirs(path: String): Map<String, Any?> = Commands.FilesLs.args(LsReq(path = WirePath.Text(path), dirsOnly = true))

    data class Dir(
        val path: String,
        val name: String,
        /** 屏上那一串（那台按它的家目录写好的 `~/…`，照抄）。 */
        val shown: String,
    )

    /**
     * [dirs] 那一问的一层：那台排好的文件夹 ＋ 上一级（到顶 ⇒ `null`）＋ 这一层屏上那一串（那台写好的）＋
     * 这一层的绝对路径（面包屑最后一段；问的是 `~/…` 时就是那台展开的那一条；没有或读不成字 ⇒ `null`）。
     */
    data class Dirs(
        val dirs: List<Dir>,
        val up: String?,
        val shown: String,
        val at: String?,
    )

    /** [dirs] 那一问的回包；路径或名字读不成字（那台用 `b16` 交的）的那一行不列。形状对不上 ⇒ `null`。 */
    fun dirsOf(data: Any?): Dirs? {
        val (rows, head) = whole(data) ?: return null
        val dirs =
            rows.mapNotNull { (_, e) ->
                val p = (e.path as? WirePath.Text)?.value ?: return@mapNotNull null
                val n = (e.name as? WirePath.Text)?.value ?: return@mapNotNull null
                Dir(p, n, RemotePath.of(e.shown)?.shown ?: return@mapNotNull null)
            }
        return Dirs(
            dirs,
            (head.up as? WirePath.Text)?.value,
            RemotePath.of(head.shown)?.shown ?: return null,
            (head.crumbs.lastOrNull()?.path as? WirePath.Text)?.value,
        )
    }
}

/** `files-find`：按名字搜（那台的文件清单），起点是 [under]、只搜它以下（`scope: under`）。 */
object FilesFind {
    const val COMMAND: String = Commands.FilesFind.NAME

    fun args(
        under: RemotePath,
        query: String,
    ): Map<String, Any?> = Commands.FilesFind.args(FindReq(query = query, under = under.typed, scope = FindScope.UNDER))

    /** 命中（每条带路径 · 是不是目录 · 相对起点所在的目录）；解不出 ⇒ `null`。 */
    fun of(data: Any?): FindReply? = Commands.FilesFind.reply(data)
}

/** `files-grep`：按内容搜 [path] 以下的文件（每份第一处命中）。 */
object FilesGrep {
    const val COMMAND: String = Commands.FilesGrep.NAME

    fun args(
        path: RemotePath,
        needle: String,
    ): Map<String, Any?> = Commands.FilesGrep.args(GrepReq(path = path.typed, needle = WirePath.Text(needle)))

    /** 命中（相对起点的路径 · 行号 · 那一行）；解不出 ⇒ `null`。 */
    fun of(data: Any?): GrepReply? = Commands.FilesGrep.reply(data)
}

/** `files-stat`：一个路径的元数据（改权限那一框要它此刻的权限位 · 打开一个路径要它是不是目录、在哪个目录里）。 */
object FilesStat {
    const val COMMAND: String = Commands.FilesStat.NAME

    fun args(path: RemotePath): Map<String, Any?> = Commands.FilesStat.args(StatReq(path.typed))

    /** unix 权限位；那台没有这一说 ⇒ `null`；解不出 ⇒ `null`。 */
    fun modeOf(data: Any?): Int? = Commands.FilesStat.reply(data)?.mode

    /** 是什么（跟链接）· 上一级（到顶 ⇒ `null`），都是那台判的；解不出 ⇒ `null`。 */
    fun placeOf(data: Any?): Place? {
        val r = Commands.FilesStat.reply(data) ?: return null
        return Place(r.kind, r.up?.let { RemotePath.of(it) ?: return null })
    }

    data class Place(
        val kind: FileKind,
        val up: RemotePath?,
    )
}

/** `files-read-text`：读一份文本（预览 · 编辑）。 */
object FilesReadText {
    const val COMMAND: String = Commands.FilesReadText.NAME

    data class Text(
        val text: String,
        /** 读到的那一份字节的 SHA-256：存回时交回去（盘上被改过 ⇒ 那台拒）。 */
        val sha256: String,
    )

    /** [cap] ＝ 列表那一行上那台给的那道门许读多少。 */
    fun args(
        path: RemotePath,
        cap: Long,
    ): Map<String, Any?> = Commands.FilesReadText.args(ReadTextReq(path = path.typed, maxBytes = cap))

    fun of(data: Any?): Text? = Commands.FilesReadText.reply(data)?.let { Text(it.text, it.sha256) }
}

/** `files-write-text`：覆盖写一份已经在的文本（带读时的摘要）。 */
object FilesWriteText {
    const val COMMAND: String = Commands.FilesWriteText.NAME

    fun args(
        dir: RemotePath,
        name: RemotePath,
        text: String,
        sha256: String,
    ): Map<String, Any?> =
        Commands.FilesWriteText.args(FilesWriteTextReq(root = dir.typed, rel = name.typed, content = WirePath.Text(text), expect = Sha256Expect(sha256)))
}

/** `files-mkdir`：在当前目录新建一个目录（敲的名字）。 */
object FilesMkdir {
    const val COMMAND: String = Commands.FilesMkdir.NAME

    fun args(
        dir: RemotePath,
        typed: String,
    ): Map<String, Any?> = Commands.FilesMkdir.args(FilesMkdirReq(root = dir.typed, rel = WirePath.Text(typed), single = true))
}

/** `files-create`：在当前目录新建一份空文件（敲的名字）。 */
object FilesCreate {
    const val COMMAND: String = Commands.FilesCreate.NAME

    fun args(
        dir: RemotePath,
        typed: String,
    ): Map<String, Any?> = Commands.FilesCreate.args(FilesCreateReq(root = dir.typed, rel = WirePath.Text(typed), single = true))
}

/** `files-rename`：同一目录里改名（[from] 是那一项的末段，原样字节）。 */
object FilesRename {
    const val COMMAND: String = Commands.FilesRename.NAME

    fun args(
        dir: RemotePath,
        from: RemotePath,
        typed: String,
    ): Map<String, Any?> = Commands.FilesRename.args(FilesRenameReq(root = dir.typed, from = from.typed, to = WirePath.Text(typed), single = true))
}

/** `files-delete-session`：删一份历史会话（记录文件连同它的附属）；能不能删那台照行上 `can.delete` 判过。 */
object FilesDeleteSession {
    const val COMMAND: String = Commands.FilesDeleteSession.NAME

    fun args(sid: String): Map<String, Any?> = Commands.FilesDeleteSession.args(FilesDeleteSessionReq(sid))
}

/**
 * `files-commit-upload`：暂存区里传完的那一份挪进当前目录（整份摘要那台照核；名字带 single，那台判）。同名怎么办 [onExists]：不带 ⇒ 那台问（码 `exists` ＋ 同名清单，
 * 块与暂存件都留着）；`overwrite` · `keep_both` ⇒ 同一份暂存件再交一次。
 */
object FilesCommitUpload {
    const val COMMAND: String = Commands.FilesCommitUpload.NAME

    /** [staged]：核心送完的那一份（[CoreLink.transfer] 的 [Transfer.Stage] 回的那一形，`Wire.bind<Staged>` 解）。 */
    fun args(
        dir: RemotePath,
        name: String,
        staged: Staged,
        onExists: String? = null,
    ): Map<String, Any?> =
        Commands.FilesCommitUpload.args(
            FilesCommitUploadReq(
                root = dir.typed,
                rel = WirePath.Text(name),
                key = staged.key,
                onExists = onExists?.let { w -> OnExists.entries.first { it.wire == w } },
                expect = Sha256Expect(staged.sha256),
                chunks = staged.chunks,
                bytes = staged.bytes,
                single = true,
            ),
        )
}
