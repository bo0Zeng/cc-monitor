package com.ccmonitor.mobile.core.data.db

import androidx.room.testing.MigrationTestHelper
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Room 迁移链：每一跳的 schema 与导出 json 校验，加数据保留。
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

    /** 1→2：旧 host 数据保留 + 新增 extraAddresses 列为 null。 */
    @Test
    fun migrate1To2PreservesHostData() {
        helper.createDatabase(dbName, 1).apply {
            execSQL(
                "INSERT INTO hosts (id,label,host,port,username,authRef,groupId,sortOrder," +
                    "lastConnected,defaultWorkingDir,sessionManager,colorScheme,autoReconnect," +
                    "reconnectOnNetworkChange,maxReconnectAttempts) " +
                    "VALUES ('h1','NanoPi','192.0.2.2',22,'pi',NULL,NULL,0,NULL,NULL,NULL,NULL,1,1,0)",
            )
            close()
        }
        val db = helper.runMigrationsAndValidate(dbName, 2, true, MIGRATION_1_2)
        db.query("SELECT host, extraAddresses FROM hosts WHERE id='h1'").use { c ->
            assertTrue("旧 host 应保留", c.moveToFirst())
            assertEquals("192.0.2.2", c.getString(0))
            assertTrue("迁移后 extraAddresses 应为 null", c.isNull(1))
        }
    }

    /** 2→3：新增 bookmarks 表可写读 + 旧 host 保留。 */
    @Test
    fun migrate2To3CreatesBookmarks() {
        helper.createDatabase(dbName, 2).apply {
            execSQL(
                "INSERT INTO hosts (id,label,host,port,username,authRef,groupId,sortOrder," +
                    "lastConnected,defaultWorkingDir,sessionManager,colorScheme,autoReconnect," +
                    "reconnectOnNetworkChange,maxReconnectAttempts,extraAddresses) " +
                    "VALUES ('h2','H','1.1.1.1',22,'u',NULL,NULL,0,NULL,NULL,NULL,NULL,1,1,0,NULL)",
            )
            close()
        }
        val db = helper.runMigrationsAndValidate(dbName, 3, true, MIGRATION_2_3)
        db.query("SELECT id FROM hosts WHERE id='h2'").use { c -> assertTrue("旧 host 应保留", c.moveToFirst()) }
        db.execSQL("INSERT INTO bookmarks (id,hostId,path,label,sortOrder) VALUES ('b1','h2','/tmp','tmp',0)")
        db.query("SELECT path FROM bookmarks WHERE id='b1'").use { c ->
            assertTrue("bookmarks 表应可用", c.moveToFirst())
            assertEquals("/tmp", c.getString(0))
        }
    }

    /** 3→4：新增 launchers 表可写读 + 旧 host 保留。 */
    @Test
    fun migrate3To4CreatesLaunchers() {
        helper.createDatabase(dbName, 3).apply {
            execSQL(
                "INSERT INTO hosts (id,label,host,port,username,authRef,groupId,sortOrder," +
                    "lastConnected,defaultWorkingDir,sessionManager,colorScheme,autoReconnect," +
                    "reconnectOnNetworkChange,maxReconnectAttempts,extraAddresses) " +
                    "VALUES ('h3','H','1.1.1.1',22,'u',NULL,NULL,0,NULL,NULL,NULL,NULL,1,1,0,NULL)",
            )
            close()
        }
        val db = helper.runMigrationsAndValidate(dbName, 4, true, MIGRATION_3_4)
        db.query("SELECT id FROM hosts WHERE id='h3'").use { c -> assertTrue("旧 host 应保留", c.moveToFirst()) }
        db.execSQL(
            "INSERT INTO launchers (id,hostId,label,workingDir,tmuxSession,command,sortOrder,pinned) " +
                "VALUES ('l1','h3','cc',NULL,'cc','claude',0,0)",
        )
        db.query("SELECT command, tmuxSession FROM launchers WHERE id='l1'").use { c ->
            assertTrue("launchers 表应可用", c.moveToFirst())
            assertEquals("claude", c.getString(0))
            assertEquals("cc", c.getString(1))
        }
    }

    /** 4→5：hosts 加 claudeDir（旧 host 保留、新列 null）+ settings 键值表可写读。 */
    @Test
    fun migrate4To5AddsClaudeDirAndSettings() {
        helper.createDatabase(dbName, 4).apply {
            execSQL(
                "INSERT INTO hosts (id,label,host,port,username,authRef,groupId,sortOrder," +
                    "lastConnected,defaultWorkingDir,sessionManager,colorScheme,autoReconnect," +
                    "reconnectOnNetworkChange,maxReconnectAttempts,extraAddresses) " +
                    "VALUES ('h4','H','1.1.1.1',22,'u',NULL,NULL,0,NULL,NULL,NULL,NULL,1,1,0,NULL)",
            )
            close()
        }
        val db = helper.runMigrationsAndValidate(dbName, 5, true, MIGRATION_4_5)
        db.query("SELECT claudeDir FROM hosts WHERE id='h4'").use { c ->
            assertTrue("旧 host 应保留", c.moveToFirst())
            assertTrue("迁移后 claudeDir 应为 null", c.isNull(0))
        }
        db.execSQL("INSERT INTO settings (`key`,`value`) VALUES ('default_claude_dir','/data/claude')")
        db.query("SELECT `value` FROM settings WHERE `key`='default_claude_dir'").use { c ->
            assertTrue("settings 表应可用", c.moveToFirst())
            assertEquals("/data/claude", c.getString(0))
        }
    }

    @Test
    fun migrate5To6AddsProxyJumpHostId() {
        helper.createDatabase(dbName, 5).apply {
            execSQL(
                "INSERT INTO hosts (id,label,host,port,username,authRef,groupId,sortOrder," +
                    "lastConnected,defaultWorkingDir,sessionManager,colorScheme,autoReconnect," +
                    "reconnectOnNetworkChange,maxReconnectAttempts,extraAddresses,claudeDir) " +
                    "VALUES ('h5','H','1.1.1.1',22,'u',NULL,NULL,0,NULL,NULL,NULL,NULL,1,1,0,NULL,NULL)",
            )
            close()
        }
        val db = helper.runMigrationsAndValidate(dbName, 6, true, MIGRATION_5_6)
        db.query("SELECT proxyJumpHostId FROM hosts WHERE id='h5'").use { c ->
            assertTrue("旧 host 应保留", c.moveToFirst())
            assertTrue("迁移后 proxyJumpHostId 应为 null", c.isNull(0))
        }
    }

    @Test
    fun migrate6To7AddsPasswordEnc() {
        helper.createDatabase(dbName, 6).apply {
            execSQL(
                "INSERT INTO hosts (id,label,host,port,username,authRef,groupId,sortOrder," +
                    "lastConnected,defaultWorkingDir,sessionManager,colorScheme,autoReconnect," +
                    "reconnectOnNetworkChange,maxReconnectAttempts,extraAddresses,claudeDir,proxyJumpHostId) " +
                    "VALUES ('h6','H','1.1.1.1',22,'u',NULL,NULL,0,NULL,NULL,NULL,NULL,1,1,0,NULL,NULL,NULL)",
            )
            close()
        }
        val db = helper.runMigrationsAndValidate(dbName, 7, true, MIGRATION_6_7)
        db.query("SELECT passwordEnc FROM hosts WHERE id='h6'").use { c ->
            assertTrue("旧 host 应保留", c.moveToFirst())
            assertTrue("迁移后 passwordEnc 应为 null", c.isNull(0))
        }
    }

    @Test
    fun migrate7To8DropsDeadColumnsKeepingData() {
        helper.createDatabase(dbName, 7).apply {
            // v7 行仍含 colorScheme（hosts）/ biometricProtected（identities）死列。
            execSQL(
                "INSERT INTO hosts (id,label,host,port,username,authRef,groupId,sortOrder," +
                    "lastConnected,defaultWorkingDir,sessionManager,colorScheme,autoReconnect," +
                    "reconnectOnNetworkChange,maxReconnectAttempts,extraAddresses,claudeDir,proxyJumpHostId,passwordEnc) " +
                    "VALUES ('h7','H','1.1.1.1',22,'u',NULL,NULL,0,NULL,NULL,NULL,'dark',1,1,0,NULL,'/mid/.claude',NULL,'pw')",
            )
            execSQL(
                "INSERT INTO identities (id,label,keyType,privateKeyEnc,publicKeyOpenSsh,fingerprintSha256," +
                    "createdAt,biometricProtected,keystoreAlias) VALUES ('i7','I','ed25519',X'00','pub','fp',0,1,'alias')",
            )
            close()
        }
        val db = helper.runMigrationsAndValidate(dbName, 8, true, MIGRATION_7_8)
        // 保留列数据未丢；死列已不存在（SELECT 死列应报错——用 pragma 验列不在）。
        db.query("SELECT passwordEnc, claudeDir FROM hosts WHERE id='h7'").use { c ->
            assertTrue("hosts 行应保留", c.moveToFirst())
            assertEquals("pw", c.getString(0)) // 末尾保留列
            assertEquals("/mid/.claude", c.getString(1)) // 中段保留列（防重建串位）
        }
        db.query("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='claude_bindings'").use { c ->
            c.moveToFirst()
            assertEquals("死表 claude_bindings 应已删", 0, c.getInt(0))
        }
        db.query("SELECT keystoreAlias FROM identities WHERE id='i7'").use { c ->
            assertTrue("identities 行应保留", c.moveToFirst())
            assertEquals("alias", c.getString(0))
        }
        db.query("SELECT COUNT(*) FROM pragma_table_info('hosts') WHERE name='colorScheme'").use { c ->
            c.moveToFirst()
            assertEquals("colorScheme 死列应已删", 0, c.getInt(0))
        }
        db.query("SELECT COUNT(*) FROM pragma_table_info('identities') WHERE name='biometricProtected'").use { c ->
            c.moveToFirst()
            assertEquals("biometricProtected 死列应已删", 0, c.getInt(0))
        }
    }

    @Test
    fun migrate8To9AddsOsColumn() {
        helper.createDatabase(dbName, 8).apply {
            execSQL(
                "INSERT INTO hosts (id,label,host,port,username,authRef,groupId,sortOrder," +
                    "lastConnected,defaultWorkingDir,sessionManager,autoReconnect," +
                    "reconnectOnNetworkChange,maxReconnectAttempts,extraAddresses,claudeDir,proxyJumpHostId,passwordEnc) " +
                    "VALUES ('h8','H','1.1.1.1',22,'u',NULL,NULL,0,NULL,'/w',NULL,1,1,0,NULL,NULL,NULL,NULL)",
            )
            close()
        }
        val db = helper.runMigrationsAndValidate(dbName, 9, true, MIGRATION_8_9)
        db.query("SELECT os, defaultWorkingDir FROM hosts WHERE id='h8'").use { c ->
            assertTrue("旧 host 应保留", c.moveToFirst())
            assertTrue("迁移后 os 应为 null（默认 POSIX）", c.isNull(0))
            assertEquals("保留列不丢", "/w", c.getString(1))
        }
    }

    /** 9→10：custom_buttons 加 ctrl/alt/shift（组合键），存量行默认 0，保留列不丢。 */
    @Test
    fun migrate9To10AddsButtonModifierColumns() {
        helper.createDatabase(dbName, 9).apply {
            execSQL(
                "INSERT INTO custom_buttons (id,hostId,label,command,sortOrder) VALUES ('b9','h','Ctrl+C','c',0)",
            )
            close()
        }
        val db = helper.runMigrationsAndValidate(dbName, 10, true, MIGRATION_9_10)
        db.query("SELECT command, ctrl, alt, shift FROM custom_buttons WHERE id='b9'").use { c ->
            assertTrue("旧按钮应保留", c.moveToFirst())
            assertEquals("保留列不丢", "c", c.getString(0))
            assertEquals("ctrl 默认 0", 0, c.getInt(1))
            assertEquals("alt 默认 0", 0, c.getInt(2))
            assertEquals("shift 默认 0", 0, c.getInt(3))
        }
    }

    /**
     * 10→11：custom_buttons 加 type（默认 'command'）并回填：
     * 有修饰的存量行设为 keycode（派发逐字不变），无修饰行保持 command；保留列不丢。
     */
    @Test
    fun migrate10To11AddsTypeColumnAndBackfills() {
        helper.createDatabase(dbName, 10).apply {
            // 无修饰行（命令行按钮）+ 有修饰行（Ctrl+C 按键码按钮）
            execSQL(
                "INSERT INTO custom_buttons (id,hostId,label,command,sortOrder,ctrl,alt,shift) " +
                    "VALUES ('bc','h','ls','ls -la',0,0,0,0)",
            )
            execSQL(
                "INSERT INTO custom_buttons (id,hostId,label,command,sortOrder,ctrl,alt,shift) " +
                    "VALUES ('bm','h','Ctrl+C','c',1,1,0,0)",
            )
            close()
        }
        val db = helper.runMigrationsAndValidate(dbName, 11, true, MIGRATION_10_11)
        db.query("SELECT command, ctrl, type FROM custom_buttons WHERE id='bc'").use { c ->
            assertTrue("无修饰行应保留", c.moveToFirst())
            assertEquals("保留列不丢", "ls -la", c.getString(0))
            assertEquals("无修饰 → type=command", "command", c.getString(2))
        }
        db.query("SELECT command, ctrl, type FROM custom_buttons WHERE id='bm'").use { c ->
            assertTrue("有修饰行应保留", c.moveToFirst())
            assertEquals("保留基准键", "c", c.getString(0))
            assertEquals("ctrl 保留", 1, c.getInt(1))
            assertEquals("有修饰 → 回填 type=keycode（派发逐字不变）", "keycode", c.getString(2))
        }
    }

    /** 11→12：hosts 加 agentKind（可空 TEXT，无 DEFAULT、无回填，null 即 ClaudeCode）；保留列不丢。 */
    @Test
    fun migrate11To12AddsAgentKindColumn() {
        helper.createDatabase(dbName, 11).apply {
            execSQL(
                "INSERT INTO hosts (id,label,host,port,username,authRef,groupId,sortOrder," +
                    "lastConnected,defaultWorkingDir,sessionManager,autoReconnect," +
                    "reconnectOnNetworkChange,maxReconnectAttempts,extraAddresses,claudeDir,proxyJumpHostId,passwordEnc,os) " +
                    "VALUES ('h11','H','1.1.1.1',22,'u',NULL,NULL,0,NULL,'/w',NULL,1,1,0,NULL,'/c',NULL,NULL,NULL)",
            )
            close()
        }
        val db = helper.runMigrationsAndValidate(dbName, 12, true, MIGRATION_11_12)
        db.query("SELECT agentKind, claudeDir FROM hosts WHERE id='h11'").use { c ->
            assertTrue("旧 host 应保留", c.moveToFirst())
            assertTrue("迁移后 agentKind 应为 null（默认 ClaudeCode）", c.isNull(0))
            assertEquals("保留列不丢", "/c", c.getString(1))
        }
    }

    /**
     * 12→13：custom_buttons 加 btnGroup（默认 'main'）并种默认键。存量按钮保留、btnGroup 为 main；
     * 种子 40 条固定 id 写入。MigrationTestHelper 不触发 onCreate，新装路径另由 SeedOnCreateTest 验。
     */
    @Test
    fun migrate12To13AddsBtnGroupAndSeeds() {
        helper.createDatabase(dbName, 12).apply {
            execSQL(
                "INSERT INTO custom_buttons (id,hostId,label,command,sortOrder,ctrl,alt,shift,type) " +
                    "VALUES ('bu','h','my','ls',0,0,0,0,'command')",
            )
            close()
        }
        val db = helper.runMigrationsAndValidate(dbName, 13, true, MIGRATION_12_13)
        db.query("SELECT command, btnGroup FROM custom_buttons WHERE id='bu'").use { c ->
            assertTrue("旧按钮应保留", c.moveToFirst())
            assertEquals("保留列不丢", "ls", c.getString(0))
            assertEquals("存量按钮 btnGroup 默认 main", "main", c.getString(1))
        }
        db.query("SELECT btnGroup, type FROM custom_buttons WHERE id='ak-fn-left'").use { c ->
            assertTrue("fn 种子应写入", c.moveToFirst())
            assertEquals("fn", c.getString(0))
            assertEquals("escape", c.getString(1))
        }
        db.query("SELECT COUNT(*) FROM custom_buttons WHERE id LIKE 'ak-%'").use { c ->
            c.moveToFirst()
            assertEquals("种子 40 条", 40, c.getInt(0))
        }
    }

    /** 13→14：custom_buttons 加长按 4 列，存量行取默认（none/''/400/60），保留列不丢。 */
    @Test
    fun migrate13To14AddsLongPressColumns() {
        helper.createDatabase(dbName, 13).apply {
            execSQL(
                "INSERT INTO custom_buttons (id,hostId,label,command,sortOrder,ctrl,alt,shift,type,btnGroup) " +
                    "VALUES ('b13','h','my','ls',0,0,0,0,'command','main')",
            )
            close()
        }
        val db = helper.runMigrationsAndValidate(dbName, 14, true, MIGRATION_13_14)
        val sql =
            "SELECT command, longPressMode, longPressCommand, longPressThresholdMs, longPressRepeatMs " +
                "FROM custom_buttons WHERE id='b13'"
        db.query(sql).use { c ->
            assertTrue("旧按钮应保留", c.moveToFirst())
            assertEquals("保留列不丢", "ls", c.getString(0))
            assertEquals("longPressMode 默认 none", "none", c.getString(1))
            assertEquals("longPressCommand 默认空", "", c.getString(2))
            assertEquals("阈值默认 400", 400, c.getInt(3))
            assertEquals("速率默认 60", 60, c.getInt(4))
        }
    }

    /** 全链路 1→14 schema 校验（validateDroppedTables=true 严格比对导出 schema）。 */
    @Test
    fun migrateAll1To14Validates() {
        helper.createDatabase(dbName, 1).close()
        helper.runMigrationsAndValidate(dbName, 14, true, *ALL_MIGRATIONS)
    }
}
