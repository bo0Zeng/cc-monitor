package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.core.claude.catalog.ClaudeSessionCatalog
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import kotlinx.coroutines.flow.toList

/**
 * 一次批量投影取回一批会话的血统（`forkedFrom.sessionId`）。复用
 * [ClaudeSessionCatalog.titleProbeCommand]：一条 exec 里 `for f in …`，每个文件 `grep -a -m 1 -o`
 * 只回几十字节，血统随标题 / cwd 一起回来，没有新的往返。
 *
 * RS 标记行（`printf '\036%s\n' "$f"`）是无条件打的，所以一个文件的段出现了就证明命令跑到了它；
 * 段里没有血统片段 = 真的没有血统。
 *
 * 继承自那条探针的约束：
 * 1. 按 [ClaudeSessionCatalog.HISTORY_TITLE_CHUNK] 分片，单条命令探的文件数有上限。
 * 2. 命令里不许出现 `tail`：tail 管线按子串分派命令，会撞。
 * 3. `-o` 片段 + `cut` 上限，免得超大 `tool_result` 行整条流回手机。
 */
class DaemonHistoryReader(
    private val channel: RemoteCommandChannel,
) {
    /** `sid → 父 sid`（值 `null` = 确认过没有父）。探针失败的不进缓存，重连会重试。 */
    private val parentCache = HashMap<String, String?>()

    /**
     * 取一批会话的父 sid。只查还不知道的，按分片跑；返回本次新查到的父子对。
     *
     * @param pathsBySid 会话 id → 它的 JSONL 绝对路径（来自后端的 `session_added.path`）。
     */
    suspend fun parentsFor(pathsBySid: Map<String, String>): Map<String, String> {
        val todo = pathsBySid.filterKeys { it !in parentCache }
        if (todo.isEmpty()) return emptyMap()
        val sidByPath = todo.entries.associate { (sid, path) -> path to sid }
        val found = LinkedHashMap<String, String>()
        todo.values.toList().chunked(ClaudeSessionCatalog.HISTORY_TITLE_CHUNK).forEach { chunk ->
            val cmd = ClaudeSessionCatalog.titleProbeCommand(chunk) ?: return@forEach
            val raw = runCatching { collect(cmd) }.getOrNull() ?: return@forEach // 整片失败：不缓存，下次重试
            val parts = ClaudeSessionCatalog.parseTitleProbe(raw)
            // 没有某个文件的段 = 命令没跑到它 ⇒ 不缓存，别当成「没有血统」
            chunk.mapNotNull { path -> sidByPath[path]?.let { sid -> parts[path]?.let { sid to it } } }.forEach { (sid, probed) ->
                // 自指的 `forkedFrom` 出现在 uuid 没 remap 的分叉上 ⇒ 当没有父
                val parent = probed.forkedFrom?.takeIf { it.isNotBlank() && it != sid }
                parentCache[sid] = parent
                if (parent != null) found[sid] = parent
            }
        }
        return found
    }

    private suspend fun collect(command: String): String =
        channel
            .exec(command)
            .toList()
            .fold(ByteArray(0)) { acc, b -> acc + b }
            .decodeToString()
}
