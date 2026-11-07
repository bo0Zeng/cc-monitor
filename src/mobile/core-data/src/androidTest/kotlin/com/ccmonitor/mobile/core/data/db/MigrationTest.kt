package com.ccmonitor.mobile.core.data.db

import androidx.room.testing.MigrationTestHelper
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Room 迁移链（从 [OLDEST_STATE_DB] 起）：每一跳的 schema 与导出 json 校验，加数据保留。更老的库不迁（`StateDbAgeTest`）。
 * 需 androidTest assets 含 schemas（见 build.gradle sourceSets）。
 */
@RunWith(AndroidJUnit4::class)
class MigrationTest {
    private val dbName = "migration-test.db"

    @get:Rule
    val helper =
        MigrationTestHelper(
            InstrumentationRegistry.getInstrumentation(),
            AppDatabase::class.java,
        )

    private fun tables(db: androidx.sqlite.db.SupportSQLiteDatabase): Set<String> =
        db.query("SELECT name FROM sqlite_master WHERE type = 'table'").use { c ->
            generateSequence { if (c.moveToNext()) c.getString(0) else null }.toSet()
        }

    private fun columns(
        db: androidx.sqlite.db.SupportSQLiteDatabase,
        table: String,
    ): List<String> =
        db.query("PRAGMA table_info($table)").use { c ->
            generateSequence { if (c.moveToNext()) c.getString(c.getColumnIndexOrThrow("name")) else null }.toList()
        }

    /**
     * 23→24：机器表换成拨号核心那一形 —— 跳板从 id 换成那一台的名字（名字是核心算的：`label` 空 ⇒ `host`）；A→B→A 原样留着（拨号时核心拒）；
     * 跳板指着已不在的那一台 ⇒ 照抄那个 id（拨号时核心照「找不到」拒，不悄悄变直连）；备用地址原样；「起哪一家」那一格删。
     */
    @Test
    fun migrate23To24NamesTheJumpAndKeepsTheRest() {
        helper.createDatabase(dbName, 23).apply {
            fun host(
                id: String,
                label: String,
                jump: String?,
            ) = execSQL(
                "INSERT INTO hosts (id,label,host,port,username,authRef,extraAddresses,groupId,sortOrder,lastConnected," +
                    "defaultWorkingDir,sessionManager,autoReconnect,proxyJumpHostId,passwordEnc,agentKind) " +
                    "VALUES (?,?,?,2222,'pi','k1','backup.example:22\n[::1]:2200','g',3,99,'~/w','tmux',0,?,'ENC','claude')",
                arrayOf(id, label, "$id.example", jump),
            )
            host("a", "A", "b")
            host("b", "  ", "a")
            host("c", "C", "gone")
            host("d", "D", null)
            close()
        }
        val db = helper.runMigrationsAndValidate(dbName, 24, true, MIGRATION_23_24)
        db.query("SELECT id, jump FROM hosts ORDER BY id").use { c ->
            val got = generateSequence { if (c.moveToNext()) c.getString(0) to c.getString(1) else null }.toList()
            assertEquals(listOf("a" to "b.example", "b" to "A", "c" to "gone", "d" to ""), got)
        }
        db.query("SELECT label,host,port,username,authRef,addresses,groupId,sortOrder,lastConnected,defaultWorkingDir,sessionManager,autoReconnect,passwordEnc FROM hosts WHERE id='a'").use { c ->
            assertTrue(c.moveToFirst())
            val got = (0 until c.columnCount).map { if (c.isNull(it)) null else c.getString(it) }
            assertEquals(listOf("A", "a.example", "2222", "pi", "k1", "backup.example:22\n[::1]:2200", "g", "3", "99", "~/w", "tmux", "0", "ENC"), got)
        }
        db.query("PRAGMA table_info(hosts)").use { c ->
            val names = generateSequence { if (c.moveToNext()) c.getString(c.getColumnIndexOrThrow("name")) else null }.toList()
            assertTrue("旧那三格应已删掉：$names", names.none { it in setOf("proxyJumpHostId", "extraAddresses", "agentKind") })
        }
    }

    /** 24→25：状态库里那四张缓存表整张删掉（搬去缓存库，不搬行）；④ 那几张一行不丢。 */
    @Test
    fun migrate24To25DropsTheCacheTablesAndKeepsTheRest() {
        helper.createDatabase(dbName, 24).apply {
            execSQL("INSERT INTO sessions (hostId, sid, listRow, ord, viewedAt) VALUES ('h', 's', NULL, NULL, 1)")
            execSQL("INSERT INTO drafts (hostId, sid, text, cwd, account, model, permission, updatedAt) VALUES ('h', 's', '字', NULL, NULL, NULL, NULL, 1)")
            close()
        }
        val db = helper.runMigrationsAndValidate(dbName, 25, true, MIGRATION_24_25)
        db.query("SELECT name FROM sqlite_master WHERE type = 'table'").use { c ->
            val names = generateSequence { if (c.moveToNext()) c.getString(0) else null }.toSet()
            assertTrue("缓存表还在状态库里：$names", names.none { it in setOf("sessions", "list_holds", "record_pages", "sync_anchors") })
        }
        db.query("SELECT text FROM drafts WHERE hostId = 'h'").use { c ->
            assertTrue(c.moveToFirst())
            assertEquals("字", c.getString(0))
        }
    }

