package com.ccmonitor.mobile.ui.files

import android.content.ContentResolver
import android.net.Uri
import android.provider.DocumentsContract
import android.webkit.MimeTypeMap
import com.ccmonitor.mobile.core.claude.link.LocalFile
import com.ccmonitor.mobile.core.claude.link.LocalSaves
import java.io.IOException

/**
 * 下到本机那一侧在 Android 上那一份：`folder` 是系统「选文件夹」给的那一棵（存储访问框架的树，`Uri` 原样一串；选的时候记了长期授权，
 * 进程没了再开照它开）。列名字 · 新建 / 盖一份 · 删掉没下成的那一份。
 */
class TreeFolder(
    private val resolver: ContentResolver,
) : LocalSaves {
    override fun names(folder: String): List<String> = runCatching { Tree(folder).children().map { it.first } }.getOrDefault(emptyList())

    override fun create(
        folder: String,
        name: String,
        over: String?,
    ): LocalFile {
        val t = Tree(folder)
        val uri =
            over?.let { o -> t.children().firstOrNull { it.first == o }?.second }
                ?: guarded { DocumentsContract.createDocument(resolver, t.parent, mimeOf(name), name) }
                ?: throw IOException(name)
        return LocalFile(
            fd = { guarded { resolver.openFileDescriptor(uri, "wt")?.detachFd() } ?: throw IOException(name) },
            drop = { runCatching { DocumentsContract.deleteDocument(resolver, uri) } },
        )
    }

    private inner class Tree(
        folder: String,
    ) {
        private val tree: Uri = Uri.parse(folder)
        private val root = guarded { DocumentsContract.getTreeDocumentId(tree) }
        val parent: Uri = DocumentsContract.buildDocumentUriUsingTree(tree, root)

        /** 里面那几项：名字 ⇒ 它的文档地址。 */
        fun children(): List<Pair<String, Uri>> {
            val q = DocumentsContract.buildChildDocumentsUriUsingTree(tree, root)
            val cols = arrayOf(DocumentsContract.Document.COLUMN_DOCUMENT_ID, DocumentsContract.Document.COLUMN_DISPLAY_NAME)
            return guarded {
                resolver
                    .query(q, cols, null, null, null)
                    ?.use { c ->
                        buildList { while (c.moveToNext()) add(c.getString(1) to DocumentsContract.buildDocumentUriUsingTree(tree, c.getString(0))) }
                    }.orEmpty()
            }
        }
    }

    private fun mimeOf(name: String): String =
        MimeTypeMap.getSingleton().getMimeTypeFromExtension(name.substringAfterLast('.', "").lowercase()) ?: "application/octet-stream"

    /** 存储那一侧拒（没授权 · 那一棵不在了 · 那一串不是一棵）也当本机开不了：同一句。 */
    private fun <T> guarded(f: () -> T): T =
        try {
            f()
        } catch (e: SecurityException) {
            throw IOException(e)
        } catch (e: IllegalArgumentException) {
            throw IOException(e)
        }
}
