package com.ccmonitor.mobile.core.claude.catalog

import com.ccmonitor.mobile.core.claude.model.ContentBlock
import com.ccmonitor.mobile.core.claude.model.JsonlParser
import com.ccmonitor.mobile.core.claude.model.JsonlRecord
import com.ccmonitor.mobile.core.claude.model.isCompactSummary
import com.ccmonitor.mobile.core.claude.model.parseBashInput
import com.ccmonitor.mobile.core.claude.model.parseBashOutput
import com.ccmonitor.mobile.core.claude.model.parseSlashCommand
import com.ccmonitor.mobile.core.claude.model.stripInternalNoise
import com.ccmonitor.mobile.core.claude.transport.ClaudePaths
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.ccmonitor.mobile.core.remote.shellQuote
import com.squareup.moshi.Moshi
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow

/*
 * Claude 会话目录。每个活的 Claude Code 进程写一份 pidfile `<claudeDir>/sessions/<PID>.json`
 * （{pid, sessionId, cwd, procStart?, name?…}）。会话「活动」= pidfile 的 pid 是活进程且进程身份一致，不是最新 mtime。
 * 标题回退链：customTitle ?? aiTitle ?? firstUserExcerpt ?? sessionId[:8]。
 *
 * PID 复用防护：死 pidfile 长年不清，PID 回绕后会被存在性探测误判为活、把死会话钉住。所以 pidfile 带 procStart 时，
 * 还要 `/proc/<pid>/stat` 第 22 字段（starttime）与它一致才算活；没有 procStart 时只看存在性。
 *
 * 没有 pidfile 或没有活会话时，退回最近 mtime 的 jsonl 列表（截前 N 条）。
 * 历史分组见 [historyByProject]：按 JSONL 里真实的 `User.cwd` 分组；`projects/` 下的编码目录名有损，不反解。
 *
 * 注入安全：pid 是 Long 才进 `kill -0`；jsonl 路径取自 `ls` 输出、逐个 shell 引号；
 * sessionId 只做字符串比对，不拼路径、不进命令；pidfile cwd 只经 projectDirName 归一后比对。
 */

/** 一份已解析的 pidfile。[name] 是 Claude 写的 AI 标题（可缺）；[procStart] 是进程 starttime 令牌（可缺）。 */
data class SessionPidfile(
    val pid: Long,
    val sessionId: String,
    val cwd: String?,
    val name: String?,
    val procStart: String? = null,
    // 会话状态字段，只在状态转换时重写。
    val kind: String? = null, // interactive / bg（缺省按 interactive）
    val status: String? = null, // busy / idle / waiting / shell（缺省或未知显示中性）
    val waitingFor: String? = null, // waiting 的子类：permission / dialog / input …
    val statusUpdatedAt: String? = null, // 状态转换时间戳（可缺）
)

/** 会话引用，不含标题探针：tail 目标即刻可选，标题异步补。 */
data class SessionRef(
    val path: String,
    val sessionId: String,
    val live: Boolean,
    val pidfileName: String?,
    val status: String? = null, // 活会话 pidfile 的 status；null = 不活或未写
) {
    /** 取到标题前的占位名：pidfile name ?? sid8。 */
    val provisionalTitle: String get() = ClaudeSessionCatalog.readableTitle(null, pidfileName, sessionId)
}

/** 目录条目：一个可展示、可 tail 的会话。[path] 是 `ls` 输出的展开绝对路径。 */
data class CatalogSession(
    val path: String,
    val sessionId: String,
    val title: String,
    val live: Boolean,
)

/**
 * 活动集扫描结果，带存活探测是否可信的标记。
 * [probeOk]=false：pidfile / `/proc` 探测失败，[refs] 的 `live` 不可信（可能只是探测瞬时失败）。
 * 调用方只在 probeOk 时才撤 tail，否则一次探测抖动会误杀活 tail、令其从 offset 0 全量重放。
 * probeOk=true 时 `refs` 要么全 live=true，要么全 live=false（可信的零活会话，兜底最新 mtime 列表）。
 */
