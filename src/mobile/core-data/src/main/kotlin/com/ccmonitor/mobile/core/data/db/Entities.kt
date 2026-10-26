package com.ccmonitor.mobile.core.data.db

import androidx.room.Entity
import androidx.room.PrimaryKey
import androidx.room.TypeConverter
import com.ccmonitor.mobile.core.rs.Core
import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.MachineRow
import com.ccmonitor.mobile.core.wire.gen.arr
import com.ccmonitor.mobile.core.wire.gen.Host as WireHost

/** 机器表与身份表的表名：两张表里的密文按「表名 · 行 id」绑在那一行（见 rowBinding）。 */
const val HOSTS_TABLE = "hosts"
const val IDENTITIES_TABLE = "identities"

/** SSH 主机：地址与认证、tmux/Claude 面、重连策略。那台是什么系统 · shell 方言 · 有没有 tmux 都问那台，不存。 */
@Entity(tableName = HOSTS_TABLE)
data class Host(
    @PrimaryKey val id: String,
    val label: String,
    val host: String,
    val port: Int = 22,
    val username: String,
    val authRef: String? = null, // FK → Identity.id（密钥认证）；它与 passwordEnc 都为 null 表示未绑身份
    // 备用地址（一行一条 `host` / `host:port` / `[v6]:port`，原样存）：怎么解析 · 怎么竞速都在拨号核心。
    val addresses: List<String> = emptyList(),
    val groupId: String? = null,
    val sortOrder: Int = 0,
    val lastConnected: Long? = null,
    // tmux / Claude 面
    val defaultWorkingDir: String? = null,
    val sessionManager: String? = null, // "tmux" | null
    // 断了要不要自己接回来（网络变了 · 重试几次不归这里：重连只有常驻流那一条循环）。
    val autoReconnect: Boolean = true,
    // 跳板：表里另一台的名字（同桌面那张表；空 = 直连）。多跳 · 成环 · 找不到怎么办都在拨号核心。
    val jump: String = "",
    // 密码认证：CryptoBox 加密后 Base64（TEXT，非明文）；非空 → 用密码认证（优先于 authRef）；null = 不用密码。
    val passwordEnc: String? = null,
)

/** 交拨号核心的那一形（`ssh_dial::machine::Host`）：`keyPath` 写这一台的 id —— 核心组好的请求里每一跳带着它，认证照它解。 */
fun Host.wire(): WireHost =
    WireHost(
        label = label,
        host = host,
        port = port.toLong(),
        user = username,
        keyPath = id,
        addresses = addresses,
        jump = jump,
        connect = true,
    )

/** 这一端的机器表交核心的那一份（JSON 数组，一台一个 [wire]）。 */
fun List<Host>.machineTable(): String = arr(WireHost.ADAPTER).toJson(map { it.wire() })

/** 每台的名字（拨号核心算的，同桌面那张表；跳板指的就是它），与 [hosts] 一一对应。要那份 .so。 */
fun machineNames(hosts: List<Host>): List<String> =
    (Wire.read(arr(MachineRow.ADAPTER), Core.machines(hosts.machineTable())) ?: error("wire contract mismatch: machine rows"))
        .map { it.name }

/** 备用地址那一格在库里一行一条（原样存，不解析）。 */
class HostConverters {
    @TypeConverter
    fun linesToText(lines: List<String>): String = lines.joinToString("\n")

    @TypeConverter
    fun textToLines(text: String): List<String> = text.lines().filter { it.isNotBlank() }
}

/**
 * SSH 身份/密钥。privateKeyEnc 为 CryptoBox 加密后的字节。
 * keystoreAlias 非空表示私钥不可导出（存于 Keystore TEE），此时 privateKeyEnc 可空。
 */
@Entity(tableName = IDENTITIES_TABLE)
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

/** 旧表的一行（主机钥匙原先住这里；迁进核心那份库之后清空，下一版删表）。 */
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
