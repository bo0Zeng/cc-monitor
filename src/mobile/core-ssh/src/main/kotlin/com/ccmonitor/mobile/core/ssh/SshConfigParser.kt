package com.ccmonitor.mobile.core.ssh

import com.ccmonitor.mobile.core.rs.Core
import org.json.JSONArray

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
 * 把 OpenSSH `~/.ssh/config` 文本解析成可导入的主机列表。解析住拨号核心（`ssh-dial` 的 `config_text`，判据也在那里），
 * 这里只把它回的 JSON 换成 [SshConfigHost]。要那份 .so。
 */
object SshConfigParser {
    fun parse(text: String): List<SshConfigHost> {
        val rows = JSONArray(Core.parseSshConfig(text))
        return (0 until rows.length()).map { i ->
            val r = rows.getJSONObject(i)
            SshConfigHost(
                alias = r.getString("alias"),
                hostName = r.getString("host_name"),
                port = r.getInt("port"),
                user = r.optString("user").takeUnless { r.isNull("user") },
                identityFile = r.optString("identity_file").takeUnless { r.isNull("identity_file") },
            )
        }
    }
}
