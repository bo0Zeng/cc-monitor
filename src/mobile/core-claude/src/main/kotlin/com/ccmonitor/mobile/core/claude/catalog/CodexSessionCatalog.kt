package com.ccmonitor.mobile.core.claude.catalog

import com.ccmonitor.mobile.core.claude.model.CodexRecordParser
import com.ccmonitor.mobile.core.claude.transport.CodexPaths
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.ccmonitor.mobile.core.remote.shellQuote
import kotlinx.coroutines.CancellationException

/**
 * Codex CLI 的会话目录。与 [ClaudeSessionCatalog] 的差别：
 * - 发现按日期分区：`<codexDir>/sessions/年/月/日/rollout-*.jsonl*`，cwd 不进 glob，含 `.zst`；
 * - Codex 不写 pidfile，没有权威判活：全部 `live=false`，按 mtime 取最近的；
 * - sessionId 是文件名末尾的 UUID（[CodexPaths.sessionIdOf]）；
 * - 没有 title 记录，标题取首条真实用户消息（开场注入的上下文已标 isMeta，会被跳过）。
 *
 * 不按 cwd 过滤，也不解压 `.jsonl.zst`。
 */
class CodexSessionCatalog(
    private val channel: RemoteCommandChannel,
    private val codexDir: String,
) : SessionCatalog {
    override suspend fun scanLiveSessions(cwd: String?): LiveSessionScan {
        val rollouts = listRollouts()
        // 取 mtime 最新的前 N 条。authoritativeLiveness=false：调用方不能把「全不活」当成可信的零活会话。
        val refs =
            rollouts.take(ClaudeSessionCatalog.FALLBACK_MAX_SESSIONS).map { p ->
                SessionRef(path = p, sessionId = CodexPaths.sessionIdOf(p), live = false, pidfileName = null, status = null)
            }
        return LiveSessionScan(refs, probeOk = true, authoritativeLiveness = false)
    }

    override suspend fun liveSessionRefs(cwd: String?): List<SessionRef> = scanLiveSessions(cwd).refs

    override suspend fun labeledSessions(refs: List<SessionRef>): List<CatalogSession> {
        val titles = LinkedHashMap<String, String>()
        refs.map { it.path }.chunked(ClaudeSessionCatalog.HISTORY_TITLE_CHUNK).forEach { chunk ->
            // 一片探针失败只让这片回退 sid8；CancellationException 必须重抛，否则被取消的旧任务会拿陈旧结果覆盖新的。
            try {
                titles.putAll(codexTitlesFor(chunk))
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                // 该片标题回退 sid8
            }
        }
        return refs.map { r -> CatalogSession(r.path, r.sessionId, titles[r.path] ?: r.sessionId.take(8), live = false) }
    }

    /** `ls -t` 发现 rollout（最新在前、展开绝对路径；含 .jsonl 与 .jsonl.zst）。 */
    private suspend fun listRollouts(): List<String> =
        collectString("ls -t ${CodexPaths.sessionsGlob(codexDir)} 2>/dev/null")
            .lineSequence()
            .map { it.trim() }
            .filter { it.contains("/rollout-") } // 防御：只留 rollout 文件
            .toList()

    /**
     * 批量标题探针：每个 rollout 抓前 [USER_PROBE_LINES] 条 user 消息行，交给 [ClaudeSessionCatalog.firstUserExcerpt]。
     * 抓多条是为越过开场注入的消息。路径逐个 shell 引号；`cut -c` 限每行字节，防超大行。
     */
    private suspend fun codexTitlesFor(paths: List<String>): Map<String, String> {
        if (paths.isEmpty()) return emptyMap()
        val files = paths.joinToString(" ") { shellQuote(it) }
        val cmd =
            "for f in $files; do printf '\\036%s\\n' \"\$f\"; " +
                "grep -a -m $USER_PROBE_LINES '\"role\":\"user\"' -- \"\$f\" 2>/dev/null | cut -c -${ClaudeSessionCatalog.PROBE_LINE_CAP}; done"
        return parseCodexTitleProbe(collectString(cmd))
    }

    /** 解析探针输出：按 RS 切段，段首行是路径，其余是 user 消息的 JSONL 行。 */
    private fun parseCodexTitleProbe(raw: String): Map<String, String> {
        val out = LinkedHashMap<String, String>()
        raw.split(ClaudeSessionCatalog.CHUNK_MARKER).drop(1).forEach { chunk ->
            val nl = chunk.indexOf('\n')
            if (nl < 0) return@forEach
            val path = chunk.substring(0, nl).trim()
            if (path.isEmpty()) return@forEach
            val records =
                chunk
                    .substring(nl + 1)
                    .lineSequence()
                    .filter { it.isNotBlank() }
                    .map { CodexRecordParser.parse(it) }
                    .toList()
            ClaudeSessionCatalog.firstUserExcerpt(records)?.let { out[path] = it }
        }
        return out
    }

    private suspend fun collectString(command: String): String {
        val buf = java.io.ByteArrayOutputStream()
        channel.exec(command).collect { buf.write(it) }
        return buf.toString("UTF-8")
    }

    companion object {
        /** 标题探针每文件抓多少条 user 消息；开场注入至多几条。 */
        const val USER_PROBE_LINES = 12
    }
}
