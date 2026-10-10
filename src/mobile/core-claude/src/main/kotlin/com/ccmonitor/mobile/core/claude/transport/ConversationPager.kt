package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.core.claude.transport.DaemonConversationCatalog.Conversation
import com.ccmonitor.mobile.core.claude.transport.DaemonConversationCatalog.Project

/**
 * 按项目切页的历史翻页器：翻多远下多远。
 *
 * 后端的 `--list-sessions` 不支持分页（`--limit` 被静默忽略），所以页边界在客户端切：
 * 项目按 [Project.lastActivityMs] 倒序，一页凑够 [pageSize] 条对话就停，下一页从没查过的那个项目接着走。
 * 「翻多远下多远」看的是发出去几次查询，不是结果长什么样（全量重下再本地切片，结果也照样对），
 * 所以 [fetched] 记下每个 dirName 被查过几次。
 *
 * 翻页只调 [fetch]（一次性查询），不碰实时流；重起实时流会再来一次大流量洪峰。
 *
 * 边界：
 * 1. 一页的大小是下界不是上界：一个项目自己就可能有上百条对话。
 * 2. 不跨项目去重：一条对话只属于一个项目目录，这是后端的性质，不是本类保证的。
 * 3. 查询失败的项目也算翻过：进 [Page.failedProjects]，[cursor] 照常前进，否则一个坏项目会把翻页卡死。
 *    代价是那个项目这一轮看不到，所以上层必须把失败显示出来。
 *
 * @param pageSize 一页至少凑多少条对话。
 * @param fetch 取一个项目下的对话。返回 `null` = 查询失败（与「这个项目没有对话」分开）。
 *   放在最后一个位置，调用方用尾随 lambda。
 */
class ConversationPager(
    private val pageSize: Int = DEFAULT_PAGE_SIZE,
    private val fetch: suspend (dirName: String) -> List<Conversation>?,
) {
    /**
     * 一次翻页的结果。
     *
     * @param added 本页新增的对话（已按 [Conversation.updatedAtMs] 倒序）。
     * @param exhausted 项目走完了 ⇒ 已是最早。UI 要显式说出来，不许无限转圈。
     * @param failedProjects 本页里查询失败的项目目录名。非空时上层必须显示，否则列表缺了几个项目却毫无线索。
     */
    data class Page(
        val added: List<Conversation> = emptyList(),
        val exhausted: Boolean = false,
        val failedProjects: List<String> = emptyList(),
    )

    /** 按 [Project.lastActivityMs] 倒序排好的项目表。`start` 之前是空的。 */
    private var projects: List<Project> = emptyList()

    /** 下一页从第几个项目开始。只前进，不回头。 */
    private var cursor: Int = 0

    /**
     * 每个 dirName 被 [fetch] 过几次。「不重传第一页」等价于「第一页查过的项目，第二页一次都不再查」。
     */
    private val fetchCounts = LinkedHashMap<String, Int>()

    /** 已经发出去的查询记录（dirName → 次数）。 */
    val fetched: Map<String, Int> get() = fetchCounts

    /** 还有没有下一页。项目走完 ⇒ 没有了。 */
    val hasMore: Boolean get() = cursor < projects.size

    /** 已经翻过几个项目。 */
    val projectsConsumed: Int get() = cursor

    /**
     * 换一份项目表重新开始翻，清空游标与查询记录。
     * 排序在这里做（不假设后端给的有序）：最近活动的项目排前面。
     */
    fun start(all: List<Project>) {
        // 只留有对话的项目：`sessionCount == 0` 的查了也是空，白多一个 SSH 往返。
        projects = all.filter { it.sessionCount > 0 }.sortedByDescending { it.lastActivityMs }
        cursor = 0
        fetchCounts.clear()
    }

    /**
     * 翻下一页：从 [cursor] 起逐个项目查，凑够 [pageSize] 条就停；没翻到的项目一次查询都不发。
     */
    suspend fun loadNextPage(): Page {
        if (!hasMore) return Page(exhausted = true)
        val added = ArrayList<Conversation>()
        val failed = ArrayList<String>()
        while (cursor < projects.size && added.size < pageSize) {
            val dir = projects[cursor].dirName
            cursor++ // 先前进：失败的项目也算翻过，否则一个坏项目把翻页卡死
            fetchCounts[dir] = (fetchCounts[dir] ?: 0) + 1
            val rows = fetch(dir)
            if (rows == null) {
                failed += dir
            } else {
                added += rows
            }
        }
        return Page(
            added = added.sortedByDescending { it.updatedAtMs },
            exhausted = !hasMore,
            failedProjects = failed,
        )
    }

    companion object {
        /**
         * 一页多少条。
         *
         * 30 是「一屏塞不满但滑两下就到底」的量级：太小会不停触发翻页（每页至少一个 SSH 往返），
         * 太大就退回全有或全无。它是下界不是上界。
         */
        const val DEFAULT_PAGE_SIZE = 30
    }
}
