package com.ccmonitor.mobile.core.data.repo

import com.ccmonitor.mobile.core.data.crypto.CryptoBox
import com.ccmonitor.mobile.core.data.db.Bookmark
import com.ccmonitor.mobile.core.data.db.BookmarkDao
import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.data.db.HostDao
import com.ccmonitor.mobile.core.data.db.Identity
import com.ccmonitor.mobile.core.data.db.IdentityDao
import com.ccmonitor.mobile.core.data.db.Launcher
import com.ccmonitor.mobile.core.data.db.LauncherDao
import com.ccmonitor.mobile.core.data.db.Settings
import com.ccmonitor.mobile.core.data.db.SettingsDao
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map

class HostRepository(
    private val dao: HostDao,
    private val crypto: CryptoBox, // 加解密 passwordEnc，密码不明文落库
) {
    fun observeAll(): Flow<List<Host>> = dao.observeAll()

    suspend fun get(id: String): Host? = dao.get(id)

    suspend fun save(host: Host) = dao.upsert(host)

    /** 批量导入 ssh config 主机。 */
    suspend fun saveAll(hosts: List<Host>) = dao.upsertAll(hosts)

    suspend fun delete(host: Host) = dao.delete(host)

    suspend fun deleteById(id: String) = dao.deleteById(id)

    /** 把明文密码加密后写入 host.passwordEnc（Base64）；plain 空或 null 则清空（不用密码认证）。返回新 host，由调用方 save。 */
    fun withPassword(
        host: Host,
        plain: String?,
    ): Host =
        host.copy(
            passwordEnc =
                plain?.takeIf { it.isNotEmpty() }?.let {
                    java.util.Base64
                        .getEncoder()
                        .encodeToString(crypto.encrypt(it.toByteArray(Charsets.UTF_8)))
                },
        )

    /** 解密 host.passwordEnc 得明文密码；null 表示未设密码认证。 */
    fun password(host: Host): String? =
        host.passwordEnc?.let {
            String(
                crypto.decrypt(
                    java.util.Base64
                        .getDecoder()
                        .decode(it),
                ),
                Charsets.UTF_8,
            )
        }
}

/** SFTP 目录书签。 */
class BookmarkRepository(
    private val dao: BookmarkDao,
) {
    fun observeForHost(hostId: String): Flow<List<Bookmark>> = dao.observeForHost(hostId)

    suspend fun save(bookmark: Bookmark) = dao.upsert(bookmark)

    suspend fun delete(bookmark: Bookmark) = dao.delete(bookmark)

    suspend fun deleteById(id: String) = dao.deleteById(id)
}

/** 主机启动配置（cc/cct/进项目）。 */
class LauncherRepository(
    private val dao: LauncherDao,
) {
    fun observeForHost(hostId: String): Flow<List<Launcher>> = dao.observeForHost(hostId)

    fun observeAll(): Flow<List<Launcher>> = dao.observeAll()

    suspend fun get(id: String): Launcher? = dao.get(id)

    suspend fun save(launcher: Launcher) = dao.upsert(launcher)

    suspend fun delete(launcher: Launcher) = dao.delete(launcher)

    suspend fun deleteById(id: String) = dao.deleteById(id)
}

