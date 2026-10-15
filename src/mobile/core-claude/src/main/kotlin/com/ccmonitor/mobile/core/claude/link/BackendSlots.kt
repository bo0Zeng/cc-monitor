package com.ccmonitor.mobile.core.claude.link

/**
 * 包里带的那几格后端（给远端装的那两份 Linux 后端，assets 里 `backends/cc-monitor-backend-<arch>`；哪几格由构建时看字节定，
 * 见 [EmbeddedBuild.CARRIED]）。App 把要的解到私有目录（每版一次），把路径交核心（`Core.deploy`）；核心自己从文件读、自己再核一遍身份。
 */
object BackendSlots {
    /** assets 里那一格的路径。 */
    fun asset(arch: String): String = "backends/cc-monitor-backend-$arch"

    /** 解到私有目录里的文件名（目录按版本分，见 [dirName]）。 */
    fun fileName(arch: String): String = "cc-monitor-backend-$arch"

    /** 这一版的那一层目录名（换版即换目录，旧版的整层删）。 */
    val dirName: String = EmbeddedBuild.ID

    /** 交核心的那一段：`[{os, arch, id, path}]`（远端只装 Linux；身份是这一版的，核心照字节再核）。 */
    fun json(files: Map<String, String>): String =
        Json.line(files.map { (arch, path) -> mapOf("os" to "Linux", "arch" to arch, "id" to EmbeddedBuild.ID, "path" to path) })
}
