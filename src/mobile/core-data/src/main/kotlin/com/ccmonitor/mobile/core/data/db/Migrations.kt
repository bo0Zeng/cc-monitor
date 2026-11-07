package com.ccmonitor.mobile.core.data.db

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

/*
 * Room 迁移链，升级不清库：从 [OLDEST_STATE_DB] 起逐跳到 `AppDatabase` 的 `version`；更老的库不迁、照「太老」拒开（[StateDbAge]）。
 * 往后再换一次库形，把 [OLDEST_STATE_DB] 往上挪，挪过去的那几跳与那几版的 schema 一起删。
 * DDL 与 core-data/schemas 下导出的 json createSql 严格对齐（MigrationTest 用 MigrationTestHelper 校验）。
 * 老 SQLite 没有 DROP COLUMN：删格一律照「建新表 → 搬行 → 删旧表 → 改名」重建。
 */

// 机器表换成拨号核心那一形（同桌面那张表）：跳板从「另一台的 id」换成「另一台的名字」（名字由核心算，[machineNames]）·
// 备用地址那一格改名 `addresses`（一行一条，原样）·「起哪一家」那一格删（缺省照那台的，要别的在起会话框里选）。
// 跳板指着的那一台已经不在 ⇒ 名字照抄那个 id：拨号时核心照「跳板找不到」拒，不悄悄变直连。别的表一字不动。
val MIGRATION_23_24 =
    object : Migration(23, 24) {
        override fun migrate(db: SupportSQLiteDatabase) {
            val rows =
                db.query("SELECT id, label, host, proxyJumpHostId FROM `hosts`").use { c ->
                    generateSequence {
                        if (c.moveToNext()) listOf(c.getString(0), c.getString(1), c.getString(2), c.getString(3)) else null
                    }.toList()
                }
            val names =
                rows.map { it[0] }.zip(machineNames(rows.map { Host(id = it[0], label = it[1], host = it[2], username = "") })).toMap()
            db.execSQL(
                "CREATE TABLE `hosts_new` (`id` TEXT NOT NULL, `label` TEXT NOT NULL, `host` TEXT NOT NULL, " +
                    "`port` INTEGER NOT NULL, `username` TEXT NOT NULL, `authRef` TEXT, `addresses` TEXT NOT NULL, " +
                    "`groupId` TEXT, `sortOrder` INTEGER NOT NULL, `lastConnected` INTEGER, `defaultWorkingDir` TEXT, " +
                    "`sessionManager` TEXT, `autoReconnect` INTEGER NOT NULL, `jump` TEXT NOT NULL, `passwordEnc` TEXT, " +
                    "PRIMARY KEY(`id`))",
            )
            db.execSQL(
                "INSERT INTO `hosts_new` (id,label,host,port,username,authRef,addresses,groupId,sortOrder,lastConnected," +
                    "defaultWorkingDir,sessionManager,autoReconnect,jump,passwordEnc) " +
                    "SELECT id,label,host,port,username,authRef,COALESCE(extraAddresses,''),groupId,sortOrder," +
                    "lastConnected,defaultWorkingDir,sessionManager,autoReconnect,'',passwordEnc FROM `hosts`",
            )
            for (r in rows) {
                val via = r[3]?.takeIf { it.isNotBlank() } ?: continue
                db.execSQL("UPDATE `hosts_new` SET jump = ? WHERE id = ?", arrayOf(names[via] ?: via, r[0]))
            }
            db.execSQL("DROP TABLE `hosts`")
            db.execSQL("ALTER TABLE `hosts_new` RENAME TO `hosts`")
        }
    }

// 24 → 25：缓存那四张表搬去缓存库（`CacheDatabase`，不迁移）：状态库里那几张整张删掉，不搬行（缓存，照那台重取）。别的表一字不动。
val MIGRATION_24_25 =
    object : Migration(24, 25) {
        override fun migrate(db: SupportSQLiteDatabase) {
            for (t in listOf("sessions", "list_holds", "record_pages", "sync_anchors")) {
                db.execSQL("DROP TABLE IF EXISTS `$t`")
            }
        }
    }

