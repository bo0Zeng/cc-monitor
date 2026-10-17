package com.ccmonitor.mobile.core.data.db

import android.view.ViewConfiguration
import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase
import com.ccmonitor.mobile.core.data.crypto.CryptoBox
import com.ccmonitor.mobile.core.data.crypto.rowBinding
import java.security.GeneralSecurityException
import java.util.Base64

/*
 * Room 迁移链，升级不清库。
 * DDL 与 core-data/schemas 下导出的 json createSql 严格对齐（MigrationTest 用 MigrationTestHelper 校验）。
 */

/** hosts 加可空 `extraAddresses`（多地址竞速）。 */
val MIGRATION_1_2 =
    object : Migration(1, 2) {
        override fun migrate(db: SupportSQLiteDatabase) {
            db.execSQL("ALTER TABLE `hosts` ADD COLUMN `extraAddresses` TEXT")
        }
    }

/** 新增 `bookmarks` 表（SFTP 目录书签）。 */
val MIGRATION_2_3 =
    object : Migration(2, 3) {
        override fun migrate(db: SupportSQLiteDatabase) {
            db.execSQL(
                "CREATE TABLE IF NOT EXISTS `bookmarks` (" +
                    "`id` TEXT NOT NULL, `hostId` TEXT NOT NULL, `path` TEXT NOT NULL, " +
                    "`label` TEXT NOT NULL, `sortOrder` INTEGER NOT NULL, PRIMARY KEY(`id`))",
            )
        }
    }

/** 新增 `launchers` 表（启动配置 cc/cct/进项目）。DDL 与 Room 生成对齐：可空 → TEXT、Int/Boolean → INTEGER NOT NULL。 */
val MIGRATION_3_4 =
    object : Migration(3, 4) {
        override fun migrate(db: SupportSQLiteDatabase) {
            db.execSQL(
                "CREATE TABLE IF NOT EXISTS `launchers` (" +
                    "`id` TEXT NOT NULL, `hostId` TEXT NOT NULL, `label` TEXT NOT NULL, " +
                    "`workingDir` TEXT, `tmuxSession` TEXT, `command` TEXT, " +
                    "`sortOrder` INTEGER NOT NULL, `pinned` INTEGER NOT NULL, PRIMARY KEY(`id`))",
            )
        }
    }

/** hosts 加 `claudeDir`（每主机 Claude 配置目录）+ 新增 `settings` 键值表。 */
val MIGRATION_4_5 =
    object : Migration(4, 5) {
        override fun migrate(db: SupportSQLiteDatabase) {
            db.execSQL("ALTER TABLE `hosts` ADD COLUMN `claudeDir` TEXT")
            db.execSQL(
                "CREATE TABLE IF NOT EXISTS `settings` (" +
                    "`key` TEXT NOT NULL, `value` TEXT, PRIMARY KEY(`key`))",
            )
        }
    }

/** hosts 加 `proxyJumpHostId`（经跳板机连接，指向另一 Host id；null=直连）。 */
val MIGRATION_5_6 =
    object : Migration(5, 6) {
        override fun migrate(db: SupportSQLiteDatabase) {
            db.execSQL("ALTER TABLE `hosts` ADD COLUMN `proxyJumpHostId` TEXT")
        }
    }

// hosts 加 passwordEnc（加密后的密码 Base64，TEXT）。
val MIGRATION_6_7 =
    object : Migration(6, 7) {
        override fun migrate(db: SupportSQLiteDatabase) {
            db.execSQL("ALTER TABLE `hosts` ADD COLUMN `passwordEnc` TEXT")
        }
    }

