package com.ccmonitor.mobile.link

import android.content.Context
import com.ccmonitor.mobile.core.claude.link.BackendSlots
import com.ccmonitor.mobile.core.claude.link.EmbeddedBuild
import java.io.File

/**
 * 包里那几格后端解到 App 私有目录（`noBackupFilesDir/backends/<这一版>/`，每版一次；旧版那一层整个删）。
 * 先写临时件、写完再改名：解到一半被杀，下次不会拿半份去装。这一版没带 ⇒ 交空的那一段（核心照实说「这一版没带」）。
 */
class BackendFiles(
    private val context: Context,
) {
    fun slots(): String {
        val root = File(context.noBackupFilesDir, "backends")
        root.listFiles()?.filter { it.name != BackendSlots.dirName }?.forEach { it.deleteRecursively() }
        val dir = File(root, BackendSlots.dirName).apply { mkdirs() }
        val files =
            EmbeddedBuild.CARRIED.associateWith { arch ->
                val f = File(dir, BackendSlots.fileName(arch))
                if (!f.isFile) {
                    val tmp = File(dir, f.name + ".part")
                    context.assets.open(BackendSlots.asset(arch)).use { input -> tmp.outputStream().use { input.copyTo(it) } }
                    tmp.renameTo(f)
                }
                f.absolutePath
            }
        return BackendSlots.json(files)
    }
}