    /** 25→26：机器表删「连上进 tmux」那一格（宿主由那台定）；别的格原样。 */
    @Test
    fun migrate25To26DropsTheSessionManagerAndKeepsTheRest() {
        helper.createDatabase(dbName, 25).apply {
            execSQL(
                "INSERT INTO hosts (id,label,host,port,username,authRef,addresses,groupId,sortOrder,lastConnected," +
                    "defaultWorkingDir,sessionManager,autoReconnect,jump,passwordEnc) " +
                    "VALUES ('a','A','a.example',2222,'pi','k1','backup.example:22','g',3,99,'~/w','tmux',0,'B','ENC')",
            )
            close()
        }
        val db = helper.runMigrationsAndValidate(dbName, 26, true, MIGRATION_25_26)
        db.query("SELECT label,host,port,username,authRef,addresses,groupId,sortOrder,lastConnected,defaultWorkingDir,autoReconnect,jump,passwordEnc FROM hosts WHERE id='a'").use { c ->
            assertTrue(c.moveToFirst())
            val got = (0 until c.columnCount).map { if (c.isNull(it)) null else c.getString(it) }
            assertEquals(listOf("A", "a.example", "2222", "pi", "k1", "backup.example:22", "g", "3", "99", "~/w", "0", "B", "ENC"), got)
        }
        db.query("PRAGMA table_info(hosts)").use { c ->
            val names = generateSequence { if (c.moveToNext()) c.getString(c.getColumnIndexOrThrow("name")) else null }.toList()
            assertTrue("sessionManager 应已删掉：$names", "sessionManager" !in names)
        }
    }

    /** 26→27：旧书签表整张删掉（书签归各台后端，不搬行）；别的表一行不丢。 */
    @Test
    fun migrate26To27DropsTheBookmarksAndKeepsTheRest() {
        helper.createDatabase(dbName, 26).apply {
            execSQL("INSERT INTO bookmarks (id, hostId, path, label, sortOrder) VALUES ('b', 'h', '/srv', 'srv', 0)")
            execSQL("INSERT INTO drafts (hostId, sid, text, cwd, account, model, permission, updatedAt) VALUES ('h', 's', '字', NULL, NULL, NULL, NULL, 1)")
            close()
        }
        val db = helper.runMigrationsAndValidate(dbName, 27, true, MIGRATION_26_27)
        db.query("SELECT name FROM sqlite_master WHERE type = 'table'").use { c ->
            val names = generateSequence { if (c.moveToNext()) c.getString(0) else null }.toSet()
            assertTrue("书签表还在：$names", "bookmarks" !in names)
        }
        db.query("SELECT text FROM drafts WHERE hostId = 'h'").use { c ->
            assertTrue(c.moveToFirst())
            assertEquals("字", c.getString(0))
        }
    }

    /**
     * 27→28：旧主机钥匙表整张删（钥匙只住核心那份库）· 待发删附件那一格 · 草稿删新建会话那四格 · 设置表删迁移说明那一格；
     * 别的行 · 别的格一个不丢。
     */
    @Test
    fun migrate27To28DropsTheOldHostKeysAndTheUnreadCellsAndKeepsTheRest() {
        helper.createDatabase(dbName, 27).apply {
            execSQL("INSERT INTO known_hosts (host,port,keyType,publicKeyBase64,addedAt) VALUES ('example.com',2222,'ssh-ed25519','AAAA',7)")
            execSQL("INSERT INTO pending (id,hostId,sid,text,attachments,mintedAt,outcome,attempts) VALUES ('p1','h','s','你好','[\"a\"]',3,'pending',1)")
            execSQL("INSERT INTO drafts (hostId,sid,text,cwd,account,model,permission,updatedAt) VALUES ('h','s','字','~/w','work','opus','plan',5)")
            execSQL("INSERT INTO settings (`key`,`value`) VALUES ('launch_moved','h\u001fcc\u001f\u001f\u001fclaude')")
            execSQL("INSERT INTO settings (`key`,`value`) VALUES ('last_host','h')")
            close()
        }
        val db = helper.runMigrationsAndValidate(dbName, 28, true, MIGRATION_27_28)
        val names = tables(db)
        assertFalse("known_hosts 还在：$names", "known_hosts" in names)
        assertFalse("list_holds 不在状态库：$names", "list_holds" in names)
        assertEquals(listOf("id", "hostId", "sid", "text", "mintedAt", "outcome", "attempts"), columns(db, "pending"))
        assertEquals(listOf("hostId", "sid", "text", "updatedAt"), columns(db, "drafts"))
        db.query("SELECT id,hostId,sid,text,mintedAt,outcome,attempts FROM pending").use { c ->
            assertTrue(c.moveToFirst())
            assertEquals(listOf("p1", "h", "s", "你好", "3", "pending", "1"), (0 until c.columnCount).map { c.getString(it) })
        }
        db.query("SELECT text, updatedAt FROM drafts WHERE hostId='h' AND sid='s'").use { c ->
            assertTrue(c.moveToFirst())
            assertEquals("字", c.getString(0))
            assertEquals(5, c.getLong(1))
        }
        db.query("SELECT `key` FROM settings ORDER BY `key`").use { c ->
            assertEquals(listOf("last_host"), generateSequence { if (c.moveToNext()) c.getString(0) else null }.toList())
        }
    }

    /** 全链路 23→28 schema 校验（validateDroppedTables=true 严格比对导出 schema）；23 版那几组按键一路照留。 */
    @Test
    fun migrateAllFrom23To28Validates() {
        helper.createDatabase(dbName, OLDEST_STATE_DB).apply {
            execSQL("INSERT INTO key_groups (id,name,sortOrder) VALUES ('g','常用',0)")
            close()
        }
        val db = helper.runMigrationsAndValidate(dbName, 28, true, *allMigrations())
        db.query("SELECT name FROM key_groups").use { c ->
            assertEquals(listOf("常用"), generateSequence { if (c.moveToNext()) c.getString(0) else null }.toList())
        }
    }
}