// 去掉 hosts.colorScheme、identities.biometricProtected 两列，并删 claude_bindings 表。
// SQLite DROP COLUMN 要 3.35+（Android 34+），所以走重建：建新表 → 拷保留列 → DROP → RENAME。
// 新表 CREATE 必须与 Room v8 期望 schema 逐字一致（列序/类型/NOT NULL/主键），MigrationTest 全链比对。
val MIGRATION_7_8 =
    object : Migration(7, 8) {
        override fun migrate(db: SupportSQLiteDatabase) {
            // hosts：去掉 colorScheme
            db.execSQL(
                "CREATE TABLE `hosts_new` (`id` TEXT NOT NULL, `label` TEXT NOT NULL, `host` TEXT NOT NULL, " +
                    "`port` INTEGER NOT NULL, `username` TEXT NOT NULL, `authRef` TEXT, `extraAddresses` TEXT, " +
                    "`groupId` TEXT, `sortOrder` INTEGER NOT NULL, `lastConnected` INTEGER, `defaultWorkingDir` TEXT, " +
                    "`sessionManager` TEXT, `autoReconnect` INTEGER NOT NULL, `reconnectOnNetworkChange` INTEGER NOT NULL, " +
                    "`maxReconnectAttempts` INTEGER NOT NULL, `claudeDir` TEXT, `proxyJumpHostId` TEXT, `passwordEnc` TEXT, " +
                    "PRIMARY KEY(`id`))",
            )
            db.execSQL(
                "INSERT INTO `hosts_new` (id,label,host,port,username,authRef,extraAddresses,groupId,sortOrder," +
                    "lastConnected,defaultWorkingDir,sessionManager,autoReconnect,reconnectOnNetworkChange," +
                    "maxReconnectAttempts,claudeDir,proxyJumpHostId,passwordEnc) " +
                    "SELECT id,label,host,port,username,authRef,extraAddresses,groupId,sortOrder,lastConnected," +
                    "defaultWorkingDir,sessionManager,autoReconnect,reconnectOnNetworkChange,maxReconnectAttempts," +
                    "claudeDir,proxyJumpHostId,passwordEnc FROM `hosts`",
            )
            db.execSQL("DROP TABLE `hosts`")
            db.execSQL("ALTER TABLE `hosts_new` RENAME TO `hosts`")
            // identities：去掉 biometricProtected
            db.execSQL(
                "CREATE TABLE `identities_new` (`id` TEXT NOT NULL, `label` TEXT NOT NULL, `keyType` TEXT NOT NULL, " +
                    "`privateKeyEnc` BLOB NOT NULL, `publicKeyOpenSsh` TEXT NOT NULL, `fingerprintSha256` TEXT NOT NULL, " +
                    "`createdAt` INTEGER NOT NULL, `keystoreAlias` TEXT, PRIMARY KEY(`id`))",
            )
            db.execSQL(
                "INSERT INTO `identities_new` (id,label,keyType,privateKeyEnc,publicKeyOpenSsh,fingerprintSha256," +
                    "createdAt,keystoreAlias) SELECT id,label,keyType,privateKeyEnc,publicKeyOpenSsh,fingerprintSha256," +
                    "createdAt,keystoreAlias FROM `identities`",
            )
            db.execSQL("DROP TABLE `identities`")
            db.execSQL("ALTER TABLE `identities_new` RENAME TO `identities`")
            // claude_bindings：没有消费方的空表，直接 DROP
            db.execSQL("DROP TABLE IF EXISTS `claude_bindings`")
        }
    }

// hosts 加 os（"windows" 走 PowerShell 方言、跳过 tmux；null/"posix"=默认）。
val MIGRATION_8_9 =
    object : Migration(8, 9) {
        override fun migrate(db: SupportSQLiteDatabase) {
            db.execSQL("ALTER TABLE `hosts` ADD COLUMN `os` TEXT")
        }
    }

// custom_buttons 加组合键 ctrl/alt/shift（Boolean → INTEGER NOT NULL DEFAULT 0）。
// NOT NULL ADD COLUMN 须带 DEFAULT 给存量行填值；实体侧 @ColumnInfo(defaultValue="0") 与此对齐，MigrationTest 校验。
val MIGRATION_9_10 =
    object : Migration(9, 10) {
        override fun migrate(db: SupportSQLiteDatabase) {
            db.execSQL("ALTER TABLE `custom_buttons` ADD COLUMN `ctrl` INTEGER NOT NULL DEFAULT 0")
            db.execSQL("ALTER TABLE `custom_buttons` ADD COLUMN `alt` INTEGER NOT NULL DEFAULT 0")
            db.execSQL("ALTER TABLE `custom_buttons` ADD COLUMN `shift` INTEGER NOT NULL DEFAULT 0")
        }
    }

