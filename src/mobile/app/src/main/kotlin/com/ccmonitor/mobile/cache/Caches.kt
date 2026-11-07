package com.ccmonitor.mobile.cache

import com.ccmonitor.mobile.core.claude.link.wire.RawPatch
import com.ccmonitor.mobile.core.claude.link.wire.rawPatch

/*
 * 手机本地缓存，聊天屏与会话列表那两侧看到的样子。装的都是那台核心交来的成品原样（`rows[]` 一格一格），读回来交给与线上同一个解码器；
 * 手机不在缓存上另算一份。读的时候先上缓存（离线打开看得到上次那几条）、再跟那台对齐（连上以后照那台的成品覆盖）。
 */

/** 一页记录：从 [start] 字节起到 [end] 为止，[rows] 是 `rows[]` 原样（每格 `{end, record}`）。 */
data class RawPage(
    val start: Long,
    val end: Long,
    val rows: List<Any?>,
)

/** 续点：那台的记录文件 [path]，已有最后一行之后的字节 [offset]，下一行的行号 [seq]，那台给的记号 [mark]（`history-tail` 的 `held`，原样交回）。 */
data class Anchor(
    val path: String,
    val offset: Long,
    val seq: Long,
    val mark: String,
)

/** 一条会话上次读到的：几页记录 ＋ 续点（不知道接在哪 ⇒ `null`，下次从头读）。 */
data class CachedRecords(
    val pages: List<RawPage>,
    val anchor: Anchor?,
)

/** 一句待发（`pending` 那一行）：[outcome] 是 `PendingOutcome` 那四种之一。 */
data class Pending(
    val id: String,
    val sid: String?,
    val text: String,
    val outcome: String,
    val attempts: Int,
)

/** 一台机器的聊天缓存：记录页 · 续点 · 待发 · 草稿。 */
interface ChatCache {
    suspend fun load(sid: String): CachedRecords?

    /** 点开了这一条（记录缓存按最近看过的留）。 */
    suspend fun viewed(sid: String)

    /** 整份重来：这一条的页全换掉，续点换成 [anchor]。 */
    suspend fun replace(
        sid: String,
        pages: List<RawPage>,
        anchor: Anchor?,
    )

    /** 往上翻读到的更早那几页（续点不动）。 */
    suspend fun prepend(
        sid: String,
        pages: List<RawPage>,
    )

    /** 末尾新来的几格（`{end, record}` 原样）；[anchor] 是接完之后的续点，`null` ＝ 不知道接在哪了（续点作废）。 */
    suspend fun append(
        sid: String,
        rows: List<Any?>,
        anchor: Anchor?,
    )

    suspend fun pending(sid: String): List<Pending>

    /** 草稿变待发（删 [draftSid] 那张草稿与落 [p] 同一个事务）。 */
    suspend fun promote(
        draftSid: String,
        p: Pending,
    )

    suspend fun settle(
        id: String,
        outcome: String,
        attempts: Int,
    )

    /** 新建会话起成了：那几句归到 [sid]。 */
    suspend fun bind(
        ids: List<String>,
        sid: String,
    )

    /** 记录里出现了 ⇒ 不再是待发。 */
    suspend fun forget(ids: List<String>)

    suspend fun draft(sid: String): String?

    /** 存草稿（[text] 空 ⇒ 删）。 */
    suspend fun saveDraft(
        sid: String,
        text: String,
    )

    /** 不落盘（测试 · 预览）。 */
    object None : ChatCache {
        override suspend fun load(sid: String): CachedRecords? = null

        override suspend fun viewed(sid: String) = Unit

        override suspend fun replace(
            sid: String,
            pages: List<RawPage>,
            anchor: Anchor?,
        ) = Unit

        override suspend fun prepend(
            sid: String,
            pages: List<RawPage>,
        ) = Unit

        override suspend fun append(
            sid: String,
            rows: List<Any?>,
            anchor: Anchor?,
        ) = Unit

        override suspend fun pending(sid: String): List<Pending> = emptyList()

        override suspend fun promote(
            draftSid: String,
            p: Pending,
        ) = Unit

        override suspend fun settle(
            id: String,
            outcome: String,
            attempts: Int,
        ) = Unit

        override suspend fun bind(
            ids: List<String>,
            sid: String,
        ) = Unit

        override suspend fun forget(ids: List<String>) = Unit

        override suspend fun draft(sid: String): String? = null

        override suspend fun saveDraft(
            sid: String,
            text: String,
        ) = Unit
    }
}

/**
 * 一台机器的会话列表副本：`history-list` 不带筛选那一问手上那一份，离线照它画、下次带着它的记号问。
 * 照那台的答换、存盘都在嵌的核心里（`Core.listing*`）；这里只交答、拿整份那一形。列表本身不淘汰。
 */
interface ListCache {
    /** 手上那一份（整份那一形，记号在 `held`）；没问过 ⇒ `null`（不带记号问 ⇒ 那台答整份）。 */
    suspend fun held(): RawPatch?

    /** 那台那一问的回包交核心换上 ⇒ 换好的整份；读不动 ⇒ `null`，手上那一份不动。 */
    suspend fun keep(reply: Any?): RawPatch?

    /** 不留副本（测试 · 预览）：那台答的整份原样交回。 */
    object None : ListCache {
        override suspend fun held(): RawPatch? = null

        override suspend fun keep(reply: Any?): RawPatch? = rawPatch(reply)?.takeIf { it.whole }
    }
}