// 25 → 26：机器表删「连上进 tmux」那一格（会话放在哪由那台定）。别的格 · 别的表一字不动。
val MIGRATION_25_26 =
    object : Migration(25, 26) {
        override fun migrate(db: SupportSQLiteDatabase) {
            val cols =
                "id,label,host,port,username,authRef,addresses,groupId,sortOrder,lastConnected,defaultWorkingDir,autoReconnect,jump,passwordEnc"
            db.execSQL(
                "CREATE TABLE `hosts_new` (`id` TEXT NOT NULL, `label` TEXT NOT NULL, `host` TEXT NOT NULL, " +
                    "`port` INTEGER NOT NULL, `username` TEXT NOT NULL, `authRef` TEXT, `addresses` TEXT NOT NULL, " +
                    "`groupId` TEXT, `sortOrder` INTEGER NOT NULL, `lastConnected` INTEGER, `defaultWorkingDir` TEXT, " +
                    "`autoReconnect` INTEGER NOT NULL, `jump` TEXT NOT NULL, `passwordEnc` TEXT, PRIMARY KEY(`id`))",
            )
            db.execSQL("INSERT INTO `hosts_new` ($cols) SELECT $cols FROM `hosts`")
            db.execSQL("DROP TABLE `hosts`")
            db.execSQL("ALTER TABLE `hosts_new` RENAME TO `hosts`")
        }
    }

// 26 → 27：书签已归各台后端，旧书签表整张删掉（不搬行）。别的表一字不动。
val MIGRATION_26_27 =
    object : Migration(26, 27) {
        override fun migrate(db: SupportSQLiteDatabase) {
            db.execSQL("DROP TABLE IF EXISTS `bookmarks`")
        }
    }

// 27 → 28：主机钥匙只住核心那份库，旧表 `known_hosts` 整张删（迁不全的那几台下次连照首见重新记）；
// 待发删「附件」那一格、草稿删新建会话那四格（目录 · 账号 · 模型 · 权限，都没人读）；设置表删迁移说明那一格（`launch_moved`）。
// 别的格 · 别的表一字不动。
val MIGRATION_27_28 =
    object : Migration(27, 28) {
        override fun migrate(db: SupportSQLiteDatabase) {
            db.execSQL("DROP TABLE IF EXISTS `known_hosts`")
            db.execSQL("DELETE FROM `settings` WHERE `key` = 'launch_moved'")
            val pending = "id,hostId,sid,text,mintedAt,outcome,attempts"
            db.execSQL(
                "CREATE TABLE `pending_new` (`id` TEXT NOT NULL, `hostId` TEXT NOT NULL, `sid` TEXT, `text` TEXT NOT NULL, " +
                    "`mintedAt` INTEGER NOT NULL, `outcome` TEXT NOT NULL, `attempts` INTEGER NOT NULL, PRIMARY KEY(`id`))",
            )
            db.execSQL("INSERT INTO `pending_new` ($pending) SELECT $pending FROM `pending`")
            db.execSQL("DROP TABLE `pending`")
            db.execSQL("ALTER TABLE `pending_new` RENAME TO `pending`")
            val drafts = "hostId,sid,text,updatedAt"
            db.execSQL(
                "CREATE TABLE `drafts_new` (`hostId` TEXT NOT NULL, `sid` TEXT NOT NULL, `text` TEXT NOT NULL, " +
                    "`updatedAt` INTEGER NOT NULL, PRIMARY KEY(`hostId`, `sid`))",
            )
            db.execSQL("INSERT INTO `drafts_new` ($drafts) SELECT $drafts FROM `drafts`")
            db.execSQL("DROP TABLE `drafts`")
            db.execSQL("ALTER TABLE `drafts_new` RENAME TO `drafts`")
        }
    }

/** 整条迁移链（从 [OLDEST_STATE_DB] 起）。 */
fun allMigrations(): Array<Migration> =
    arrayOf(
        MIGRATION_23_24,
        MIGRATION_24_25,
        MIGRATION_25_26,
        MIGRATION_26_27,
        MIGRATION_27_28,
    )
