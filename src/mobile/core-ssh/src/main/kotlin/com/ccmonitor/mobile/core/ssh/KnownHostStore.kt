package com.ccmonitor.mobile.core.ssh

/**
 * TOFU 已知主机记录：core-ssh 自己的领域类型，不依赖 Room。
 * 字段与 app 侧 Room `KnownHost` 实体一一对应，app 侧适配器在两者之间转换。
 */
data class KnownHostRecord(
    val host: String,
    val port: Int,
    val keyType: String,
    val publicKeyBase64: String,
    val addedAt: Long,
)

/**
 * 已知主机存储端口：core-ssh 定义契约，app 侧用 Room 适配。
 * 适配器放 app：app 同时依赖 core-ssh 与 core-data，放 core-data 会和 core-ssh 对 core-data 的依赖成环。
 *
 * - [forHost] 在连接前预取快照：`TofuHostKeyVerifier.verify` 在 sshj 握手线程同步跑，不碰存储。
 * - [record] 在认证成功后才落首见的 key，免得未认证的端点往库里投毒；同 (host, port, keyType) 为 upsert。
 */
interface KnownHostStore {
    /** 查该 host:port 已钉扎的全部密钥（可能多类型：ed25519 / ecdsa …）。 */
    suspend fun forHost(
        host: String,
        port: Int,
    ): List<KnownHostRecord>

    /** 记录首见密钥（TOFU FirstUse）。同 (host,port,keyType) upsert。 */
    suspend fun record(knownHost: KnownHostRecord)

    /**
     * TOFU 恢复路径：忘记该 host:port 已钉扎的全部密钥，下次连接重新按首见接受。
     *
     * 服务器重装或轮换主机密钥会让指纹合法地变化，连接随之被拒；已知主机表与主机表之间没有级联，
     * 删掉服务器再加回来也会撞上同一条旧记录，所以需要这个出口。
     *
     * 注意：调用它等于放弃对该主机的信任锚点，只能在用户显式确认后调用，不能出现在任何自动重试或恢复路径里。
     */
    suspend fun forget(
        host: String,
        port: Int,
    )
}
