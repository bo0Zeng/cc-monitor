package com.ccmonitor.mobile.core.claude.catalog

/**
 * 会话目录：按 agent 种类实现的会话发现、活动判定和可读标题。
 * - [ClaudeSessionCatalog]：按 cwd 编码的目录 glob，pidfile 判活，标题取 title 记录。
 * - [CodexSessionCatalog]：按日期分区 glob，无 pidfile 只按 mtime 排，标题取首条真实用户消息。
 */
interface SessionCatalog {
    /** 活动集扫描（含探测可信度 [LiveSessionScan.probeOk]）：列哪些会话 + 谁活动。cwd=null → 全部。 */
    suspend fun scanLiveSessions(cwd: String?): LiveSessionScan

    /** 快路径：活动会话优先的会话引用（不含标题探针）。= [scanLiveSessions].refs。 */
    suspend fun liveSessionRefs(cwd: String?): List<SessionRef>

    /** 慢路径：给一组 refs 取可读标题（探针失败降级 sid8）。 */
    suspend fun labeledSessions(refs: List<SessionRef>): List<CatalogSession>
}
