package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.data.db.KnownHost
import com.ccmonitor.mobile.core.data.db.KnownHostDao
import com.ccmonitor.mobile.core.ssh.KnownHostRecord
import com.ccmonitor.mobile.core.ssh.KnownHostStore

/**
 * core-ssh 的 [KnownHostStore] 端口接到 Room [KnownHostDao]。
 *
 * 放在 app 里：只有 app 同时依赖 core-ssh 和 core-data；放进 core-data 会和 core-ssh 对 `SshWire` 的依赖成环。
 * 只做两种类型互转，语义由 DAO 保证。
 */
class KnownHostDaoStore(
    private val dao: KnownHostDao,
) : KnownHostStore {
    override suspend fun forHost(
        host: String,
        port: Int,
    ): List<KnownHostRecord> = dao.forHost(host, port).map { it.toRecord() }

    override suspend fun record(knownHost: KnownHostRecord) = dao.upsert(knownHost.toEntity())

    /** TOFU 恢复路径：忘掉这台主机记下的指纹。 */
    override suspend fun forget(
        host: String,
        port: Int,
    ) = dao.forget(host, port)
}

// 用具名参数：三个 String 字段（host/keyType/publicKeyBase64）按位置写时换位不报错。
private fun KnownHost.toRecord() =
    KnownHostRecord(host = host, port = port, keyType = keyType, publicKeyBase64 = publicKeyBase64, addedAt = addedAt)

private fun KnownHostRecord.toEntity() =
    KnownHost(host = host, port = port, keyType = keyType, publicKeyBase64 = publicKeyBase64, addedAt = addedAt)