data class LiveSessionScan(
    val refs: List<SessionRef>,
    val probeOk: Boolean,
    // 判活是否权威。Claude 是（pidfile + /proc 身份）；Codex 不是（只有 mtime，分不出活死），
    // 这时调用方不能把「全不活」当可信的零活会话来收敛，应 tail 最近的前 N 条。
    val authoritativeLiveness: Boolean = true,
)

/** 标题素材（每个 jsonl）：最后一条 custom-title、最后一条 ai-title、首条真实用户消息摘要、项目 cwd。 */
data class TitleParts(
    val custom: String?,
    val ai: String?,
    val excerpt: String?,
    val cwd: String? = null,
    /**
     * 该会话从哪个 sid 分叉出来（`forkedFrom.sessionId`）；null = 不是分叉。
     * 和 [cwd] 一样不是标题：这个探针每文件抓一小段投影，一条命令抓 N 个，省掉再起一条命令的往返。
     */
    val forkedFrom: String? = null,
)

/** 带 mtime 的 jsonl 路径。[mtimeEpochSec] null = stat 不可用，只有 `ls -t` 的顺序。 */
data class TimedPath(
    val path: String,
    val mtimeEpochSec: Long?,
)

/**
 * 一条历史会话。
 * [cwd] 是从 JSONL 真解出的工作目录（从同目录邻居收养的 cwd 不写进来，只用于分组）；null 不妨碍 resume，
 * resume 时远端按 sid 定位 jsonl 读权威 cwd。
 * [live] 表示当前有活 Claude 进程：界面显示「活动中」并禁止 resume——对活会话再起 `--resume`
 * 会让两个进程写同一份 JSONL。
 */
data class HistorySession(
    val path: String,
    val sessionId: String,
    val title: String,
    val cwd: String?,
    val mtimeEpochSec: Long?,
    val live: Boolean = false,
)

/**
 * 一个项目的历史会话组。[cwd] 是真实项目目录（null = 组内都还没解出 cwd）；
 * [label] 是展示名（cwd 尾段；未知时用 `projects/` 编码目录名，有损、只展示）。[sessions] 最近在前。
 */
data class ProjectHistory(
    val cwd: String?,
    val label: String,
    val sessions: List<HistorySession>,
)

