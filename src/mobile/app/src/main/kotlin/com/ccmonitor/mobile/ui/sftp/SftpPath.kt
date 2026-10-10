package com.ccmonitor.mobile.ui.sftp

/*
 * SFTP 远端路径的纯字符串逻辑。远端恒为 POSIX `/` 分隔（sshj SFTP）。
 */

/** 上级目录（尾斜杠归一后取最后 `/` 前；根的上级=根）。 */
internal fun parentOf(path: String): String = path.trimEnd('/').substringBeforeLast('/', "").ifEmpty { "/" }

/** 书签标签 = 路径末段目录名（根为 "/"）。 */
internal fun bookmarkLabel(path: String): String = path.trimEnd('/').substringAfterLast('/').ifEmpty { "/" }

/** 拼子路径（处理根目录避免双斜杠）。 */
internal fun joinPath(
    dir: String,
    name: String,
): String = if (dir == "/") "/$name" else "${dir.trimEnd('/')}/$name"

/** 面包屑段 (显示名, 完整路径)，绝对路径首段为 "/"；非绝对路径原样单段。 */
internal fun buildCrumbs(path: String): List<Pair<String, String>> {
    if (!path.startsWith("/")) return listOf(path to path)
    val out = mutableListOf("/" to "/")
    var acc = ""
    path.trim('/').split('/').filter { it.isNotEmpty() }.forEach { seg ->
        acc += "/$seg"
        out.add(seg to acc)
    }
    return out
}