// custom_buttons 加 type（command/keycode/escape/builtin）并回填：
// v10 的行按「有修饰=keycode、无修饰=command」解释，UPDATE 把有修饰的行设为 keycode，派发逐字不变
// （Ctrl+C 行仍发 0x03、无修饰行仍发 command\n）。DEFAULT 带单引号 'command'，与实体 @ColumnInfo 对齐。
val MIGRATION_10_11 =
    object : Migration(10, 11) {
        override fun migrate(db: SupportSQLiteDatabase) {
            db.execSQL("ALTER TABLE `custom_buttons` ADD COLUMN `type` TEXT NOT NULL DEFAULT 'command'")
            db.execSQL("UPDATE `custom_buttons` SET `type` = 'keycode' WHERE `ctrl` = 1 OR `alt` = 1 OR `shift` = 1")
        }
    }

// hosts 加 agentKind（null/"ClaudeCode"=Claude Code、"Codex"=Codex CLI）。可空 TEXT，不需 DEFAULT 或回填。
val MIGRATION_11_12 =
    object : Migration(11, 12) {
        override fun migrate(db: SupportSQLiteDatabase) {
            db.execSQL("ALTER TABLE `hosts` ADD COLUMN `agentKind` TEXT")
        }
    }

// custom_buttons 加 btnGroup（按键分组 main/fn/vim，功能键/vim/工具条成为可编辑行）并种默认键。
// 列名不用 group（SQL 保留字）。存量行取 DEFAULT 'main'。
// 种子是当年那一份旧默认键（seedLegacyButtons，原样留着）；INSERT OR IGNORE 固定 id，不碰已有行。
val MIGRATION_12_13 =
    object : Migration(12, 13) {
        override fun migrate(db: SupportSQLiteDatabase) {
            db.execSQL("ALTER TABLE `custom_buttons` ADD COLUMN `btnGroup` TEXT NOT NULL DEFAULT 'main'")
            seedLegacyButtons(db)
        }
    }

// custom_buttons 加长按 4 列（longPressMode/Command/ThresholdMs/RepeatMs），DEFAULT 与实体 @ColumnInfo 逐字对齐；
// 存量行的长按默认 none。
// 末尾 seedLegacyButtons 补种当年的新默认键（组切换 `⇌` / Ctrl·Alt·粘贴 builtin / alt 组）：INSERT OR IGNORE 固定 id，
// 不覆盖用户改过或自建的按钮。
// 先加列后种子，新种子行的长按列取列默认。
val MIGRATION_13_14 =
    object : Migration(13, 14) {
        override fun migrate(db: SupportSQLiteDatabase) {
            db.execSQL("ALTER TABLE `custom_buttons` ADD COLUMN `longPressMode` TEXT NOT NULL DEFAULT 'none'")
            db.execSQL("ALTER TABLE `custom_buttons` ADD COLUMN `longPressCommand` TEXT NOT NULL DEFAULT ''")
            db.execSQL("ALTER TABLE `custom_buttons` ADD COLUMN `longPressThresholdMs` INTEGER NOT NULL DEFAULT 400")
            db.execSQL("ALTER TABLE `custom_buttons` ADD COLUMN `longPressRepeatMs` INTEGER NOT NULL DEFAULT 60")
            seedLegacyButtons(db)
        }
    }

