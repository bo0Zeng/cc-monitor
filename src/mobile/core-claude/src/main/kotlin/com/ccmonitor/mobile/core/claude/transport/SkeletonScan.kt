package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.core.claude.model.MainBranch
import com.ccmonitor.mobile.core.remote.ShellWord

/*
 * 历史懒加载的骨架扫描。远端一条 awk 扫整份 jsonl，每行只吐小字段、不吐正文：
 *   byteOffset<TAB>uuid<TAB>parentUuid<TAB>type<TAB>timestamp<TAB>isInterrupt
 * 用这几 KB 算出主分支（[MainBranch]），再按字节范围只懒载最近 N 条主线正文，开屏不下整份。
 *
 * awk 要点：`LC_ALL=C` 保证 offset 按字节算（多字节 UTF-8）；uuid / parentUuid / timestamp 首匹配即顶层；
 * type 只匹配顶层白名单，避开内容块的 `"type":"text"`（很多记录以 parentUuid 开头、块 type 在前会误抓）；
 * isInterrupt 用 `index` 探 `[Request interrupted by user` 子串（骨架里没有正文）。
 */

/**
 * 一段字节区间，左闭右开。[EMPTY] 表示没有更早的了，UI 必须能区分出来并说出口，否则上滑到顶就是无限转圈。
 */
data class ByteRange(
    val start: Long,
    val endExclusive: Long,
) {
    val isEmpty: Boolean get() = start >= endExclusive

    companion object {
        val EMPTY = ByteRange(0L, 0L)
    }
}

/** 骨架记录：一行 jsonl 的小字段投影（[byteOffset] = 该行在文件的起始字节）。空串字段 = 缺失。 */
data class SkeletonRecord(
    val byteOffset: Long,
    val uuid: String,
    val parentUuid: String,
    val type: String,
    val timestamp: String,
    val isInterrupt: Boolean,
)

object SkeletonScan {
    /**
     * 骨架 awk 脚本（单行）。顶层类型白名单避开内容块 type。`\$0` = awk 整行，`\t` = TAB 分隔。
     */
    const val AWK_SCRIPT: String =
        // 显式初始化：off 未赋值会让首条记录 byteOffset 为空被丢；eof 兜底空文件（无行 → END 吐 0）。
        "BEGIN{off=0;eof=0}" +
            "{uuid=\"\";par=\"\";typ=\"\";ts=\"\";intr=\"0\";" +
            "if(match(\$0,/\"uuid\":\"[^\"]*\"/))uuid=substr(\$0,RSTART+8,RLENGTH-9);" +
            "if(match(\$0,/\"parentUuid\":\"[^\"]*\"/))par=substr(\$0,RSTART+14,RLENGTH-15);" +
            "if(match(\$0,/\"type\":\"(user|assistant|system|summary|attachment|ai-title|custom-title|agent-name|mode|permission-mode|last-prompt|queue-operation|file-history-snapshot)\"/))" +
            "{s=substr(\$0,RSTART,RLENGTH);gsub(/\"type\":\"|\"/,\"\",s);typ=s}" +
            "if(match(\$0,/\"timestamp\":\"[^\"]*\"/))ts=substr(\$0,RSTART+13,RLENGTH-14);" +
            "if(index(\$0,\"[Request interrupted by user\"))intr=\"1\";" +
            "print off\"\\t\"uuid\"\\t\"par\"\\t\"typ\"\\t\"ts\"\\t\"intr;eof=off+length(\$0);off+=length(\$0)+1}" +
            // END 吐扫描时刻的 eofBytes，供 tail 从此续接。吐的是 `eof = off + length($0)`（末行内容结束处、`\n` 前），
            // 不是 `off`：末行没有尾随 `\n`（正写到一半被扫到）时 `off` 会超出真实 EOF 1 字节，续接会跳过一个字节丢记录。
            // 这样 eof 永不超真实 EOF：末行有 `\n` ⇒ eof = 真实 - 1（tail 重读一个 `\n`，空行被解析层跳过）；
            // 末行无 `\n` ⇒ eof = 真实。
            "END{print eof}"

