package com.ccmonitor.mobile.core.data.db

import androidx.room.ColumnInfo
import androidx.room.Entity
import androidx.room.PrimaryKey

/*
 * 本地缓存（住缓存库 [CacheDatabase]，不迁移）：那台核心交来的成品原样（JSON 文本），手机不在上面另算一份。
 * 离线打开看得到上次那几条，连上以后照那台的成品覆盖。不存「怎么接着用」（账号 · 轮换 · 目录 · 模型 · 权限档都读那台）。
 */

/** 会话列表一行：`history-list` 的 `rows[]` 那一行原样 ＋ 最后一次看的时刻。列表本身不淘汰。 */
@Entity(tableName = "sessions", primaryKeys = ["hostId", "sid"])
data class CachedSession(
    val hostId: String,
    val sid: String,
    /** 那一行原样（JSON）；看过、还没在清单里见过 ⇒ `null`。 */
    val listRow: String?,
    /** 在最近那一份清单里排第几（核心排好的先后）；不在那一份里 ⇒ `null`（离线列表不画它）。 */
    val ord: Int?,
    /** 最后一次点开的时刻（毫秒）；没点开过 ⇒ `null`。记录缓存按它留最近看过的那几条。 */
    val viewedAt: Long?,
)

/**
 * 会话列表那一问上次的答：那台给的记号 [held]（下次带着问，原样交回）＋ 头 [head]（`history-list` 回包的 `head` 原样 JSON：组 · 注解那一句 · 总数）。
 * 一台一行；行在 [CachedSession]（`listRow` · `ord`），与这一行同一个事务写。
 */
@Entity(tableName = "list_holds")
data class ListHold(
    @PrimaryKey val hostId: String,
    val held: String,
    val head: String?,
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
    val fetchedAt: Long,
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