// hosts 去掉 claudeDir（每主机 agent 配置目录）：找会话 · 读记录都归那台核心，手机不再拼远端路径，这一格没人读了。
// SQLite 老版本没有 DROP COLUMN ⇒ 建新表、拷数据、换名（同 7→8）。旧值直接丢掉，不留兼容。
val MIGRATION_14_15 =
    object : Migration(14, 15) {
        override fun migrate(db: SupportSQLiteDatabase) {
            db.execSQL(
                "CREATE TABLE `hosts_new` (`id` TEXT NOT NULL, `label` TEXT NOT NULL, `host` TEXT NOT NULL, " +
                    "`port` INTEGER NOT NULL, `username` TEXT NOT NULL, `authRef` TEXT, `extraAddresses` TEXT, " +
                    "`groupId` TEXT, `sortOrder` INTEGER NOT NULL, `lastConnected` INTEGER, `defaultWorkingDir` TEXT, " +
                    "`sessionManager` TEXT, `autoReconnect` INTEGER NOT NULL, `reconnectOnNetworkChange` INTEGER NOT NULL, " +
                    "`maxReconnectAttempts` INTEGER NOT NULL, `proxyJumpHostId` TEXT, `passwordEnc` TEXT, `os` TEXT, " +
                    "`agentKind` TEXT, PRIMARY KEY(`id`))",
            )
            db.execSQL(
                "INSERT INTO `hosts_new` (id,label,host,port,username,authRef,extraAddresses,groupId,sortOrder,lastConnected,defaultWorkingDir,sessionManager,autoReconnect,reconnectOnNetworkChange,maxReconnectAttempts,proxyJumpHostId,passwordEnc,os,agentKind) SELECT id,label,host,port,username,authRef,extraAddresses,groupId,sortOrder,lastConnected,defaultWorkingDir,sessionManager,autoReconnect,reconnectOnNetworkChange,maxReconnectAttempts,proxyJumpHostId,passwordEnc,os,agentKind FROM `hosts`",
            )
            db.execSQL("DROP TABLE `hosts`")
            db.execSQL("ALTER TABLE `hosts_new` RENAME TO `hosts`")
        }
    }

// 删 `launchers` 表（手机自己存的启动配置）：起会话改用那台的别名。旧行在删表之前原样写进设置表的迁移说明那一格
// （`MovedLaunchRepository`：一条一段、一段五格），每台连上终端时列一次、可复制，「知道了」之后就没了。不留旧表。
// char(30) / char(31) 与 `MovedLaunchRepository.RECORD` / `.FIELD` 是同一对分隔符；一行都没有 ⇒ 那一格为 NULL（没有可说的）。
val MIGRATION_15_16 =
    object : Migration(15, 16) {
        override fun migrate(db: SupportSQLiteDatabase) {
            db.execSQL(
                "INSERT OR REPLACE INTO `settings` (`key`, `value`) SELECT 'launch_moved', group_concat(" +
                    "hostId || char(31) || label || char(31) || IFNULL(workingDir, '') || char(31) || " +
                    "IFNULL(tmuxSession, '') || char(31) || IFNULL(command, ''), char(30)) " +
                    "FROM (SELECT * FROM `launchers` ORDER BY hostId, sortOrder, label)",
            )
            db.execSQL("DROP TABLE `launchers`")
        }
    }

