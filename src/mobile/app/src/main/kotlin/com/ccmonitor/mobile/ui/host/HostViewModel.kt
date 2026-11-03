package com.ccmonitor.mobile.ui.host

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ccmonitor.mobile.core.data.db.CacheDao
import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.data.db.HostRemovalDao
import com.ccmonitor.mobile.core.data.db.HostRows
import com.ccmonitor.mobile.core.data.db.Identity
import com.ccmonitor.mobile.core.data.db.machineTable
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.data.repo.IdentityRepository
import com.ccmonitor.mobile.core.rs.Core
import com.ccmonitor.mobile.core.ssh.SshConfigParser
import com.ccmonitor.mobile.core.ssh.forgetHostKey
import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.ConfigHost
import com.ccmonitor.mobile.core.wire.gen.Endpoint
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.core.wire.gen.MachineCheck
import com.ccmonitor.mobile.core.wire.gen.MachineStateKind
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.ui.common.MachineDot
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class HostViewModel(
    private val hostRepo: HostRepository,
    identityRepo: IdentityRepository,
    backends: HostBackends,
    private val removal: HostRemovalDao,
    private val cache: CacheDao,
) : ViewModel() {
    /** 每台核心交的那一行（起着常驻流的那几台）⇒ 行首那颗点与尾字（[hostRowLook]）。 */
    @OptIn(ExperimentalCoroutinesApi::class)
    val rowLooks: StateFlow<Map<String, HostRowLook>> =
        backends.feeds
            .flatMapLatest { m ->
                if (m.isEmpty()) flowOf(emptyMap()) else combine(m.map { (id, f) -> f.state.map { id to hostRowLook(it) } }) { it.toMap() }
            }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyMap())

    val hosts: StateFlow<List<Host>> =
        hostRepo.observeAll().stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    val identities: StateFlow<List<Identity>> =
        identityRepo.observeAll().stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    /** 载入单个主机供编辑器回显（null 表示未找到）。 */
    suspend fun getHost(id: String): Host? = hostRepo.get(id)

    fun saveHost(host: Host) = viewModelScope.launch(Dispatchers.IO) { hostRepo.save(host) }

    /** 把明文密码加密进 host.passwordEnc（plain 为空或 null 则清空）。加密是同步的，编辑器在 onClick 里算出最终 host 再 saveHost。 */
    fun encryptPassword(
        host: Host,
        plain: String?,
    ): Host = hostRepo.withPassword(host, plain)

    /** 解密现有主机的密码，供编辑时回显。 */
    fun hostPassword(host: Host): String? = hostRepo.password(host)

    /**
     * 删一台：它在状态库每张表里的行一个事务删光（[HostRemovalDao.remove]）、缓存库里它那几行另清一次（[CacheDao.forgetHost]），
     * 再从核心那份库里忘掉只有它用着的主机钥匙。回撤销要的那一份（这一台已经不在 ⇒ `null`）。
     */
    fun deleteHost(host: Host): Deferred<HostRows?> =
        viewModelScope.async(Dispatchers.IO) {
            val table = hostRepo.observeAll().first()
            val gone = removal.remove(host.id) ?: return@async null
            cache.forgetHost(host.id)
            keysOnlyItUsed(table, host.id, ::dialEndpoints).forEach { forgetHostKey(it.host, it.port) }
            gone
        }

    /**
     * 撤销删除：机器那一行（连 id）与草稿 · 待发 · 在做的一组 · 旧书签放回；那台的缓存下次连上重取，主机钥匙下次连是一次首见。
     * 撤销在应用根部的反馈里跑，这一屏可能已经关了 ⇒ 不挂 [viewModelScope]。
     */
    suspend fun restoreHost(rows: HostRows) = withContext(Dispatchers.IO) { removal.restore(rows) }

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

/** 表里第 [at] 台的拨号地址（核心展开的，编辑页那一问同一份）；要那份 .so。 */
internal fun dialEndpoints(
    table: List<Host>,
    at: Int,
): List<Endpoint> =
    (Wire.read<MachineCheck>(Core.machineCheck(table.machineTable(), at)) ?: error("wire contract mismatch: machine check")).endpoints

/**
 * 删 [id] 那一台要忘的主机钥匙：它的拨号地址里，表里别的机器都没用着的那几个（同址的另一台还要那把钥匙）。
 * 钥匙按「主机 ＋ 口」记在核心那份库里，地址照核心算的（[endpointsOf]），这里只按它筛。
 */
internal fun keysOnlyItUsed(
    table: List<Host>,
    id: String,
    endpointsOf: (List<Host>, Int) -> List<Endpoint>,
): List<Endpoint> {
    val at = table.indexOfFirst { it.id == id }
    if (at < 0) return emptyList()
    val others = table.indices.filter { it != at }.flatMapTo(HashSet()) { endpointsOf(table, it) }
    return endpointsOf(table, at).filter { it !in others }.distinct()
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
    parsed: List<ConfigHost>,
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

/** 机器列表一行的点与第二层尾字（那台状态成品的 `word`；空 ⇒ 不写）。 */
data class HostRowLook(
    val dot: MachineDot?,
    val word: String?,
)

/**
 * 核心交的那一行 ⇒ 行首那颗点与尾字：正在连 · 装 · 更新 ⇒ 空心点；通着 ⇒ 绿；那台旧了 ⇒ 琥珀（照常能用）；断着 · 做不了 ⇒ 红；
 * 核心还没交过（这台没起常驻流）⇒ 不画。只挑格，态与字都是成品的（`deploy::machine`，桌面同一份）。
 */
internal fun hostRowLook(line: LinkLine?): HostRowLook {
    val m = line?.machine ?: return HostRowLook(null, null)
    val dot =
        when (m.state) {
            MachineStateKind.CONNECTING, MachineStateKind.INSTALLING, MachineStateKind.UPDATING -> MachineDot.CONNECTING
            MachineStateKind.UP, MachineStateKind.NEWER -> MachineDot.UP
            MachineStateKind.NEEDS_UPDATE -> MachineDot.WARN
            MachineStateKind.DOWN, MachineStateKind.HOST_KEY_CHANGED, MachineStateKind.UNSUPPORTED, MachineStateKind.DISABLED -> MachineDot.FAILED
            MachineStateKind.UNKNOWN -> if (line is LinkLine.Connecting) MachineDot.CONNECTING else null
        }
    return HostRowLook(dot, m.word.ifEmpty { null })
}