class ClaudeSessionCatalog(
    private val channel: RemoteCommandChannel,
    private val claudeDir: String,
) : SessionCatalog {
    /** 当前项目（[cwd]，null=全项目）的活动会话，含标题。一次取全，给测试和一次性场景用。 */
    suspend fun liveSessions(cwd: String?): List<CatalogSession> = labeledSessions(liveSessionRefs(cwd))

    /**
     * 快路径：只算列哪些会话、谁活动，不取标题。活动会话（pidfile 判活 + cwd 匹配）优先；
     * 无活会话时兜底最近 mtime 的前 [FALLBACK_MAX_SESSIONS] 条。
     */
    override suspend fun liveSessionRefs(cwd: String?): List<SessionRef> = scanLiveSessions(cwd).refs

    /**
     * [liveSessionRefs] 的带探测可信度版本：探测失败显式暴露为 [LiveSessionScan.probeOk]=false，
     * 不和「真无活会话」混在一起。
     * jsonl 列表命令失败照常抛，连接不可用也算（无连接时 exec 流抛 IOException）；
     * 不能折叠成可信空集，否则调用方会撤光活 tail、阅读面误报「未找到 Claude 会话」。
     */
    override suspend fun scanLiveSessions(cwd: String?): LiveSessionScan {
        val jsonls = listJsonl(cwd)
        if (jsonls.isEmpty()) return LiveSessionScan(emptyList(), probeOk = true) // ls 成功、无 jsonl
        var probeOk = true
        val livePidfiles =
            try {
                fetchLivePidfiles()
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                probeOk = false
                emptyList()
            }.filter { cwdMatches(it.cwd, cwd) }
        // 按 sid 分组、每 sid 选当前 pidfile；只剩 bg 的 sid 不算活。
        val bySid =
            livePidfiles
                .groupBy { it.sessionId }
                .mapNotNull { (sid, group) -> resolveActivePidfile(group)?.let { sid to it } }
                .toMap()
        val livePaths = jsonls.filter { sessionIdOf(it) in bySid }
        // 没有活动会话（pidfile 目录常年积累尸体文件，Claude 没在跑时是常态）→ 兜底 mtime 列表。
        val chosen = livePaths.ifEmpty { jsonls.take(FALLBACK_MAX_SESSIONS) }
        val refs =
            chosen.map { p ->
                val sid = sessionIdOf(p)
                SessionRef(path = p, sessionId = sid, live = sid in bySid, pidfileName = bySid[sid]?.name, status = bySid[sid]?.status)
            }
        return LiveSessionScan(refs, probeOk)
    }

    /**
     * 慢路径：给一组 [refs] 取标题探针、套回退链。探针按 [HISTORY_TITLE_CHUNK] 分片跑，免得命令行过长；
     * 单片失败只让这片降级。
     */
    override suspend fun labeledSessions(refs: List<SessionRef>): List<CatalogSession> {
        val parts = LinkedHashMap<String, TitleParts>()
        refs.map { it.path }.chunked(HISTORY_TITLE_CHUNK).forEach { chunk ->
            parts.putAll(degradeToDefault(emptyMap()) { titlesFor(chunk) })
        }
        return refs.map { r -> CatalogSession(r.path, r.sessionId, readableTitle(parts[r.path], r.pidfileName, r.sessionId), r.live) }
    }

    /** 项目 jsonl 列表，最近修改在前，展开绝对路径。cwd=null → 全项目。 */
    suspend fun listJsonl(cwd: String?): List<String> =
        collectString("ls -t ${ClaudePaths.projectsGlob(claudeDir, cwd)} 2>/dev/null")
            .lineSequence()
            .map { it.trim() }
            .filter { it.endsWith(".jsonl") }
            .toList()

    /** 读全部 pidfile，留下进程仍是原会话的（pid 存在 + procStart 一致）。 */
    suspend fun fetchLivePidfiles(): List<SessionPidfile> {
        val all = parsePidfiles(collectString(pidfilesCommand(claudeDir)))
        if (all.isEmpty()) return emptyList()
        val probeCmd = livenessProbeCommand(all.map { it.pid }) ?: return emptyList()
        val probe = parseProbe(collectString(probeCmd))
        return filterAliveByIdentity(all, probe)
    }

    /** 批量标题探针：一条远端命令抓每个 jsonl 的标题记录和前几条 user 行。 */
    suspend fun titlesFor(paths: List<String>): Map<String, TitleParts> {
        val cmd = titleProbeCommand(paths) ?: return emptyMap()
        return parseTitleProbe(collectString(cmd))
    }

    /**
     * jsonl 列表 + mtime，最近在前。用 `stat -c '%Y %n'` 一条命令同时拿顺序与时间；
     * stat 不可用时退回 [listJsonl]（有序无时间，界面隐藏相对时间）。
     */
    suspend fun listJsonlWithTimes(cwd: String?): List<TimedPath> {
        val timed = parseStatTimes(degradeToDefault("") { collectString(statTimesCommand(claudeDir, cwd)) })
        if (timed.isNotEmpty()) return timed
        return listJsonl(cwd).map { TimedPath(it, null) }
    }

    /**
     * 全项目历史会话，按项目分组。
     *
     * 渐进发出：先发一版骨架（标题=sid8、按编码目录分组），再按 [HISTORY_TITLE_CHUNK] 分片补标题和真实 cwd，
     * 每片完成重新分组再发。单片探针失败只跳过，列表失败照常抛。最多取 [limit] 条最近会话。
     */
    fun historyByProject(limit: Int = HISTORY_MAX_SESSIONS): Flow<List<ProjectHistory>> =
        flow {
            val listed = listJsonlWithTimes(null).take(limit)
            // 标出活会话，界面禁止对它 resume。探测失败只降级为不标。
            val liveSids = degradeToDefault(emptySet<String>()) { fetchLivePidfiles().map { it.sessionId }.toSet() }
            val parts = LinkedHashMap<String, TitleParts>()
            emit(groupHistory(listed, parts, liveSids)) // 骨架先发，空列表也发，界面据此显示「无历史」
            listed.map { it.path }.chunked(HISTORY_TITLE_CHUNK).forEach { chunk ->
                parts.putAll(degradeToDefault(emptyMap()) { titlesFor(chunk) })
                emit(groupHistory(listed, parts, liveSids))
            }
        }

    /** exec 并缓冲全部 stdout，最后一次性按 UTF-8 解码，免得多字节字符跨块截断。 */
    private suspend fun collectString(command: String): String {
        val buf = java.io.ByteArrayOutputStream()
        channel.exec(command).collect { buf.write(it) }
        return buf.toString("UTF-8")
    }

    /** 取消安全的 runCatching：失败返回 [default]，CancellationException 重抛。 */
    private inline fun <T> degradeToDefault(
        default: T,
        block: () -> T,
    ): T =
        try {
            block()
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            default
        }

    companion object {
        /** 兜底（无活动会话）时最多列的会话数；完整历史在历史页。 */
        const val FALLBACK_MAX_SESSIONS = 15

        /** 历史页最多列的会话数（最近优先）。 */
        const val HISTORY_MAX_SESSIONS = 200

        /** 标题探针分片大小：单条命令最多带这么多文件。 */
        const val HISTORY_TITLE_CHUNK = 15

        /** firstUserExcerpt 截断长度。 */
        const val EXCERPT_MAX_CHARS = 120

        /** 探针命令每行的服务端字节上限，免得超大 tool_result 整行流回手机。 */
        const val PROBE_LINE_CAP = 8192

        /** pidfile 每文件字节上限，免得误放的大文件整份流回。 */
        const val PIDFILE_BYTE_CAP = 4096

        /** 批量命令里分隔各文件段的标记（ASCII RS 0x1E，JSONL 行首不会出现）。 */
        const val CHUNK_MARKER = '\u001E'

        private val moshi = Moshi.Builder().build()
        private val anyAdapter = moshi.adapter(Any::class.java)
        private val whitespaceRun = Regex("\\s+")

        /**
         * pidfile 读取命令。[claudeDir] 前缀经 quotedClaudeDirPrefix 引号（含空格也不会 word-split），glob 留在引号外。
         * 每份 pidfile 前吐一行 RS 标记；`head -c` 限每文件字节。
         */
        fun pidfilesCommand(claudeDir: String): String =
            "for f in ${ClaudePaths.quotedClaudeDirPrefix(claudeDir)}/sessions/*.json; do [ -f \"\$f\" ] || continue; " +
                "printf '\\036%s\\n' \"\$f\"; head -c $PIDFILE_BYTE_CAP \"\$f\"; printf '\\n'; done 2>/dev/null"

        /** 解析 [pidfilesCommand] 输出：按 RS 切段，每段是标记行加一份 JSON；坏段、缺 pid、缺 sessionId 跳过。 */
        fun parsePidfiles(raw: String): List<SessionPidfile> =
            raw.split(CHUNK_MARKER).drop(1).mapNotNull { chunk ->
                val json = chunk.substringAfter('\n', "").trim()
                if (json.isEmpty()) return@mapNotNull null
                val m = runCatching { anyAdapter.fromJson(json) as? Map<*, *> }.getOrNull() ?: return@mapNotNull null
                val pid = (m["pid"] as? Number)?.toLong()?.takeIf { it > 0 } ?: return@mapNotNull null
                val sid = (m["sessionId"] as? String)?.trim()?.takeIf { it.isNotEmpty() } ?: return@mapNotNull null
                val procStart =
                    when (val v = m["procStart"]) {
                        is String -> v.trim().ifEmpty { null }
                        is Number -> v.toLong().toString() // starttime 也可能写成 JSON 数字
                        else -> null
                    }
                SessionPidfile(
                    pid,
                    sid,
                    m["cwd"] as? String,
                    m["name"] as? String,
                    procStart,
                    kind = m["kind"] as? String,
                    status = m["status"] as? String,
                    waitingFor = m["waitingFor"] as? String,
                    statusUpdatedAt = m["statusUpdatedAt"] as? String,
                )
            }

        /**
         * 存活探测命令；没有合法 pid → null。每个存活 pid 吐 `pid starttime`。
         * 先看 `/proc/<pid>`：`kill -0` 对别的用户的进程报 EPERM，会误判死。
         * starttime 是 `/proc/<pid>/stat` 第 22 字段；用 `-F')'` 取末段再拆，避开 comm 里的空格和括号。
         */
        fun livenessProbeCommand(pids: Collection<Long>): String? {
            val safe = pids.filter { it > 0 }.distinct()
            if (safe.isEmpty()) return null
            return "for p in ${safe.joinToString(" ")}; do " +
                "if [ -e \"/proc/\$p\" ] || kill -0 \"\$p\" 2>/dev/null; then " +
                "st=\$(awk -F')' '{split(\$NF,a,\" \");print a[20]}' \"/proc/\$p/stat\" 2>/dev/null); " +
                "echo \"\$p \$st\"; fi; done"
        }

        /** 解析 [livenessProbeCommand] 输出：每行 `pid [starttime]`，没有 starttime 段记空串。 */
        fun parseProbe(raw: String): Map<Long, String> {
            val out = LinkedHashMap<Long, String>()
            raw.lineSequence().forEach { line ->
                val t = line.trim()
                if (t.isEmpty()) return@forEach
                val sp = t.indexOf(' ')
                val pid = (if (sp < 0) t else t.substring(0, sp)).toLongOrNull() ?: return@forEach
                out[pid] = if (sp < 0) "" else t.substring(sp + 1).trim()
            }
            return out
        }

        /**
         * PID 复用防护：pid 须在存活集里；pidfile 带 procStart 时还须与探到的 starttime 逐字一致。
         * 有 procStart 而 starttime 读不到 → 判死，宁可漏也不把复用 PID 的死会话钉住。
         */
        fun filterAliveByIdentity(
            pidfiles: List<SessionPidfile>,
            probe: Map<Long, String>,
        ): List<SessionPidfile> =
            pidfiles.filter { pf ->
                val starttime = probe[pf.pid] ?: return@filter false
                pf.procStart.isNullOrBlank() || starttime == pf.procStart
            }

        /**
         * 批量标题探针命令；空列表 → null。每文件输出：RS 标记行、最后一条 custom-title、最后一条 ai-title、
         * 前 8 条 user 行（compact 续接会把首条真实输入推后），以及 cwd 和 forkedFrom 两个 `grep -o` 片段。
         * 组内输出经 `cut -c -N` 限每行字符数，标记行不经 cut。
         *
         * 片段用 `-o` 只回几十字节：不受 `cut` 截断（超大 user 行截成半个 JSON 就解不出 cwd），
         * 也不占 `-m 8` 的预算（前 8 条 user 可能全是 tool_result）。cwd 片段只作兜底，完整 User 记录的 cwd 优先。
         *
         * 取最后一条用 `sed -n '$p'` 而非 `tail -n 1`：测试替身按 `tail` 子串分派命令。
         */
        fun titleProbeCommand(paths: List<String>): String? {
            if (paths.isEmpty()) return null
            val files = paths.joinToString(" ") { shellQuote(it) }
            return "for f in $files; do printf '\\036%s\\n' \"\$f\"; " +
                "{ grep -a '\"type\":\"custom-title\"' -- \"\$f\" 2>/dev/null | sed -n '\$p'; " +
                "grep -a '\"type\":\"ai-title\"' -- \"\$f\" 2>/dev/null | sed -n '\$p'; " +
                "grep -a -m 8 '\"type\":\"user\"' -- \"\$f\" 2>/dev/null; " +
                "grep -a -m 1 -o '\"cwd\":\"[^\"]*\"' -- \"\$f\" 2>/dev/null; " +
                // forkedFrom 可能写在任意一条记录上，首行常是 mode / ai-title，所以 grep 整个文件取第一处。
                "grep -a -m 1 -o '\"forkedFrom\":{[^}]*}' -- \"\$f\" 2>/dev/null; } | cut -c -$PROBE_LINE_CAP; done"
        }

        /** cwd 片段行（`grep -o` 输出的 `"cwd":"…"`）。整行精确匹配，不把正文行误当片段。 */
        private val CWD_FRAGMENT_RE = Regex("^\"cwd\":\"([^\"]*)\"$")

        /** forkedFrom 片段行（`grep -o` 输出的 `"forkedFrom":{…}`）。整行精确匹配。 */
        private val FORKED_FROM_FRAGMENT_RE = Regex("""^"forkedFrom":\{[^}]*"sessionId":"([^"]+)"[^}]*\}$""")

        /**
         * 解析 [titleProbeCommand] 输出：按 RS 切段，段首行是路径，其余是 JSONL 行或片段行。
         * 片段行的 cwd 只在完整 User 记录没解出 cwd 时兜底，正文里的伪 cwd 抢不了位。
         */
        fun parseTitleProbe(raw: String): Map<String, TitleParts> {
            val out = LinkedHashMap<String, TitleParts>()
            raw.split(CHUNK_MARKER).drop(1).forEach { chunk ->
                val nl = chunk.indexOf('\n')
                if (nl < 0) return@forEach
                val path = chunk.substring(0, nl).trim()
                if (path.isEmpty()) return@forEach
                var fragmentCwd: String? = null
                var fragmentFork: String? = null
                val records =
                    chunk
                        .substring(nl + 1)
                        .lineSequence()
                        .filter { it.isNotBlank() }
                        .mapNotNull { line ->
                            val trimmed = line.trim()
                            val frag = CWD_FRAGMENT_RE.matchEntire(trimmed)
                            val fork = FORKED_FROM_FRAGMENT_RE.matchEntire(trimmed)
                            if (frag != null) {
                                if (fragmentCwd == null) fragmentCwd = frag.groupValues[1].ifBlank { null }
                                null // 片段行不是 JSONL 记录
                            } else if (fork != null) {
                                if (fragmentFork == null) fragmentFork = fork.groupValues[1].ifBlank { null }
                                null
                            } else {
                                JsonlParser.parse(line)
                            }
                        }.toList()
                val parts = titleParts(records).copy(forkedFrom = fragmentFork)
                out[path] = if (parts.cwd != null) parts else parts.copy(cwd = fragmentCwd)
            }
            return out
        }

        /** 从记录里提标题素材：custom / ai 各取最后一条非空；摘要走 [firstUserExcerpt]；cwd 取首条 User.cwd。 */
        fun titleParts(records: List<JsonlRecord>): TitleParts {
            var custom: String? = null
            var ai: String? = null
            records.filterIsInstance<JsonlRecord.Title>().forEach { t ->
                val v = t.title.trim().ifEmpty { null } ?: return@forEach
                if (t.custom) custom = v else ai = v
            }
            val cwd =
                records
                    .asSequence()
                    .filterIsInstance<JsonlRecord.User>()
                    .mapNotNull { it.cwd?.takeIf { c -> c.isNotBlank() } }
                    .firstOrNull()
            return TitleParts(custom, ai, firstUserExcerpt(records), cwd)
        }

        /**
         * 可读标题回退链：customTitle ?? aiTitle ?? pidfile name ?? firstUserExcerpt ?? sessionId[:8]。
         * 各段 trim 后空白视为缺。
         */
        fun readableTitle(
            parts: TitleParts?,
            pidfileName: String?,
            sessionId: String,
        ): String =
            parts?.custom?.trim()?.ifBlank { null }
                ?: parts?.ai?.trim()?.ifBlank { null }
                ?: pidfileName?.trim()?.ifBlank { null }
                ?: parts?.excerpt?.trim()?.ifBlank { null }
                ?: sessionId.take(8)

        /**
         * 首条真实用户消息的摘要。真实性判定复用 [stripInternalNoise]：非 meta、非侧链，剥掉 CLI 包装后非空，
         * 不是 /compact 摘要，不是 bash 输出。斜杠命令显示 `/name args`，`!bash` 显示 `!cmd`。
         * 空白折叠成单行，截到 [EXCERPT_MAX_CHARS]。
         */
        fun firstUserExcerpt(records: List<JsonlRecord>): String? =
            records
                .asSequence()
                .filterIsInstance<JsonlRecord.User>()
                .filter { !it.isMeta && !it.isSidechain }
                .mapNotNull { u ->
                    // 只看 Text 块；tool_result 没有 Text 块，自然跳过。
                    val text = u.blocks.filterIsInstance<ContentBlock.Text>().joinToString("\n") { it.text }
                    val stripped = stripInternalNoise(text)
                    when {
                        stripped.isEmpty() -> null
                        isCompactSummary(stripped) -> null
                        parseBashOutput(stripped) != null -> null // bash stdout 回显不是用户输入
                        else -> {
                            val slash = parseSlashCommand(stripped)
                            val bash = parseBashInput(stripped)
                            val line =
                                when {
                                    slash != null -> "${slash.name} ${slash.args}".trim()
                                    bash != null -> "!${bash.command}"
                                    else -> stripped
                                }
                            truncateChars(whitespaceRun.replace(line, " ").trim(), EXCERPT_MAX_CHARS).ifEmpty { null }
                        }
                    }
                }.firstOrNull()

        /** 截断到 [max] 字符加省略号；切点劈开代理对时退一位。 */
        fun truncateChars(
            s: String,
            max: Int,
        ): String {
            if (s.length <= max) return s
            var cut = max
            if (Character.isHighSurrogate(s[cut - 1])) cut -= 1
            return s.substring(0, cut) + "…"
        }

        /**
         * pidfile cwd 是否匹配阅读面项目：去尾 `/` 后经 [ClaudePaths.projectDirName] 归一比对；
         * 任一侧缺省 → 放行（项目 glob 已过滤）。
         */
        fun cwdMatches(
            pidCwd: String?,
            readerCwd: String?,
        ): Boolean {
            if (pidCwd.isNullOrBlank() || readerCwd.isNullOrBlank()) return true

            fun norm(s: String): String = s.trimEnd('/').ifEmpty { "/" }
            return ClaudePaths.projectDirName(norm(pidCwd)) == ClaudePaths.projectDirName(norm(readerCwd))
        }

        /** jsonl 路径 → sessionId（文件名去 .jsonl）。 */
        fun sessionIdOf(path: String): String = path.substringAfterLast('/').removeSuffix(".jsonl")

        /**
         * 批量 mtime 列表命令：`stat -c '%Y %n'` 每文件吐 `epoch 绝对路径`，`sort -rn` 得最近在前。
         * 无匹配时 stat 报错被吞、输出为空，调用方退回 ls。
         *
         * 注意：测试替身按命令前缀和子串分派，这条不得以 `ls `、`wc -c`、`for ` 开头，
         * 不得含 `tail -`、pidfile 目录 glob、`LC_ALL=C awk`。
         */
        fun statTimesCommand(
            claudeDir: String,
            cwd: String?,
        ): String = "stat -c '%Y %n' -- ${ClaudePaths.projectsGlob(claudeDir, cwd)} 2>/dev/null | sort -rn"

        /** 解析 [statTimesCommand] 输出：每行 `epoch 路径`（路径可含空格）；坏行和非 .jsonl 跳过。 */
        fun parseStatTimes(raw: String): List<TimedPath> =
            raw
                .lineSequence()
                .mapNotNull { line ->
                    val t = line.trim()
                    if (t.isEmpty()) return@mapNotNull null
                    val sp = t.indexOf(' ')
                    if (sp <= 0) return@mapNotNull null
                    val epoch = t.substring(0, sp).toLongOrNull() ?: return@mapNotNull null
                    val path = t.substring(sp + 1).trim()
                    if (!path.endsWith(".jsonl")) return@mapNotNull null
                    TimedPath(path, epoch)
                }.toList()

        /** jsonl 路径 → `projects/` 下的编码项目目录名。有损（cwd 里非字母数字都成 `-`），只作展示和兜底键。 */
        fun encodedProjectDir(path: String): String = path.substringBeforeLast('/', "").substringAfterLast('/')

        /** 项目展示名：真实 [cwd] 尾段（根显示 `/`）；cwd 未知 → 编码目录名原样。 */
        fun projectLabel(
            cwd: String?,
            encDir: String,
        ): String =
            when {
                cwd.isNullOrBlank() -> encDir.ifEmpty { "(未知项目)" }
                cwd.trimEnd('/').isEmpty() -> "/"
                else -> cwd.trimEnd('/').substringAfterLast('/')
            }

        /** [groupHistory] 的组累加器：组的 cwd 和编码目录在首见时定下，会话按 listed 序追加。 */
        private class HistoryGroupAcc(
            val cwd: String?,
            val encDir: String,
            val sessions: MutableList<HistorySession> = mutableListOf(),
        )

        /**
         * 纯分组：[listed]（最近在前）+ 已到手的标题素材 [parts] + 活会话 id 集 [liveSids] → 按项目分组的历史。
         *
         * 分组键是 JSONL 真实 cwd。没解出 cwd 的会话，看同编码目录里有没有已解出的（编码目录是 cwd 的确定函数，
         * 同目录即同项目）就收养它作组键；还没有就用编码目录名成组。收养的 cwd 只用于分组和展示，
         * 不写进 [HistorySession.cwd]：编码有损，邻居 cwd 可能错配。
         *
         * 键空间用 `cwd:` / `enc:` 前缀隔开：cwd 来自 JSONL、是可写文本，不隔开的话构造出的 cwd 能撞上编码兜底组。
         */
        fun groupHistory(
            listed: List<TimedPath>,
            parts: Map<String, TitleParts>,
            liveSids: Set<String> = emptySet(),
        ): List<ProjectHistory> {
            val encToCwd = HashMap<String, String>()
            listed.forEach { tp ->
                val c = parts[tp.path]?.cwd?.takeIf { it.isNotBlank() } ?: return@forEach
                encToCwd.putIfAbsent(encodedProjectDir(tp.path), c)
            }
            val groups = LinkedHashMap<String, HistoryGroupAcc>()
            listed.forEach { tp ->
                val sid = sessionIdOf(tp.path)
                val titleParts = parts[tp.path]
                val enc = encodedProjectDir(tp.path)
                val trueCwd = titleParts?.cwd?.takeIf { it.isNotBlank() } // 真解出的才写进会话
                val groupCwd = trueCwd ?: encToCwd[enc] // 收养的只进组键
                val key = if (groupCwd != null) "cwd:$groupCwd" else "enc:$enc"
                val acc = groups.getOrPut(key) { HistoryGroupAcc(groupCwd, enc) }
                acc.sessions.add(
                    HistorySession(tp.path, sid, readableTitle(titleParts, null, sid), trueCwd, tp.mtimeEpochSec, sid in liveSids),
                )
            }
            return groups.values.map { ProjectHistory(it.cwd, projectLabel(it.cwd, it.encDir), it.sessions.toList()) }
        }
    }
}