// 加本地缓存五张表（会话列表 · 记录页 · 续点 · 待发 · 草稿：都装那台核心的成品原样）；hosts 去掉「网络变化时重连」「最多重连几次」两列
// （重连只剩常驻流那一条循环，这两格没人读了）。别的表一字不动。
// hosts 照 7→8 的做法重建（老 SQLite 没有 DROP COLUMN）：建新表、拷其余各格、换名；那两列的值丢掉，不留兼容。
val MIGRATION_16_17 =
    object : Migration(16, 17) {
        override fun migrate(db: SupportSQLiteDatabase) {
            db.execSQL(
                "CREATE TABLE `hosts_new` (`id` TEXT NOT NULL, `label` TEXT NOT NULL, `host` TEXT NOT NULL, " +
                    "`port` INTEGER NOT NULL, `username` TEXT NOT NULL, `authRef` TEXT, `extraAddresses` TEXT, " +
                    "`groupId` TEXT, `sortOrder` INTEGER NOT NULL, `lastConnected` INTEGER, `defaultWorkingDir` TEXT, " +
                    "`sessionManager` TEXT, `autoReconnect` INTEGER NOT NULL, `proxyJumpHostId` TEXT, `passwordEnc` TEXT, " +
                    "`os` TEXT, `agentKind` TEXT, PRIMARY KEY(`id`))",
            )
            db.execSQL(
                "INSERT INTO `hosts_new` (id,label,host,port,username,authRef,extraAddresses,groupId,sortOrder,lastConnected," +
                    "defaultWorkingDir,sessionManager,autoReconnect,proxyJumpHostId,passwordEnc,os,agentKind) " +
                    "SELECT id,label,host,port,username,authRef,extraAddresses,groupId,sortOrder,lastConnected," +
                    "defaultWorkingDir,sessionManager,autoReconnect,proxyJumpHostId,passwordEnc,os,agentKind FROM `hosts`",
            )
            db.execSQL("DROP TABLE `hosts`")
            db.execSQL("ALTER TABLE `hosts_new` RENAME TO `hosts`")
            db.execSQL(
                "CREATE TABLE IF NOT EXISTS `sessions` (`hostId` TEXT NOT NULL, `sid` TEXT NOT NULL, `listRow` TEXT, " +
                    "`ord` INTEGER, `viewedAt` INTEGER, PRIMARY KEY(`hostId`, `sid`))",
            )
            db.execSQL(
                "CREATE TABLE IF NOT EXISTS `record_pages` (`hostId` TEXT NOT NULL, `sid` TEXT NOT NULL, `start` INTEGER NOT NULL, " +
                    "`end` INTEGER NOT NULL, `rows` TEXT NOT NULL, `fetchedAt` INTEGER NOT NULL, PRIMARY KEY(`hostId`, `sid`, `start`))",
            )
            db.execSQL(
                "CREATE TABLE IF NOT EXISTS `sync_anchors` (`hostId` TEXT NOT NULL, `scope` TEXT NOT NULL, `path` TEXT NOT NULL, " +
                    "`offset` INTEGER NOT NULL, `seq` INTEGER NOT NULL, PRIMARY KEY(`hostId`, `scope`))",
            )
            db.execSQL(
                "CREATE TABLE IF NOT EXISTS `pending` (`id` TEXT NOT NULL, `hostId` TEXT NOT NULL, `sid` TEXT, `text` TEXT NOT NULL, " +
                    "`attachments` TEXT, `mintedAt` INTEGER NOT NULL, `outcome` TEXT NOT NULL, `attempts` INTEGER NOT NULL, PRIMARY KEY(`id`))",
            )
            db.execSQL(
                "CREATE TABLE IF NOT EXISTS `drafts` (`hostId` TEXT NOT NULL, `sid` TEXT NOT NULL, `text` TEXT NOT NULL, `cwd` TEXT, " +
                    "`account` TEXT, `model` TEXT, `permission` TEXT, `updatedAt` INTEGER NOT NULL, PRIMARY KEY(`hostId`, `sid`))",
            )
        }
    }

// 键条换形：建「组」「按键」两张表，把旧按钮表的每一行照 convertLegacyButtons 转过去，再删旧表。别的表一字不动。
// fn · alt · vim 成「常用」「shell」「vim」，main 组用户加的成「按钮栏」，只在某台显示的按机器各成一组（组名是机器名）；
// 工具条那三颗内建丢掉（工具条现在是固定的）。认不出的几颗照实记下（「未能转换」），不猜。
// 长按阈值和系统的一样就不另写：系统值在这台手机上现读。
val MIGRATION_17_18 =
    object : Migration(17, 18) {
        override fun migrate(db: SupportSQLiteDatabase) {
            db.execSQL(
                "CREATE TABLE IF NOT EXISTS `key_groups` (`id` TEXT NOT NULL, `name` TEXT NOT NULL, `sortOrder` INTEGER NOT NULL, " +
                    "PRIMARY KEY(`id`))",
            )
            db.execSQL(
                "CREATE TABLE IF NOT EXISTS `term_keys` (`id` TEXT NOT NULL, `groupId` TEXT NOT NULL, `sortOrder` INTEGER NOT NULL, " +
                    "`label` TEXT, `icon` TEXT, `modifier` TEXT, `tap` TEXT, `doubleTap` TEXT, `longPress` TEXT, `doubleTapMs` INTEGER, " +
                    "`longPressMs` INTEGER, `defaultId` TEXT, `unconverted` TEXT, PRIMARY KEY(`id`))",
            )
            val hosts =
                db.query("SELECT id, label, sortOrder FROM `hosts`").use { c ->
                    generateSequence { if (c.moveToNext()) LegacyHost(c.getString(0), c.getString(1), c.getInt(2)) else null }.toList()
                }
            val rows =
                db
                    .query(
                        "SELECT id, hostId, label, command, sortOrder, ctrl, alt, shift, type, btnGroup, longPressMode, longPressCommand, " +
                            "longPressThresholdMs FROM `custom_buttons`",
                    ).use { c ->
                        generateSequence {
                            if (!c.moveToNext()) return@generateSequence null
                            LegacyButton(
                                id = c.getString(0),
                                hostId = if (c.isNull(1)) null else c.getString(1),
                                label = c.getString(2),
                                command = c.getString(3),
                                sortOrder = c.getInt(4),
                                ctrl = c.getInt(5) != 0,
                                alt = c.getInt(6) != 0,
                                shift = c.getInt(7) != 0,
                                type = c.getString(8),
                                btnGroup = c.getString(9),
                                longPressMode = c.getString(10),
                                longPressCommand = c.getString(11),
                                longPressThresholdMs = c.getInt(12),
                            )
                        }.toList()
                    }
            val (groups, keys) = convertLegacyButtons(rows, hosts, ViewConfiguration.getLongPressTimeout())
            insertKeyRows(db, groups, keys)
            db.execSQL("DROP TABLE `custom_buttons`")
        }
    }

