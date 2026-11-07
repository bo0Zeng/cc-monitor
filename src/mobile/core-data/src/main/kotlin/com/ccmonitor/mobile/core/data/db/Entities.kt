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

/** SSH 主机：地址与认证、连上进哪个目录、重连策略。那台是什么系统 · shell 方言 · 有没有 tmux · 会话放在哪都问那台，不存。 */
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
    // 连上进哪个目录（空 ⇒ 不进）
    val defaultWorkingDir: String? = null,
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

/** 应用级设置（键值表，增键不用迁移）。类型化访问经 SettingsRepository。 */
@Entity(tableName = "settings")
data class Settings(
    @PrimaryKey val key: String,
    val value: String? = null,
)

/**
 * 待发：投进会话输入队（`session-queue-put`）的一句，会话那台还没回执、或回执到了还没在记录里出现、或那台拒了。
 * [id] 就是投队那一下交的编号：没回执 ⇒ 接上照同一个编号再交（那台去重只进一条）。[outcome] 是 [PendingOutcome] 那几种。
 */
@Entity(tableName = "pending")
data class PendingSend(
    /** 本地铸的 id。 */
    @PrimaryKey val id: String,
    val hostId: String,
    /** 哪条会话；新建会话（还没起成）⇒ `null`。 */
    val sid: String?,
    val text: String,
    val mintedAt: Long,
    val outcome: String,
    val attempts: Int,
)

/** [PendingSend.outcome] 那几种（落盘的字，不上屏）。 */
object PendingOutcome {
    /** 交出去了、会话那台的回执还没到：接上照同一个编号再交。 */
    const val PENDING = "pending"

    /** 回执到了（会话那台已落盘、排进队）：等它在记录里出现。 */
    const val DELIVERED = "delivered"

    /** 新建会话随起交的头一句，那台说不知道送没送到（`session-open` 回包）；挂回来照「没回执」投进队再交，那台按编号去重。 */
    const val UNSURE = "unsure"

    /** 会话那台拒了（队满 · 太长 · 附件已不在）：不再交，字留着可改了再送。 */
    const val REFUSED = "refused"
}

/** 在做的一组：核心交的那一串原样（票 · 发出去的那一份入参 · 断开时的完成数）；开始时写、收场删，进程没了再开照它带票再问。 */
@Entity(tableName = "pending_batches", primaryKeys = ["hostId", "ticket"])
data class PendingBatch(
    val hostId: String,
    val ticket: String,
    val plan: String,
)

/** 草稿：一条会话（新建会话 [sid] 为空串）输入框里还没发的字。 */
@Entity(tableName = "drafts", primaryKeys = ["hostId", "sid"])
data class Draft(
    val hostId: String,
    val sid: String,
    val text: String,
    val updatedAt: Long,
) {
    companion object {
        /** 新建会话那一张的 sid（主键不许空，空串代「还没有」）。 */
        const val NEW: String = ""
    }
}
