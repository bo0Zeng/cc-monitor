package com.ccmonitor.mobile.core.data.db

import androidx.room.Entity
import androidx.room.PrimaryKey

/** SSH 主机：地址与认证、tmux/Claude 面、重连策略。那台是什么系统 · shell 方言 · 有没有 tmux 都问那台，不存。 */
@Entity(tableName = "hosts")
data class Host(
    @PrimaryKey val id: String,
    val label: String,
    val host: String,
    val port: Int = 22,
    val username: String,
    val authRef: String? = null, // FK → Identity.id（密钥认证）；它与 passwordEnc 都为 null 表示未绑身份
    // 额外地址（换行分隔 `host[:port]`），连接时与主地址并发竞速、先连通者胜。
    val extraAddresses: String? = null,
    val groupId: String? = null,
    val sortOrder: Int = 0,
    val lastConnected: Long? = null,
    // tmux / Claude 面
    val defaultWorkingDir: String? = null,
    val sessionManager: String? = null, // "tmux" | null
    // 断了要不要自己接回来（网络变了 · 重试几次不归这里：重连只有常驻流那一条循环）。
    val autoReconnect: Boolean = true,
    // 经跳板机连接，指向另一个 Host 的 id（ssh -J A B）；null = 直连。
    val proxyJumpHostId: String? = null,
    // 密码认证：CryptoBox 加密后 Base64（TEXT，非明文）；非空 → 用密码认证（优先于 authRef）；null = 不用密码。
    val passwordEnc: String? = null,
    // 这台起会话用哪一家。旧值（`ClaudeCode` / `Codex`）不迁：那台认不出 ⇒ 当没选。
    // 存那台 `session-new-facts` 给的线上 kind 原值（主机设置里选的那一家）；没选 ⇒ null。
    val agentKind: String? = null,
)

/**
 * SSH 身份/密钥。privateKeyEnc 为 CryptoBox 加密后的字节。
 * keystoreAlias 非空表示私钥不可导出（存于 Keystore TEE），此时 privateKeyEnc 可空。
 */
@Entity(tableName = "identities")
data class Identity(
    @PrimaryKey val id: String,
    val label: String,
    val keyType: String, // "ecdsa-p256" | "ed25519" | "rsa" | "imported"
    val privateKeyEnc: ByteArray,
    val publicKeyOpenSsh: String,
    val fingerprintSha256: String,
    val createdAt: Long,
    val keystoreAlias: String? = null,
) {
    // ByteArray 字段需手写 equals/hashCode（data class 默认按引用比较）
    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other !is Identity) return false
        return id == other.id &&
            label == other.label &&
            keyType == other.keyType &&
            privateKeyEnc.contentEquals(other.privateKeyEnc) &&
            publicKeyOpenSsh == other.publicKeyOpenSsh &&
            fingerprintSha256 == other.fingerprintSha256 &&
            createdAt == other.createdAt &&
            keystoreAlias == other.keystoreAlias
    }

    override fun hashCode(): Int {
        var r = id.hashCode()
        r = 31 * r + privateKeyEnc.contentHashCode()
        r = 31 * r + fingerprintSha256.hashCode()
        return r
    }
}

/**
 * 是否硬件 Keystore 背书的不可导出密钥（私钥在 TEE、privateKeyEnc 空）。
 * 连接时据此走 KeystoreSigner，而不去解密那串空字节。
 */
fun Identity.isKeystoreBacked(): Boolean = keystoreAlias != null

/** TOFU 已知主机公钥（首次连接记录，后续校验）。 */
@Entity(tableName = "known_hosts", primaryKeys = ["host", "port", "keyType"])
data class KnownHost(
    val host: String,
    val port: Int,
    val keyType: String,
    val publicKeyBase64: String,
    val addedAt: Long,
)

/** SFTP 目录书签 / Pin。收藏常用远端目录，一键跳转 / 在此打开终端。 */
@Entity(tableName = "bookmarks")
data class Bookmark(
    @PrimaryKey val id: String,
    val hostId: String,
    val path: String,
    val label: String,
    val sortOrder: Int = 0,
)

/** 应用级设置（键值表，增键不用迁移）。类型化访问经 SettingsRepository。 */
@Entity(tableName = "settings")
data class Settings(
    @PrimaryKey val key: String,
    val value: String? = null,
)