    /*
     * 下面三条命令都有两个入口、一份实现：
     *   - 接 `String` 的是字面路径（远端 glob 打回来的、相对路径常量…），在这里 [ShellWord.literal] 一次；
     *   - 接 [ShellWord] 的已经是一个词（如 `ClaudePaths.sessionRecordPath` 那个要远端展开的表达式），原样拼。
     * 已经 quote 过的词再引一层，远端就开不到文件、历史永远空白而不报错；两种入口靠类型分开，混用编译不过。
     */

    /** 骨架命令：`LC_ALL=C awk '<script>' <file>`。只用于 POSIX 远端。 */
    fun skeletonCommand(file: ShellWord): String = "LC_ALL=C awk '$AWK_SCRIPT' $file"

    /** [skeletonCommand] 的字面路径入口。 */
    fun skeletonCommand(path: String): String = skeletonCommand(ShellWord.literal(path))

    /** 窗口正文命令：`LC_ALL=C tail -c +<start+1> <file>`，只取 [start, EOF] 字节（有界首载）。 */
    fun windowContentCommand(
        file: ShellWord,
        startOffset: Long,
    ): String = "LC_ALL=C tail -c +${startOffset + 1} $file"

    /** [windowContentCommand] 的字面路径入口。 */
    fun windowContentCommand(
        path: String,
        startOffset: Long,
    ): String = windowContentCommand(ShellWord.literal(path), startOffset)

    /** 骨架解析结果：[records] = 每行小字段；[eofBytes] = 扫描时刻文件总字节（awk END 吐，供 tail 续接）。 */
    data class Result(
        val records: List<SkeletonRecord>,
        val eofBytes: Long,
    )

    /**
     * 解析骨架 TSV：6 字段行 = 记录；末尾单字段数字行 = END 的总字节（[Result.eofBytes]）。坏行跳过。
     * 没有 END 行则 eofBytes 回退为最后一条记录的 byteOffset（保守：最坏 tail 重读末行，解析后去重）。
     */
    fun parse(tsv: String): Result {
        val records = ArrayList<SkeletonRecord>()
        var eof = 0L
        for (line in tsv.lineSequence()) {
            val f = if (line.isEmpty()) emptyList() else line.split('\t')
            when {
                f.size >= 6 ->
                    f[0].toLongOrNull()?.let { records.add(SkeletonRecord(it, f[1], f[2], f[3], f[4], f[5] == "1")) }
                f.size == 1 -> f[0].toLongOrNull()?.let { eof = it } // END 总字节
            }
        }
        if (eof == 0L && records.isNotEmpty()) eof = records.last().byteOffset
        return Result(records, eof)
    }

    /** 全量 [MainBranch.extractBranchRecord] 只让这四种结构类型入分支链；骨架必须对齐，否则两路主线可能不同。 */
    private val BRANCH_TYPES = setOf("user", "assistant", "attachment", "system")

    /** 全量把 ai-title / custom-title 解成 [JsonlRecord.Title]（uuid 恒 null，永不入链）；骨架同样排除，两路才对齐。 */
    private val TITLE_TYPES = setOf("ai-title", "custom-title")

    /**
     * 骨架记录 → [MainBranch.BranchRecord]，与全量 [MainBranch.extractBranchRecord] 对齐（骨架主线 == 全量主线）：
     * user / assistant / attachment / system 原样；其余有 uuid + ts 的记录作 pass-through（type → "unknown"、非 interrupt），
     * 免得未知记录断链、让其后的对话成孤儿。无 uuid / ts → null。
     */
    fun toBranchRecord(s: SkeletonRecord): MainBranch.BranchRecord? {
        if (s.uuid.isEmpty() || s.timestamp.isEmpty() || s.type in TITLE_TYPES) return null
        val parent = s.parentUuid.ifEmpty { null }
        return if (s.type in BRANCH_TYPES) {
            MainBranch.BranchRecord(s.uuid, parent, s.timestamp, s.type, s.isInterrupt)
        } else {
            MainBranch.BranchRecord(s.uuid, parent, s.timestamp, "unknown", false)
        }
    }

    /**
     * 在骨架上算主线：过滤出主分支上的骨架记录（按文件序、每 uuid 保首见）。
     * 复用 [MainBranch.computeMainBranch]。骨架含全文件的 parent 链，这是主线算对的前提。
     */
    fun mainBranchSkeleton(skeleton: List<SkeletonRecord>): List<SkeletonRecord> {
        val onMain = MainBranch.computeMainBranch(skeleton.mapNotNull { toBranchRecord(it) })
        val emitted = HashSet<String>()
        return skeleton.filter { it.uuid.isNotEmpty() && it.uuid in onMain && emitted.add(it.uuid) }
    }