/** 应用级设置（键值），类型化访问。 */
class SettingsRepository(
    private val dao: SettingsDao,
) {
    /** 应用级默认 Claude 配置目录（null=未设，回退内置默认 [com.ccmonitor.mobile... ClaudePaths.DEFAULT_CLAUDE_DIR]）。 */
    fun defaultClaudeDir(): Flow<String?> = dao.observe(KEY_DEFAULT_CLAUDE_DIR)

    suspend fun getDefaultClaudeDir(): String? = dao.get(KEY_DEFAULT_CLAUDE_DIR)

    suspend fun setDefaultClaudeDir(value: String?) =
        dao.upsert(Settings(KEY_DEFAULT_CLAUDE_DIR, value?.trim()?.ifEmpty { null }))

    /** 历史页「自定义命令…」的上次输入（全局单键，只作弹窗预填；null 表示没用过）。 */
    fun resumeCustomCommand(): Flow<String?> = dao.observe(KEY_RESUME_CUSTOM_COMMAND)

    suspend fun setResumeCustomCommand(value: String?) =
        dao.upsert(Settings(KEY_RESUME_CUSTOM_COMMAND, value?.trim()?.ifEmpty { null }))

    /**
     * 新形态界面的总开关，默认开。
     *
     * 判据是 `!= FLAG_OFF`：`null` 或空串（没设过）是开，`"0"`（[FLAG_OFF]）是显式关，`"1"`（[FLAG_ON]）是显式开。
     * 写成 `== FLAG_ON` 会变成默认关；写成 `it != null` 会让空串变成关。
     * 注意：设置屏上没有关它的入口，[setNewUiEnabled] `(false)` 只能由代码调用。
     */
    fun newUiEnabled(): Flow<Boolean> = dao.observe(KEY_NEW_UI_ENABLED).map { it != FLAG_OFF }

    suspend fun setNewUiEnabled(enabled: Boolean) =
        dao.upsert(Settings(KEY_NEW_UI_ENABLED, if (enabled) FLAG_ON else FLAG_OFF))

    /**
     * 新对话用哪个权限模式（`claude --permission-mode`）。null 表示用 Claude 自己的默认。
     *
     * 这里只存字符串，合法性由用的地方（`ClaudeInvocation.permissionModeFlag` 的白名单）把关：
     * 脏值不会拼进命令行，而是 fail-closed 成不带这个参数。用的地方是必经之路，存的地方不是。
     */
    fun permissionMode(): Flow<String?> = dao.observe(KEY_PERMISSION_MODE)

    suspend fun setPermissionMode(value: String?) =
        dao.upsert(Settings(KEY_PERMISSION_MODE, value?.trim()?.ifEmpty { null }))

    /**
     * 这台主机上次进的是哪个对话。null 表示没进过。
     *
     * 落地路由要在导航那一刻同步定下来，去远端问一趟就成了先转圈再进屏，所以记在本地。
     * 它是导航提示，不是事实来源：这个编号在远端可能已经不存在，用它的地方必须容忍打开一个空对话。
     */
    suspend fun getLastConversation(hostId: String): String? = dao.get(keyLastConversation(hostId))

    suspend fun setLastConversation(
        hostId: String,
        sessionId: String?,
    ) = dao.upsert(Settings(keyLastConversation(hostId), sessionId?.trim()?.ifEmpty { null }))

    /**
     * 这台主机上被远端确认存在过的对话编号。null 表示一个都没有。
     *
     * 只有它认过的编号才允许 `--resume`：远端没有那条记录时 Claude 会当场退出
     * （`No conversation found with session ID: …`）。没认过的仍记进 [setLastConversation]，但按新对话起。
     * 注意：它只证明这个编号在远端跑起来过，不证明现在还在；那种情况仍会退化成一次失败的 resume。
     */
    suspend fun getConfirmedConversation(hostId: String): String? = dao.get(keyConfirmedConversation(hostId))

    suspend fun setConfirmedConversation(
        hostId: String,
        sessionId: String?,
    ) = dao.upsert(Settings(keyConfirmedConversation(hostId), sessionId?.trim()?.ifEmpty { null }))

    /**
     * 上次进的是哪台机器。null 表示从没进过。先由它定位机器，再由 [getLastConversation] 定位对话。
     * 同样是导航提示：那台机器可能已被删掉，用它的地方必须先核对主机还在。
     */
    suspend fun getLastHost(): String? = dao.get(KEY_LAST_HOST)

    suspend fun setLastHost(hostId: String?) = dao.upsert(Settings(KEY_LAST_HOST, hostId?.trim()?.ifEmpty { null }))

    companion object {
        const val KEY_DEFAULT_CLAUDE_DIR = "default_claude_dir"
        const val KEY_RESUME_CUSTOM_COMMAND = "resume_custom_command"

        /** 新形态界面总开关的存储键。 */
        const val KEY_NEW_UI_ENABLED = "new_ui_enabled"

        /** 开关的开值。`SettingsDao` 是字符串键值表，所以存字符串。 */
        const val FLAG_ON = "1"

        /** 开关的关值，只有它判成关。 */
        const val FLAG_OFF = "0"

        /** 新对话的权限模式。 */
        const val KEY_PERMISSION_MODE = "permission_mode"

        /** 上次进的那台机器（全局单键）。 */
        const val KEY_LAST_HOST = "last_host"

        /** 每台主机各存一条「上次进的对话」：两台机器上的对话互不相干，共用一个键会串号。 */
        fun keyLastConversation(hostId: String): String = "last_conversation:$hostId"

        /** 被远端确认存在过的那个编号，与 [keyLastConversation] 分开存。 */
        fun keyConfirmedConversation(hostId: String): String = "confirmed_conversation:$hostId"
    }
}

class IdentityRepository(
    private val dao: IdentityDao,
    private val crypto: CryptoBox,
) {
    fun observeAll(): Flow<List<Identity>> = dao.observeAll()

    suspend fun get(id: String): Identity? = dao.get(id)

    /** 存身份；plainPrivateKey 经 CryptoBox 加密后落库。 */
    suspend fun save(
        identity: Identity,
        plainPrivateKey: ByteArray,
    ) {
        dao.upsert(identity.copy(privateKeyEnc = crypto.encrypt(plainPrivateKey)))
    }

    /** 取解密后的私钥字节（经 Keystore，可能触发生物识别）。 */
    suspend fun privateKey(id: String): ByteArray? =
        dao.get(id)?.let { crypto.decrypt(it.privateKeyEnc) }

    /** 存 Keystore 背书身份（私钥不可导出、不落库；privateKeyEnc 空、keystoreAlias 非空）。 */
    suspend fun saveKeystoreBacked(identity: Identity) = dao.upsert(identity)

    suspend fun delete(identity: Identity) = dao.delete(identity)
}
