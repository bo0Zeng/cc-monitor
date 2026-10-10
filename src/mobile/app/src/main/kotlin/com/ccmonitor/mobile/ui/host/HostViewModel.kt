package com.ccmonitor.mobile.ui.host

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.data.db.Identity
import com.ccmonitor.mobile.core.data.db.Launcher
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.data.repo.IdentityRepository
import com.ccmonitor.mobile.core.data.repo.LauncherRepository
import com.ccmonitor.mobile.core.ssh.SshConfigHost
import com.ccmonitor.mobile.core.ssh.SshConfigParser
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class HostViewModel(
    private val hostRepo: HostRepository,
    identityRepo: IdentityRepository,
    launcherRepo: LauncherRepository,
) : ViewModel() {
    val hosts: StateFlow<List<Host>> =
        hostRepo.observeAll().stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    val identities: StateFlow<List<Identity>> =
        identityRepo.observeAll().stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** launcher 按 hostId 分组，供主机行的 chips。 */
    val launchersByHost: StateFlow<Map<String, List<Launcher>>> =
        launcherRepo
            .observeAll()
            .map { all -> all.groupBy { it.hostId } }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyMap())

    /** 载入单个主机供编辑器回显（null 表示未找到）。 */
    suspend fun getHost(id: String): Host? = hostRepo.get(id)

    fun saveHost(host: Host) = viewModelScope.launch(Dispatchers.IO) { hostRepo.save(host) }

    /** 把明文密码加密进 host.passwordEnc（plain 为空或 null 则清空）。加密是同步的，编辑器在 onClick 里算出最终 host 再 saveHost。 */
    fun encryptPassword(
        host: Host,
        plain: String?,
    ): Host = hostRepo.withPassword(host, plain)

    /** 解密现有主机的密码，供编辑时回显。 */
    suspend fun hostPassword(host: Host): String? = hostRepo.password(host)

    fun deleteHost(host: Host) = viewModelScope.launch(Dispatchers.IO) { hostRepo.delete(host) }

    /**
     * 解析粘贴的 ssh config 文本并批量导入主机，返回导入条数。
     * IdentityFile 按 basename 匹配已导入钥匙的 label，匹配到就设 authRef，否则留 null 待编辑器里补。
     * 手机读不到 IdentityFile 指向的私钥文件，所以只按名字匹配。
     */
    suspend fun importSshConfig(text: String): Int =
        withContext(Dispatchers.IO) {
            val parsed = SshConfigParser.parse(text)
            if (parsed.isEmpty()) return@withContext 0
            val hosts = hostsFromSshConfig(parsed, identities.value)
            hostRepo.saveAll(hosts)
            hosts.size
        }
}

/**
 * 从 ssh config 的一个 `Host` 别名派生稳定的机器 id。
 *
 * `HostRepository.saveAll` 是 upsert，id 稳定，同一份配置导两次才不会生出两套机器。
 * 一份 ssh config 里 `Host` 别名本来唯一，就是这台机器在那份配置里的身份；
 * 改了别名就算换了一台，旧的那条留着，由人自己删。
 * 手工建的机器不受影响，它们的 id 不带 `cfg-` 前缀。
 */
fun hostIdForSshAlias(alias: String): String = SSH_CONFIG_HOST_ID_PREFIX + alias.trim()

/** [hostIdForSshAlias] 的前缀。手工建的机器不带它，所以导入永远不会覆盖手工建的那些。 */
const val SSH_CONFIG_HOST_ID_PREFIX = "cfg-"

/**
 * 解析结果加已有钥匙，得出要落库的机器列表。
 *
 * 钥匙按 `IdentityFile` 的 basename 精确匹配已导入的那几把（手机读不到文件内容，只能按名字认）。
 */
fun hostsFromSshConfig(
    parsed: List<SshConfigHost>,
    known: List<Identity>,
): List<Host> =
    parsed.map { p ->
        val base = p.identityFile?.substringAfterLast('/')?.substringAfterLast('\\')
        val matched = base?.let { b -> known.firstOrNull { it.label.equals(b, ignoreCase = true) } }
        Host(
            id = hostIdForSshAlias(p.alias),
            label = p.alias,
            host = p.hostName,
            port = p.port,
            username = p.user ?: "",
            authRef = matched?.id,
        )
    }