    /**
     * 只取一段字节（左闭右开），翻历史时用；[windowContentCommand] 是读到 EOF。
     *
     * 用 `tail -c +N | head -c LEN`：两者都是 POSIX，`dd` 的 `bs/skip` 在不同实现上差得多。
     * `head -c` 拿够就退出 ⇒ 上游 `tail` 收到 SIGPIPE 提前结束，不会把整个文件读完。
     */
    fun rangeContentCommand(
        file: ShellWord,
        range: ByteRange,
    ): String {
        require(!range.isEmpty) { "空区间不该发命令——调用方应先判 isEmpty 显示「已是最早」" }
        val len = range.endExclusive - range.start
        return "LC_ALL=C tail -c +${range.start + 1} $file | LC_ALL=C head -c $len"
    }

    /** [rangeContentCommand] 的字面路径入口。 */
    fun rangeContentCommand(
        path: String,
        range: ByteRange,
    ): String = rangeContentCommand(ShellWord.literal(path), range)

    /**
     * 上滑翻历史：已载起点 [currentStart] 之前那 [n] 条主线记录的字节区间。
     *
     * [windowStartOffset] 回答首载从哪开始到 EOF，这个回答再往前一段是哪一段；骨架不重扫，吃同一份 [mainSkeleton]。
     *
     * - 返回左闭右开 `[start, currentStart)`；空区间表示已经到顶，UI 显示「已是最早」。
     * - [maxBytes] 封顶，单次翻页不拉巨块。收紧只让 `start` 变大，且仍落在记录边界上（不传无用的半行）。
     */
    fun rangeBefore(
        mainSkeleton: List<SkeletonRecord>,
        currentStart: Long,
        n: Int,
        maxBytes: Long,
    ): ByteRange {
        if (currentStart <= 0L) return ByteRange.EMPTY
        if (n <= 0) return ByteRange.EMPTY // n≤0 时 `older[older.size - n]` 会越界抛异常
        val older = mainSkeleton.filter { it.byteOffset < currentStart }
        if (older.isEmpty()) return ByteRange.EMPTY

        val countStart = if (older.size <= n) older.first().byteOffset else older[older.size - n].byteOffset
        val start =
            if (maxBytes <= 0L || currentStart - countStart <= maxBytes) {
                countStart
            } else {
                val capFloor = currentStart - maxBytes
                // 落在记录边界上的最早一条；单条巨记录本身就超封顶时退到最后一条（最小可行窗口）
                (older.firstOrNull { it.byteOffset >= capFloor } ?: older.last()).byteOffset
            }
        return ByteRange(start, currentStart)
    }

    /**
     * 有界首载的窗口起始字节：主线骨架取最后 [n] 条，返回最早那条的 [SkeletonRecord.byteOffset]。
     * 主线 ≤ n ⇒ 从 0 起，仍受字节封顶约束。供 [windowContentCommand] 拉 [start, EOF]。
     *
     * 字节封顶：最近 n 条若 `eof - start > maxBytes`（巨型工具输出能让首载几十 MB），收紧到 `byteOffset ≥ eof - maxBytes`
     * 的最早主线记录；floor 之后没有记录（单条尾记录本身就超封顶）⇒ 退到最后一条。收紧只让 start 变大，
     * 且落在记录边界上（半行会被消费方按 uuid 过滤丢掉，但传它没用）。[maxBytes] ≤ 0 = 不封顶。
     */
    fun windowStartOffset(
        mainSkeleton: List<SkeletonRecord>,
        n: Int,
        eofBytes: Long,
        maxBytes: Long,
    ): Long {
        if (mainSkeleton.isEmpty()) return 0L
        val countStart = if (mainSkeleton.size <= n) 0L else mainSkeleton[mainSkeleton.size - n].byteOffset
        if (maxBytes <= 0L || eofBytes - countStart <= maxBytes) return countStart // 关阀 / 计数窗口本就 ≤ 封顶
        val capFloor = eofBytes - maxBytes
        return (mainSkeleton.firstOrNull { it.byteOffset >= capFloor } ?: mainSkeleton.last()).byteOffset
    }
}
