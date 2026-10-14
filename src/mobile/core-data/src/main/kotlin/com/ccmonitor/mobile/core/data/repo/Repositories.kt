package com.ccmonitor.mobile.core.data.repo

import com.ccmonitor.mobile.core.data.crypto.CryptoBox
import com.ccmonitor.mobile.core.data.db.Bookmark
import com.ccmonitor.mobile.core.data.db.BookmarkDao
import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.data.db.HostDao
import com.ccmonitor.mobile.core.data.db.Identity
import com.ccmonitor.mobile.core.data.db.IdentityDao
import com.ccmonitor.mobile.core.data.db.Settings
import com.ccmonitor.mobile.core.data.db.SettingsDao
import kotlinx.coroutines.flow.Flow

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

/** 应用级设置（键值），类型化访问。 */
class SettingsRepository(
    private val dao: SettingsDao,
) {
    /**
     * 上次进的是哪台机器。null 表示从没进过。打开 app 落在它上空的新建会话（不记上次那条）。
     * 同样是导航提示：那台机器可能已被删掉，用它的地方必须先核对主机还在。
     */
    suspend fun getLastHost(): String? = dao.get(KEY_LAST_HOST)

    suspend fun setLastHost(hostId: String?) = dao.upsert(Settings(KEY_LAST_HOST, hostId?.trim()?.ifEmpty { null }))

    companion object {
        /** 上次进的那台机器（全局单键）。 */
        const val KEY_LAST_HOST = "last_host"
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
