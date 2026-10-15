package com.ccmonitor.mobile.core.data.db

import androidx.room.Entity
import androidx.room.PrimaryKey

/*
 * 本地缓存：那台核心交来的成品原样（JSON 文本），手机不在上面另算一份。离线打开看得到上次那几条，连上以后照那台的成品覆盖。
 * 不存「怎么接着用」（账号 · 轮换 · 目录 · 模型 · 权限档都读那台）。
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
)

/** 待发：一句送出去、结局还没定或没送到的话。[outcome] 照 `session-say` 的回包：[PendingOutcome] 那四种。 */
@Entity(tableName = "pending")
data class PendingSend(
    /** 本地铸的 id。 */
    @PrimaryKey val id: String,
    val hostId: String,
    /** 哪条会话；新建会话（还没起成）⇒ `null`。 */
    val sid: String?,
    val text: String,
    /** 附件引用（JSON）；没有 ⇒ `null`。 */
    val attachments: String?,
    val mintedAt: Long,
    val outcome: String,
    val attempts: Int,
)

/** [PendingSend.outcome] 的四种（落盘的字，不上屏）。 */
object PendingOutcome {
    /** 交出去了、回包还没到。 */
    const val PENDING = "pending"

    const val DELIVERED = "delivered"

    /** 发出去了、不知道送没送到（别重发）。 */
    const val UNSURE = "unsure"

    /** 那台说没送。 */
    const val REFUSED = "refused"
}

/** 草稿：一条会话（新建会话 [sid] 为空串）输入框里还没发的字 ＋ 新建会话那几样选项。 */
@Entity(tableName = "drafts", primaryKeys = ["hostId", "sid"])
data class Draft(
    val hostId: String,
    val sid: String,
    val text: String,
    val cwd: String?,
    val account: String?,
    val model: String?,
    val permission: String?,
    val updatedAt: Long,
) {
    companion object {
        /** 新建会话那一张的 sid（主键不许空，空串代「还没有」）。 */
        const val NEW: String = ""
    }
}
