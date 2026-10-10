package com.ccmonitor.mobile.core.ssh

/** ssh_config 里的一个具体主机（通配 Host 已滤掉）。 */
data class SshConfigHost(
    /** Host 别名（作 label）。 */
    val alias: String,
    /** HostName（缺省回退 alias）。 */
    val hostName: String,
    val port: Int,
    val user: String?,
    /** IdentityFile 原值（手机读不到文件内容，仅用于按 basename 匹配已导入身份）。 */
    val identityFile: String?,
)

/**
 * 把 OpenSSH `~/.ssh/config` 文本解析成可导入的主机列表。纯函数。
 *
 * 支持 `Host` / `HostName` / `Port` / `User` / `IdentityFile`（键不分大小写；`Key value` 与 `Key=value` 都行）；
 * 忽略注释与空行；跳过含通配符（`*` `?` `!`）的别名；`Host a b` 多别名各出一条。
 * 不处理 `Include`、`ProxyJump` 与 Match 块。
 */
object SshConfigParser {
    private val WILDCARD = Regex("[*?!]")

    fun parse(text: String): List<SshConfigHost> {
        val result = mutableListOf<SshConfigHost>()
        var aliases: List<String> = emptyList()
        var hostName: String? = null
        var port: Int? = null
        var user: String? = null
        var identityFile: String? = null

        fun flush() {
            aliases.filterNot { it.contains(WILDCARD) }.forEach { alias ->
                result.add(SshConfigHost(alias, hostName ?: alias, port ?: DEFAULT_PORT, user, identityFile))
            }
        }

        text.lineSequence().forEach { raw ->
            val line = raw.substringBefore('#').trim()
            if (line.isEmpty()) return@forEach
            val (key, value) = splitKeyValue(line) ?: return@forEach
            when (key.lowercase()) {
                "host" -> {
                    flush() // 收束上一个 Host 块
                    aliases = value.split(WHITESPACE).filter { it.isNotEmpty() }
                    hostName = null
                    port = null
                    user = null
                    identityFile = null
                }
                "hostname" -> hostName = value
                "port" -> port = value.toIntOrNull()
                "user" -> user = value
                "identityfile" -> identityFile = value
                else -> {} // 其它键忽略
            }
        }
        flush()
        return result
    }

    /** 拆 `Key value` 或 `Key=value`（key 与 value 间可空格或 `=`）。无 value → null。 */
    private fun splitKeyValue(line: String): Pair<String, String>? {
        val eq = line.indexOf('=')
        val sp = line.indexOfFirst { it == ' ' || it == '\t' }
        val sep =
            when {
                eq >= 0 && (sp < 0 || eq < sp) -> eq
                sp >= 0 -> sp
                else -> return null
            }
        val key = line.substring(0, sep).trim()
        val value = line.substring(sep + 1).trim().removeSurrounding("\"")
        return if (key.isEmpty() || value.isEmpty()) null else key to value
    }

    private const val DEFAULT_PORT = 22
    private val WHITESPACE = Regex("\\s+")
}
