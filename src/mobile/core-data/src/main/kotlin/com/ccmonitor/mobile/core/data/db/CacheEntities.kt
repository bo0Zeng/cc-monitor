package com.ccmonitor.mobile.core.data.db

import androidx.room.ColumnInfo
import androidx.room.Entity

/*
 * 本地缓存（住缓存库 [CacheDatabase]，不迁移）：那台核心交来的成品原样（JSON 文本），手机不在上面另算一份。
 * 离线打开看得到上次那几条，连上以后照那台的成品覆盖。不存「怎么接着用」（账号 · 轮换 · 目录 · 模型 · 权限档都读那台）。
 * 会话列表副本不在这里：住嵌的核心（照那台的答换、存盘都在核心）。
 */

/** 点开过的一条会话：最后一次点开的时刻。记录缓存按它留最近看过的那几条。 */
@Entity(tableName = "sessions", primaryKeys = ["hostId", "sid"])
data class CachedSession(
    val hostId: String,
    val sid: String,
    /** 最后一次点开的时刻（毫秒）。 */
    val viewedAt: Long,
)

/** 一页记录：`history-read` / `history-tail` 读到的 `rows[]` 原样，[start] 是页锚（这一页从哪个字节起）。 */
@Entity(tableName = "record_pages", primaryKeys = ["hostId", "sid", "start"])
data class RecordPage(
    val hostId: String,
    val sid: String,
    val start: Long,
    /** 这一页到哪个字节为止（下一页从这里接）；流上来的行没带字节时照前一个。 */
    val end: Long,
    /** `rows[]` 原样（JSON 数组，每格是 `{end, record}`）。 */
    val rows: String,
)

/** 续点：一条会话往下续读从哪接（[scope] 是 sid）。[offset] 是已有最后一行之后的字节，[seq] 是下一行的行号。 */
@Entity(tableName = "sync_anchors", primaryKeys = ["hostId", "scope"])
data class SyncAnchor(
    val hostId: String,
    val scope: String,
    /** 那台的记录文件（续读按它问；换了文件 ⇒ 从头读）。 */
    val path: String,
    val offset: Long,
    val seq: Long,
    /** 那台给的记号（`history-tail` 的 `held.mark`）：续读时原样交回，对不上那台说从头读。 */
    @ColumnInfo(name = "file", defaultValue = "")
    val mark: String,
)
