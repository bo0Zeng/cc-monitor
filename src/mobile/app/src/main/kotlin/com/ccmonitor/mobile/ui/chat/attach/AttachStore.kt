package com.ccmonitor.mobile.ui.chat.attach

import com.ccmonitor.mobile.core.claude.link.RawJson
import java.io.File
import java.io.IOException
import java.io.InputStream
import java.util.UUID

/**
 * 草稿上的附件在本机那一份：一条会话一个目录（[root]/<会话>/），里面一份一个子目录装副本（原名），`tray.json` 记先后 · 名字 · 大小 · 那台回的编号。
 * 被杀回来照它接着传（已落的不重传）；撤一份 · 发出去都删干净。发出去那一句的编号另记在 `sent/<本地编号>.json`，重发照带，送到了删。
 * 不进库：副本本来就是文件，清单跟它住一起，库的版本号不动。
 */
class AttachStore(
    private val root: File,
) {
    /** 一份：[key] 是这台给的（目录名）；[id] 是那台落好回的编号，还没落 ⇒ `null`。 */
    data class Kept(
        val key: String,
        val name: String,
        val bytes: Long,
        val image: Boolean,
        val file: File,
        val id: String?,
    )

    /** 这条会话草稿上的那几份（先后照加进来的）。清单读不动 ⇒ 空。 */
    @Synchronized
    fun list(sid: String): List<Kept> = read(sid)

    /** 拷一份进来：拷完才进清单（拷到一半坏了 ⇒ 半份删掉、抛出去，清单不动）。名字只留最后一段。 */
    @Synchronized
    @Throws(IOException::class)
    fun keep(
        sid: String,
        name: String,
        image: Boolean,
        open: () -> InputStream,
    ): Kept {
        val key =
            UUID
                .randomUUID()
                .toString()
                .replace("-", "")
                .take(KEY_LEN)
        val shown = lastSegment(name)
        val home = File(dirOf(sid), key)
        val file = File(home, shown)
        try {
            if (!home.mkdirs()) throw IOException("mkdir ${home.path}")
            open().use { src -> file.outputStream().use { src.copyTo(it) } }
        } catch (e: IOException) {
            home.deleteRecursively()
            throw e
        }
        val k = Kept(key, shown, file.length(), image, file, null)
        write(sid, read(sid) + k)
        return k
    }

    /** 那台落好了：记下编号。 */
    @Synchronized
    fun landed(
        sid: String,
        key: String,
        id: String,
    ) = write(sid, read(sid).map { if (it.key == key) it.copy(id = id) else it })

    /** 撤一份：副本连目录删掉。 */
    @Synchronized
    fun drop(
        sid: String,
        key: String,
    ) {
        File(dirOf(sid), key).deleteRecursively()
        write(sid, read(sid).filterNot { it.key == key })
    }

    /** 发出去了：这条会话草稿上的全清掉。 */
    @Synchronized
    fun clear(sid: String) {
        dirOf(sid).deleteRecursively()
    }

    /** 那一句（本地编号 [localId]）带出去的编号：记下，重发照带。 */
    @Synchronized
    fun sent(
        localId: String,
        ids: List<String>,
    ) {
        if (ids.isEmpty()) return
        val f = sentFile(localId)
        f.parentFile?.mkdirs()
        runCatching { f.writeText(RawJson.write(ids)) }
    }

    @Synchronized
    fun sentOf(localId: String): List<String> =
        runCatching { (RawJson.read(sentFile(localId).readText()) as? List<*>)?.filterIsInstance<String>() }.getOrNull().orEmpty()

    /** 那一句送到了（或不再重发）：不再记。 */
    @Synchronized
    fun settled(localId: String) {
        sentFile(localId).delete()
    }

    private fun dirOf(sid: String) = File(root, lastSegment(sid))

    private fun sentFile(localId: String) = File(File(root, SENT), lastSegment(localId) + ".json")

    private fun read(sid: String): List<Kept> {
        val home = dirOf(sid)
        val rows = runCatching { RawJson.read(File(home, MANIFEST).readText()) as? List<*> }.getOrNull().orEmpty()
        return rows.mapNotNull { r ->
            val m = r as? Map<*, *> ?: return@mapNotNull null
            val key = m["key"] as? String ?: return@mapNotNull null
            val name = m["name"] as? String ?: return@mapNotNull null
            val file = File(File(home, key), name)
            Kept(key, name, file.length(), m["image"] == true, file, m["id"] as? String).takeIf { file.isFile }
        }
    }

    private fun write(
        sid: String,
        rows: List<Kept>,
    ) {
        val home = dirOf(sid)
        if (rows.isEmpty()) {
            home.deleteRecursively()
            return
        }
        home.mkdirs()
        val body = rows.map { mapOf("key" to it.key, "name" to it.name, "image" to it.image, "id" to it.id) }
        val tmp = File(home, "$MANIFEST.tmp")
        runCatching {
            tmp.writeText(RawJson.write(body))
            if (!tmp.renameTo(File(home, MANIFEST))) tmp.delete()
        }
    }

    private companion object {
        const val MANIFEST = "tray.json"
        const val SENT = "sent"
        const val KEY_LEN = 12

        /** 别处交来的名字只留最后一段（不许带目录走出这一格）；空 · `.` · `..` ⇒ `file`。 */
        fun lastSegment(name: String): String = name.substringAfterLast('/').substringAfterLast('\\').takeIf { it.isNotBlank() && it != "." && it != ".." } ?: "file"
    }
}
