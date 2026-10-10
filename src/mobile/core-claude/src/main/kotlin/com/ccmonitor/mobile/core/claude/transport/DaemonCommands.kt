package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.core.remote.shellQuote

/**
 * cc-monitor 后端的远端命令构造。拼后端命令只许在这里。
 *
 * 两种模式，输出形状不同，别混：
 *
 * | 模式 | 命令形 | 输出 | 谁解析 |
 * |---|---|---|---|
 * | 流模式 | `<path> -- --stream [--with-bg] [--tail-only]` | wire 帧（每行带 `kind`） | [DaemonTransport] |
 * | 一次性查询 | `<path> -- --list-projects` 等 | 每行一个裸 JSON 对象，没有 `kind` | [parseQueryLines] |
 *
 * 把查询输出喂给 [DaemonTransport] 会得到 0 帧且不报错：流解析器静默跳过未知 kind。
 *
 * 后端的 argv 分派：
 * - 先看 argv0：二进制被叫成一段配置的名字（不是 `ccm` / `cc-monitor-backend` 打头、又是合法配置名）
 *   ⇒ 整行交给 claude，参数一个都不进后端。注意：没有连字符又不叫 `ccm` 的路径名（如 `backend`）
 *   会落进这一支，表现与「没装」一样，因为永远等不到 hello。
 * - 再看 `argv[1] == "--"` 且 `argv[2]` 是后端词（子命令或流 flag）⇒ 进后端。
 * - 其余整行交给 claude；零参数 ⇒ 起一个 claude。
 *
 * 所以流模式必须显式发 `-- --stream`，一次性查询必须发 `-- <子命令>`。[ARGV_END] 是那道门，不是修饰。
 * 载体不由 argv 决定：stdio 管道喂 `-- --stream` 仍是 hello ＋ JSONL。
 *
 * 能力协商：只发对端声明过的 flag。`capabilities` 缺席 = 空集 = 一个 flag 都不发。
 * 发了对端剥不掉的 flag ⇒ 剩余参数非空 ⇒ 落进一次性查询 ⇒ `unknown argument`、exit 2、不发 hello
 * ⇒ 客户端重连 ⇒ 再 exit 2，日志里什么都没有。
 */
object DaemonCommands {
    /**
     * argv 分派门，逐字是 `--`。必须是二进制路径之后的第一个词：
     * 挪到子命令后面或省掉，整行都交给 claude；一个词都不剩时 claude 会被真的起起来。
     */
    const val ARGV_END: String = "--"

    /** 流模式的后端词，即 `argv[2]`。不发它，整行照样交给 claude。 */
    const val STREAM_FLAG: String = "--stream"

    /** 流模式可用的 flag —— 必须与 `capabilities` token 同名（后端按 token 剥离）。 */
    const val CAP_BG: String = "bg"
    const val CAP_TAIL_ONLY: String = "tail-only"

    private val FLAG_OF = mapOf(CAP_BG to "--with-bg", CAP_TAIL_ONLY to "--tail-only")

    /**
     * 「要 `line` 帧上的原文」那个词。它不是 capability token：后端的 `STREAM_CAPABILITIES`
     * 只有 `bg` 和 `tail-only`，但流 flag 表里有它。所以它不能走 `capabilities` 交集，
     * 否则对端永远不声明它、它恒被静默丢掉。
     *
     * 不认识它的后端会把未知 `--flag` 警告后忽略、照常进流模式发 hello，所以无条件发是安全的。
     *
     * @see HARD_FLAG_OF
     */
    const val WANT_RAW: String = "with-raw"

    /**
     * 不进 `capabilities` 交集的 flag 表，只看 [stream] 的 `want`。
     * 与 [FLAG_OF] 分开是结构上的：混成一张表就会被交集静默丢掉。
     *
     * 目前没有生产 `want` 要它：`line` 帧暂无消费方，要来的 `raw` 没人读却按行多一份流量。
     * 需要 `line` 原文时，往 `want` 里加 [WANT_RAW]，并同时处置 [JsonlFrame.Line.raw] 的可空性。
     */
    private val HARD_FLAG_OF = mapOf(WANT_RAW to "--with" + "-raw")

    /**
     * 流模式命令：`<quote 过的路径> -- --stream [--with-bg] [--tail-only]`。
     *
     * 能力 flag 只取 [want] 与对端 [capabilities] 的交集；[HARD_FLAG_OF] 里的词只看 [want]。
     * `-- --stream` 不可省：裸路径的含义是「起一个 claude」。
     *
     * @param capabilities 对端 `hello.capabilities`（缺席时传空集）
     * @param want 想要的能力；对端没声明的静默降级（旧后端是正常情况）
     */
    fun stream(
        daemonPath: String,
        capabilities: Collection<String> = emptyList(),
        want: Collection<String> = emptyList(),
    ): String =
        buildString {
            // 路径 → 分派门 → 后端词，顺序就是后端读 argv 的顺序。
            append(shellQuote(daemonPath))
            append(' ').append(ARGV_END)
            append(' ').append(STREAM_FLAG)
            // 固定顺序（bg → tail-only），与 want/capabilities 的迭代序无关，命令串可测、可 diff。
            for (token in listOf(CAP_BG, CAP_TAIL_ONLY)) {
                if (token in want && token in capabilities) {
                    append(' ').append(FLAG_OF.getValue(token))
                }
            }
            // 硬 flag 只看 want，排在能力 flag 之后（后端剥 flag 与位置无关）。
            for ((token, flag) in HARD_FLAG_OF) {
                if (token in want) append(' ').append(flag)
            }
        }