// 续点多一格文件身份（`history-tail` 的 `file`）：旧行给空串 ⇒ 与那台的身份对不上 ⇒ 那一条下一次从头读一回。别的表一字不动。
val MIGRATION_18_19 =
    object : Migration(18, 19) {
        override fun migrate(db: SupportSQLiteDatabase) {
            db.execSQL("ALTER TABLE `sync_anchors` ADD COLUMN `file` TEXT NOT NULL DEFAULT ''")
        }
    }

// hosts 去掉用户手标的「远端系统」那一格（方言 · 有没有 tmux 都问那台了）：照 7→8 重建（老 SQLite 没有 DROP COLUMN），那一格的值丢掉。别的表一字不动。
val MIGRATION_19_20 =
    object : Migration(19, 20) {
        override fun migrate(db: SupportSQLiteDatabase) {
            db.execSQL(
                "CREATE TABLE `hosts_new` (`id` TEXT NOT NULL, `label` TEXT NOT NULL, `host` TEXT NOT NULL, " +
                    "`port` INTEGER NOT NULL, `username` TEXT NOT NULL, `authRef` TEXT, `extraAddresses` TEXT, " +
                    "`groupId` TEXT, `sortOrder` INTEGER NOT NULL, `lastConnected` INTEGER, `defaultWorkingDir` TEXT, " +
                    "`sessionManager` TEXT, `autoReconnect` INTEGER NOT NULL, `proxyJumpHostId` TEXT, `passwordEnc` TEXT, " +
                    "`agentKind` TEXT, PRIMARY KEY(`id`))",
            )
            db.execSQL(
                "INSERT INTO `hosts_new` (id,label,host,port,username,authRef,extraAddresses,groupId,sortOrder,lastConnected," +
                    "defaultWorkingDir,sessionManager,autoReconnect,proxyJumpHostId,passwordEnc,agentKind) " +
                    "SELECT id,label,host,port,username,authRef,extraAddresses,groupId,sortOrder,lastConnected," +
                    "defaultWorkingDir,sessionManager,autoReconnect,proxyJumpHostId,passwordEnc,agentKind FROM `hosts`",
            )
            db.execSQL("DROP TABLE `hosts`")
            db.execSQL("ALTER TABLE `hosts_new` RENAME TO `hosts`")
        }
    }

// 加「在做的一组」那张表（进程没了再开，带同一张票再问）。别的表一字不动。
val MIGRATION_20_21 =
    object : Migration(20, 21) {
        override fun migrate(db: SupportSQLiteDatabase) {
            db.execSQL(
                "CREATE TABLE IF NOT EXISTS `pending_batches` (`hostId` TEXT NOT NULL, `ticket` TEXT NOT NULL, `plan` TEXT NOT NULL, " +
                    "PRIMARY KEY(`hostId`, `ticket`))",
            )
        }
    }

