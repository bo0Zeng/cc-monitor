package com.ccmonitor.mobile.core.data.db

import android.database.sqlite.SQLiteDatabase
import android.database.sqlite.SQLiteException
import java.io.File

/**
 * 状态库还收的最老那一版：低于它的库不迁、照「太老」拒开（迁移链从这一版起，[allMigrations]）。
 * 往后再换一次库形，把这个数往上挪、删掉挪过去的那几跳与那几版的 schema。
 */
const val OLDEST_STATE_DB = 23

/** 状态库那一份文件（App 的库目录里）。 */
const val STATE_DB_FILE = "ccm.db"

/** 盘上那一份状态库开不开。 */
sealed interface StateDbAge {
    /** 新装 · 迁得上去：照常开（Room 照迁移链升）。 */
    data object Openable : StateDbAge

    /** 比 [OLDEST_STATE_DB] 还老（第 [version] 版）：不开、不迁，整屏说一句（库一字不动）。 */
    data class TooOld(
        val version: Int,
    ) : StateDbAge
}

/** 盘上那一份的版本（SQLite `user_version`；没有那份文件 ⇒ 0）⇒ 开不开。 */
fun stateDbAge(version: Int): StateDbAge = if (version in 1 until OLDEST_STATE_DB) StateDbAge.TooOld(version) else StateDbAge.Openable

/** 读盘上那一份的版本：只读打开读 `user_version`（WAL 里已交的那一笔也算）。没有那份文件 · 读不开 ⇒ 0（交 Room 照常开）。 */
fun stateDbVersion(file: File): Int {
    if (!file.isFile) return 0
    return try {
        SQLiteDatabase.openDatabase(file.path, null, SQLiteDatabase.OPEN_READONLY).use { it.version }
    } catch (_: SQLiteException) {
        0
    }
}
