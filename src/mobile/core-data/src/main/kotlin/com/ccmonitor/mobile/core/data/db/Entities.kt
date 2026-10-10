package com.ccmonitor.mobile.core.data.db

import androidx.room.ColumnInfo
import androidx.room.Entity
import androidx.room.PrimaryKey

/** SSH 主机：地址与认证、tmux/Claude 面、重连策略。 */
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
    // 重连策略
    val autoReconnect: Boolean = true,
    val reconnectOnNetworkChange: Boolean = true,
    val maxReconnectAttempts: Int = 0, // 0 = 无限
    // 每主机 Claude 配置目录（null/空 → 默认 ${CLAUDE_CONFIG_DIR:-$HOME/.claude}，远端 shell 就地探测）
    val claudeDir: String? = null,
    // 经跳板机连接，指向另一个 Host 的 id（ssh -J A B）；null = 直连。
    val proxyJumpHostId: String? = null,
    // 密码认证：CryptoBox 加密后 Base64（TEXT，非明文）；非空 → 用密码认证（优先于 authRef）；null = 不用密码。
    val passwordEnc: String? = null,
    // 远端 OS：null/"posix" = POSIX（bash/sh+tmux，默认）；"windows" = Windows（PowerShell，无 tmux）。命令按 os 走方言。
    val os: String? = null,
    // agent 种类：null/"ClaudeCode" = Claude Code（默认）；"Codex" = Codex CLI。
    // core-data 不依赖 core-claude，所以存 enum.name 字符串，在 :app 边界映射为 AgentKind。
    // claudeDir 按 kind 解释为 agent 配置目录覆盖（~/.claude 或 ~/.codex）。
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

/** 自定义按钮 / 一键命令。hostId 为 null 表示全局按钮。 */
@Entity(tableName = "custom_buttons")
data class CustomButton(
    @PrimaryKey val id: String,
    val hostId: String? = null,
    val label: String,
    val command: String,
    val sortOrder: Int = 0,
    // 组合键修饰：任一为真时按钮发按键码（command 当基准键），否则发命令行文本（command+回车）。
    // defaultValue 与迁移的 `ADD COLUMN ... NOT NULL DEFAULT 0` 逐字对齐，schema 校验才过。
    @ColumnInfo(defaultValue = "0") val ctrl: Boolean = false,
    @ColumnInfo(defaultValue = "0") val alt: Boolean = false,
    @ColumnInfo(defaultValue = "0") val shift: Boolean = false,
    // 按钮类型：command=命令行 / keycode=按键码含修饰 / escape=转义序列 / builtin=内建动作 / switch=组切换。
    // command 列按 type 解释为载荷（switch 时是目标 btnGroup，点击切换活动键排，不发字节）。
    // 注意：defaultValue 要带单引号 `'command'`，Room 不会自动加；迁移 SQL 同写 `DEFAULT 'command'`。
    @ColumnInfo(defaultValue = "'command'") val type: String = "command",
    // 按键分组，决定渲染归属：`main`=用户按钮 + 顶部工具条内建（CustomButtonBar）；
    // `fn`/`alt`/`vim`=键排的组（ExtraKeysRow 渲染当前活动组，⇌ 按钮在组间切）。各组都可编辑。
    // 列名用 btnGroup：group 是 SQL 保留字。
    @ColumnInfo(defaultValue = "'main'") val btnGroup: String = "main",
    // 每按钮长按：[longPressMode] none=无 / repeat=按住时每 [longPressRepeatMs] 重发 tap 命令 /
    // discrete=按住到 [longPressThresholdMs] 发一次 [longPressCommand]（按本按钮 [type] 解释，如 escape 型 "\e\e"=两下 Esc）。
    // 阈值与速率单位 ms。@ColumnInfo 的 defaultValue 与迁移 SQL 的 DEFAULT 逐字对齐。
    @ColumnInfo(defaultValue = "'none'") val longPressMode: String = "none",
    @ColumnInfo(defaultValue = "''") val longPressCommand: String = "",
    @ColumnInfo(defaultValue = "400") val longPressThresholdMs: Int = 400,
    @ColumnInfo(defaultValue = "60") val longPressRepeatMs: Int = 60,
)

/** [CustomButton.longPressMode] 的取值（自由 TEXT，未知值降级 NONE）。 */
enum class LongPressMode {
    NONE,
    REPEAT,
    DISCRETE,
    ;

    companion object {
        fun fromDb(value: String): LongPressMode = entries.firstOrNull { it.name.equals(value, ignoreCase = true) } ?: NONE
    }
}

/**
 * [CustomButton.btnGroup] 的默认组名。写裸字符串拼错一个字母，那一排就静默变空，所以集中在这里。
 *
 * btnGroup 是自由字符串：用户可自建任意组，用 [CustomButtonType.SWITCH] 按钮在组间切换。
 * 这些常量只是种子与默认渲染的起点，渲染与编辑不限于这几组。
 */
object ButtonGroups {
    /** 用户按钮 + 顶部工具条内建 —— `CustomButtonBar` 与命令面板渲染。 */
    const val MAIN = "main"

    /** 功能键排（Esc/Tab/方向/Home/End）—— `ExtraKeysRow` 非 vim 态渲染。 */
    const val FN = "fn"

    /** vim 键排 —— 经 `⇌` 组切换按钮到达。 */
    const val VIM = "vim"

    /** 第二排（Alt/翻页/Ctrl+End 等）—— fn 组的 `⇌` 切到此，此组 `⇌` 再切到 vim（fn→alt→vim→fn 循环）。 */
    const val ALT = "alt"
}

/** [CustomButton.type] 的解释（DB 存自由小写字符串，未知值降级 COMMAND）。 */
enum class CustomButtonType {
    COMMAND,
    KEYCODE,
    ESCAPE,
    BUILTIN,

    /** 组切换按钮：[CustomButton.command] 是目标 btnGroup，点击把活动键排切到该组，不发字节。 */
    SWITCH,
    ;

    companion object {
        fun fromDb(value: String): CustomButtonType = entries.firstOrNull { it.name.equals(value, ignoreCase = true) } ?: COMMAND
    }
}

/** SFTP 目录书签 / Pin。收藏常用远端目录，一键跳转 / 在此打开终端。 */
@Entity(tableName = "bookmarks")
data class Bookmark(
    @PrimaryKey val id: String,
    val hostId: String,
    val path: String,
    val label: String,
    val sortOrder: Int = 0,
)

/**
 * 启动配置：把「连上后手拼 cd/tmux/命令」存成一键可复用的项（cc/cct/进项目）。
 * 经 [com.ccmonitor.mobile.ssh.LaunchSpec] 消费：tmuxSession 非空→`tmux new -A -s <name>`；command 非空→进入后跑（如 claude）。
 */
@Entity(tableName = "launchers")
data class Launcher(
    @PrimaryKey val id: String,
    val hostId: String,
    val label: String,
    val workingDir: String? = null,
    val tmuxSession: String? = null,
    val command: String? = null,
    val sortOrder: Int = 0,
    val pinned: Boolean = false,
)

/** 应用级设置（键值表，增键不用迁移）。类型化访问经 SettingsRepository。 */
@Entity(tableName = "settings")
data class Settings(
    @PrimaryKey val key: String,
    val value: String? = null,
)