    /** 一次性查询：项目列表。 */
    fun listProjects(daemonPath: String): String = query(daemonPath, "--list-projects")

    /**
     * 一次性查询：`<quote 过的路径> -- <子命令> [quote 过的参数…]`，外加 [guarded] 的正向标记。
     *
     * [ARGV_END] 必须紧跟路径，否则整行交给 claude，表现与「后端不认识这个子命令」一样（都拿不到
     * [QUERY_OK_MARKER]）。子命令名和 [ARGV_END] 不 quote（`'--'` 对后端只是普通参数）；
     * 路径与参数值必须 quote（含空格的路径会被 word-split）。
     */
    fun query(
        daemonPath: String,
        subcommand: String,
        vararg args: String,
    ): String =
        guarded(
            buildString {
                append(shellQuote(daemonPath))
                append(' ').append(ARGV_END)
                append(' ').append(subcommand)
                for (a in args) append(' ').append(shellQuote(a))
            },
        )

    /**
     * 查询成功的肯定证据。后端出错时写 stderr、exit 2、stdout 空，而远端执行通道只搬 stdout，
     * 拿不到 stderr 和退出码 ⇒ 空 stdout 既可能是「真的没有」也可能是「命令不存在」。见到标记才算成功。
     */
    const val QUERY_OK_MARKER: String = "ATERM_Q_OK"

    /**
     * 给一条一次性查询加正向成功标记。
     *
     * 标记前的换行不能省：`--read-session*` 输出原始文件字节，末尾未必有换行，
     * 不补的话标记会和最后一行黏在一起、把那行 JSON 撑坏。
     */
    fun guarded(command: String): String = "$command && printf '\\n$QUERY_OK_MARKER\\n'"

    /** 定位命中时打回来的行首标记。整行形状是 `ATERM_LOC_HIT <候选序号> <远端展开后的绝对路径>`。 */
    const val LOCATE_HIT_MARKER: String = "ATERM_LOC_HIT"

    /**
     * 一条候选的存在性判定 —— 绝不执行它。
     *
     * | [byName] | 判定 | 用在 |
     * |---|---|---|
     * | true | `command -v -- X >/dev/null 2>&1` | PATH 上的名字 |
     * | false | `[ -x X ]` | 一条路径（含 `"$HOME"/…`） |
     *
     * 裸跑 `ccm` 这类启动器会在远端真起一条 Claude 会话，而定位要挨个碰候选，
     * 所以命令模板只收在这一处，任何多出来的片段（如 `|| X`）都不许有。
     *
     * 命中时把 [rendered] 再展开一次打回来：`"$HOME"/…` 只在远端有意义。
     *
     * @param index 候选序号，原样打回来，用来把命中映射回是哪一条候选。
     * @param rendered 已经决定好要不要 quote 的那一段（见 `DaemonLocator.Candidate`），这里不再动它。
     */
    fun presence(
        index: Int,
        rendered: String,
        byName: Boolean,
    ): String {
        val test = if (byName) "command -v -- $rendered >/dev/null 2>&1" else "[ -x $rendered ]"
        return "$test && printf '$LOCATE_HIT_MARKER $index %s\\n' $rendered"
    }

    /**
     * 把若干条 [presence] 串成一条定位命令。
     *
     * 末尾的 `; true` 不能省：最后一条候选不存在时 `&&` 让整条命令退非零，就见不到 [guarded] 的标记，
     * 「跑完了但一条都没命中」会被误读成「命令没跑成」。这两者必须分得开。
     */
    fun locate(tests: List<String>): String {
        require(tests.isNotEmpty()) { "候选表不许为空——空表等于悄悄放弃定位" }
        return guarded(tests.joinToString("; ") + "; true")
    }

    /** 从 [guarded] 包装过的输出里取正文；没见到标记 ⇒ `null`（查询失败，别当空结果）。 */
    fun bodyOrNullIfFailed(stdout: String): String? {
        val trimmed = stdout.trimEnd()
        if (!trimmed.endsWith(QUERY_OK_MARKER)) return null
        // 连同 [guarded] 补的前置换行一起剥掉，正文才和远端文件逐字节一致
        return trimmed.removeSuffix(QUERY_OK_MARKER).removeSuffix("\n")
    }

    /**
     * 解析一次性查询的输出：每行一个裸 JSON 对象，没有 `kind` 字段。返回原始行，解析交给各查询自己。
     *
     * 返回类型就是判据：`null` = 查询失败（没见到正向标记），空列表 = 查到了但没有内容。
     * 丢空行和非 `{` 开头的行，免得哪天错误文本落到 stdout 被当 JSON 喂下去。
     */
    fun parseQueryLines(stdout: String): List<String>? =
        bodyOrNullIfFailed(stdout)
            ?.lineSequence()
            ?.map { it.trim() }
            ?.filter { it.startsWith("{") }
            ?.toList()
}