// 密文绑到那一行：机器的密码 · 身份的私钥按「表名 · 行 id」作关联数据重加一遍（原先全库共用一份，两行的密文互换照样解得开）。
// 表结构一字不动。旧密文解不开（坏了的旧数据）⇒ 身份那一行删掉；机器那一行只清掉密码那一格，机器的别的设置照留。
// Keystore 背书的身份没有私钥字节（私钥在 Keystore 里），不碰。
class Migration21To22(
    private val crypto: CryptoBox,
) : Migration(21, 22) {
    override fun migrate(db: SupportSQLiteDatabase) {
        val passwords =
            db.query("SELECT id, passwordEnc FROM `hosts` WHERE passwordEnc IS NOT NULL").use { c ->
                generateSequence { if (c.moveToNext()) c.getString(0) to c.getString(1) else null }.toList()
            }
        val keys =
            db.query("SELECT id, privateKeyEnc FROM `identities` WHERE keystoreAlias IS NULL").use { c ->
                generateSequence { if (c.moveToNext()) c.getString(0) to c.getBlob(1) else null }.toList()
            }
        if (passwords.isEmpty() && keys.isEmpty()) return
        // 先拿到钥匙：拿不到就整趟抛出（这一跳不算迁过，下次开再迁），免得把「拿不到钥匙」当成「这一行坏了」删掉。
        crypto.encrypt(ByteArray(0), ByteArray(0))
        for ((id, enc) in passwords) {
            db.execSQL("UPDATE `hosts` SET passwordEnc = ? WHERE id = ?", arrayOf(resealPassword(crypto, id, enc), id))
        }
        for ((id, enc) in keys) {
            val sealed = reseal(crypto, enc, IDENTITIES_TABLE, id)
            if (sealed == null) {
                db.execSQL("DELETE FROM `identities` WHERE id = ?", arrayOf(id))
            } else {
                db.execSQL("UPDATE `identities` SET privateKeyEnc = ? WHERE id = ?", arrayOf(sealed, id))
            }
        }
    }
}

/** 第 21 版及以前全库共用的那一份关联数据：只这一跳解旧密文用。 */
private val SHARED_BEFORE_22 = "ccm-private-key".toByteArray(Charsets.UTF_8)

/** 一份旧密文按那一行重加；解不开 ⇒ null。 */
internal fun reseal(
    crypto: CryptoBox,
    blob: ByteArray,
    table: String,
    rowId: String,
): ByteArray? {
    val plain =
        try {
            crypto.decrypt(blob, SHARED_BEFORE_22)
        } catch (_: GeneralSecurityException) {
            return null
        }
    return crypto.encrypt(plain, rowBinding(table, rowId)).also { plain.fill(0) }
}

/** 机器那一格密码（密文的 Base64）按那一行重加；不是 Base64 · 解不开 ⇒ null（清掉那一格）。 */
internal fun resealPassword(
    crypto: CryptoBox,
    hostId: String,
    enc: String,
): String? {
    val blob =
        try {
            Base64.getDecoder().decode(enc)
        } catch (_: IllegalArgumentException) {
            return null
        }
    return reseal(crypto, blob, HOSTS_TABLE, hostId)?.let { Base64.getEncoder().encodeToString(it) }
}

// 会话列表带记号对齐：加「那一问上次的答」那张表（记号 ＋ 头）；会话行一字不动（第一次带不出记号 ⇒ 那台答整份）。
val MIGRATION_22_23 =
    object : Migration(22, 23) {
        override fun migrate(db: SupportSQLiteDatabase) {
            db.execSQL(
                "CREATE TABLE IF NOT EXISTS `list_holds` (`hostId` TEXT NOT NULL, `held` TEXT NOT NULL, `head` TEXT, PRIMARY KEY(`hostId`))",
            )
        }
    }

/** 整条迁移链。21→22 要那把主钥重加密文 ⇒ 链从依赖注入拿 [CryptoBox]（与仓库读写同一个）。 */
fun allMigrations(crypto: CryptoBox): Array<Migration> =
    arrayOf(
        MIGRATION_1_2,
        MIGRATION_2_3,
        MIGRATION_3_4,
        MIGRATION_4_5,
        MIGRATION_5_6,
        MIGRATION_6_7,
        MIGRATION_7_8,
        MIGRATION_8_9,
        MIGRATION_9_10,
        MIGRATION_10_11,
        MIGRATION_11_12,
        MIGRATION_12_13,
        MIGRATION_13_14,
        MIGRATION_14_15,
        MIGRATION_15_16,
        MIGRATION_16_17,
        MIGRATION_17_18,
        MIGRATION_18_19,
        MIGRATION_19_20,
        MIGRATION_20_21,
        Migration21To22(crypto),
        MIGRATION_22_23,
    )
