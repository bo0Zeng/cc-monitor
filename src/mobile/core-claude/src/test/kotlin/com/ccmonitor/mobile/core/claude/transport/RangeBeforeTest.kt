package com.ccmonitor.mobile.core.claude.transport

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/**
 * [SkeletonScan.rangeBefore]：「再往前一段是哪一段」。
 *
 * 与 [SkeletonScan.windowStartOffset]（首载：从哪开始到 EOF）成对；两者吃同一份骨架 ⇒ 翻页不重扫。
 *
 * 真实数据那一路的输入取法有界：
 * 1. 选文件按尺寸升序，取第一个 ≥ [SCAN_BYTES] 的；没有那么大的就退回最小的合格者。不取最大的那份。
 * 2. 只读定长前缀：在固定 [BUFFER_BYTES] 的缓冲里数 `'\n'`，最多读 [SCAN_BYTES] 字节、最多收
 *    [MAX_RECORDS] 条记录。任何一行都不读成 String，一条 50 MB 的巨型记录也撑不爆堆。
 * 单次运行的读入量与分配量都有编译期常量上界，与 `~/.claude/projects` 里有什么、有多大无关；
 * 整读最大那份会超出 Gradle test 默认的 512m 堆，而且会随使用越变越大。
 *
 * 不保证的：
 * - 只看会话的前 [SCAN_BYTES] 字节：只在尾部出现的形态（末行没有尾随换行、超大单行落在文件末尾）看不见。
 * - `~/.claude/projects` 不存在时（干净 CI）真实数据那条直接 skip，只剩
 *   [pagingBackwardOverAnAdversarialSkeletonCoversItExactlyOnce] 恒跑兜底。
 * - 「不取最大」这一维没有判据接住：选法换成取最大的照样绿（读入已有前缀上界），丢的只是输入可复现。
 * - 合成骨架的记录长度是手写的固定序列，不是属性测试：恰好不在那串长度里的病态形状会漏网。
 */
class RangeBeforeTest {
    private fun rec(
        offset: Long,
        uuid: String,
    ) = SkeletonRecord(offset, uuid, "", "assistant", "2026-08-01T00:00:00Z", false)

    private fun skeleton(vararg offsets: Long) = offsets.mapIndexed { i, o -> rec(o, "u$i") }

    // ---- 基本语义 ---------------------------------------------------------------

    @Test
    fun takesTheLastNRecordsBeforeTheCurrentStart() {
        val sk = skeleton(0, 100, 200, 300, 400, 500)
        val r = SkeletonScan.rangeBefore(sk, currentStart = 400, n = 2, maxBytes = 0)
        assertEquals("应取 200/300 这两条 ⇒ 从 200 开始", 200L, r.start)
        assertEquals("右端就是当前起点（左闭右开）", 400L, r.endExclusive)
    }

    /** 更早的不足 n 条 ⇒ 一路取到文件头。 */
    @Test
    fun fallsBackToTheBeginningWhenFewerThanNRecordsRemain() {
        val sk = skeleton(0, 100, 200)
        assertEquals(0L, SkeletonScan.rangeBefore(sk, currentStart = 200, n = 99, maxBytes = 0).start)
    }

    /**
     * 到顶 = 空区间，UI 靠它显示「已是最早」。
     *
     * 返回一个「看起来正常但实际什么都没有」的区间会让 UI 无限转圈。
     */
    @Test
    fun reachingTheBeginningYieldsAnEmptyRangeSoTheUiCanSaySo() {
        val sk = skeleton(0, 100, 200)
        assertTrue("已在文件头", SkeletonScan.rangeBefore(sk, currentStart = 0, n = 5, maxBytes = 0).isEmpty)
        assertTrue("没有更早的记录了", SkeletonScan.rangeBefore(sk, currentStart = 0L, n = 5, maxBytes = 0).isEmpty)
    }

    // ---- 字节封顶 ---------------------------------------------------------------

    /**
     * 单次翻页不许拉巨块；收紧只让 start 变大（区间变小），绝不多载。
     * 且收紧后必须仍落在记录边界上，否则会传一段没用的 partial 首行。
     */
    @Test
    fun byteCapShrinksTheRangeAndStaysOnARecordBoundary() {
        val sk = skeleton(0, 1_000, 2_000, 3_000, 4_000)
        val uncapped = SkeletonScan.rangeBefore(sk, currentStart = 4_000, n = 99, maxBytes = 0)
        val capped = SkeletonScan.rangeBefore(sk, currentStart = 4_000, n = 99, maxBytes = 1_500)

        assertEquals("不封顶时应一路到头", 0L, uncapped.start)
        assertTrue("封顶只能让 start 变大", capped.start > uncapped.start)
        assertTrue("收紧后仍须落在记录边界上：${capped.start}", sk.any { it.byteOffset == capped.start })
        assertTrue("区间不得超过封顶", capped.endExclusive - capped.start <= 1_500)
    }

    /** 单条巨记录本身就超封顶 ⇒ 退到最后一条（最小可行窗口，不可再小）。 */
    @Test
    fun aSingleHugeRecordFallsBackToTheSmallestPossibleWindow() {
        val sk = skeleton(0, 10_000_000)
        val r = SkeletonScan.rangeBefore(sk, currentStart = 20_000_000, n = 1, maxBytes = 1_000)
        assertEquals(10_000_000L, r.start)
        assertTrue("仍是个非空区间，不能因为超封顶就假装到顶了", !r.isEmpty)
    }

    // ---- 真实大会话 -------------------------------------------------------------

    /**
     * 连续翻页必须首尾相接、不重不漏，且一路翻到文件头：翻多远下多远。
     *
     * 两条测试共用（真实会话前缀 / 合成刁钻骨架）：一条依赖本机有真数据，另一条恒跑。
     *
     * 四条断言各守一件事：
     * 1. `endExclusive == cursor`：接缝上不许有缝（漏）也不许有叠（重）。
     * 2. 每页正好 [PAGE_SIZE] 条（到文件头那页除外）：计数游标少退/多退一格时只有这一条会红。
     * 3. `cursor` 最终收敛到 `0`：不许中途谎报「已是最早」。
     * 4. 区间长度之和 `== eof`：整份正好被覆盖一次。
     *
     * @return 翻了几页（调用方再断言页数地板，免得 0 页也算「全过」）。
     */
    private fun assertPagingCoversExactlyOnce(
        sk: List<SkeletonRecord>,
        eof: Long,
        label: String,
    ): Int {
        var cursor = eof
        var covered = 0L
        var pages = 0
        while (pages < MAX_PAGES) {
            val r = SkeletonScan.rangeBefore(sk, cursor, PAGE_SIZE, maxBytes = 0)
            if (r.isEmpty) break
            assertEquals("$label：区间必须与上一段首尾相接（不重不漏）", cursor, r.endExclusive)
            val inPage = sk.count { it.byteOffset >= r.start && it.byteOffset < r.endExclusive }
            assertTrue(
                "$label：一页应正好 $PAGE_SIZE 条（只有翻到文件头那页允许更少），实际 $inPage 条 @[${r.start},${r.endExclusive})",
                inPage == PAGE_SIZE || (r.start == 0L && inPage in 1..PAGE_SIZE),
            )
            covered += r.endExclusive - r.start
            cursor = r.start
            pages++
        }
        assertTrue("$label：必须真的翻了好几页，实际 $pages", pages >= MIN_PAGES)
        assertEquals("$label：一路翻到底应正好停在文件头", 0L, cursor)
        assertEquals("$label：所有区间拼起来应正好覆盖整个文件一次", eof, covered)
        return pages
    }

    /** [scanLineStarts] 的读数：[lineStarts] 是行起点（含 0），[scannedBytes] 是实际从磁盘读了多少。 */
    private class PrefixScan(
        val lineStarts: List<Long>,
        val scannedBytes: Long,
    )

    /**
     * 有界、流式地扫出 [file] 开头那段的行起点。
     *
     * 终止条件是两个编译期常量（[SCAN_BYTES] 字节、[MAX_RECORDS] 条），表达式里不出现 [file] 的长度。
     * 只在固定 [BUFFER_BYTES] 缓冲里数 `'\n'`，不把任何一行读成 String：
     * 一条 50 MB 的巨型记录（大文件 Read / base64 图）也只是被数过去，不会被物化。
     */
    private fun scanLineStarts(file: File): PrefixScan {
        val starts = ArrayList<Long>()
        starts.add(0L)
        val buf = ByteArray(BUFFER_BYTES)
        var pos = 0L
        var stop = false
        file.inputStream().use { input ->
            while (!stop && pos < SCAN_BYTES) {
                val want = minOf(buf.size.toLong(), SCAN_BYTES - pos).toInt()
                val read = input.read(buf, 0, want)
                if (read <= 0) break
                var i = 0
                while (i < read) {
                    if (buf[i] == NEWLINE) {
                        starts.add(pos + i + 1)
                        // 多收一个起点当 eof 用 ⇒ 记录数正好 MAX_RECORDS
                        if (starts.size > MAX_RECORDS) {
                            pos += i + 1
                            stop = true
                            break
                        }
                    }
                    i++
                }
                if (!stop) pos += read
            }
        }
        return PrefixScan(starts, pos)
    }

    /**
     * 选一份真实会话：合格者（≥ [MIN_BYTES]）按 (尺寸, 路径) 升序，
     * 取第一个 ≥ [SCAN_BYTES] 的（让字节上界真的绷紧）；没有那么大的就退回最小的合格者。
     */
    private fun pickRealSession(): File? {
        val root = File(System.getProperty("user.home"), ".claude/projects")
        if (!root.isDirectory) return null
        val candidates =
            root
                .walkTopDown()
                .maxDepth(3)
                .filter { it.isFile && it.name.endsWith(".jsonl") && it.length() >= MIN_BYTES }
                .sortedWith(compareBy({ it.length() }, { it.path }))
                .toList()
        return candidates.firstOrNull { it.length() >= SCAN_BYTES } ?: candidates.firstOrNull()
    }

    /**
     * 真实会话上连续翻页：区间必须首尾相接、不重不漏，且一路能翻到文件头。
     *
     * 骨架用真文件的行起点构造（与 awk 扫出来的 `byteOffset` 同义），只取前 [SCAN_BYTES] 字节，
     * 把「最后一个行起点」当成这段的 eof ⇒ eof 落在真实记录边界上，覆盖等式仍是精确的。
     */
    @Test
    fun pagingBackwardThroughARealSessionCoversItExactlyOnce() {
        val jsonl = pickRealSession()
        assumeTrue("本机没有足够大的真实会话（这条依赖真数据）", jsonl != null)
        requireNotNull(jsonl)

        val scan = scanLineStarts(jsonl)
        // 末尾那个行起点留作 eof，其余当记录 ⇒ 每条记录都是完整的一行，eof 也在记录边界上
        assumeTrue("会话记录数不足", scan.lineStarts.size >= MIN_RECORDS + 1)
        val sk = scan.lineStarts.dropLast(1).mapIndexed { i, o -> rec(o, "u$i") }
        val eof = scan.lineStarts.last()

        // 有界自证：实现正确时恒真，去掉读入循环的上界就会红。
        assertTrue(
            "单次运行读入的字节数必须有编译期常量上界：实际 ${scan.scannedBytes} > $SCAN_BYTES",
            scan.scannedBytes <= SCAN_BYTES,
        )
        assertTrue("骨架条数必须有编译期常量上界：实际 ${sk.size} > $MAX_RECORDS", sk.size <= MAX_RECORDS)
        // 绿不许是「因为空而绿」：先证真的读到了东西
        assertTrue("必须真的读到了内容，实际读入 ${scan.scannedBytes} 字节", scan.scannedBytes > 0L)

        assertPagingCoversExactlyOnce(sk, eof, "真实会话前缀 ${jsonl.name}（${sk.size} 条 / ${scan.scannedBytes} 字节）")
    }

    /**
     * 同一条性质，零外部依赖，这条恒跑：上面那条在干净 CI 上会跳过。
     *
     * 记录长度故意不等（1 字节 / 普通 / 128 KB 巨型交替），条数也故意不是 [PAGE_SIZE] 的整数倍：
     * 等距 + 整除会让好几种 off-by-one 错法算出来仍然自洽。
     */
    @Test
    fun pagingBackwardOverAnAdversarialSkeletonCoversItExactlyOnce() {
        var off = 0L
        val sk = ArrayList<SkeletonRecord>(ADVERSARIAL_RECORDS)
        for (i in 0 until ADVERSARIAL_RECORDS) {
            sk.add(rec(off, "u$i"))
            off += ADVERSARIAL_LENGTHS[i % ADVERSARIAL_LENGTHS.size]
        }
        assertTrue("合成骨架条数要够翻好几页", sk.size >= MIN_RECORDS)
        assertPagingCoversExactlyOnce(sk, off, "合成刁钻骨架（${sk.size} 条）")
    }

    private companion object {
        const val MIN_BYTES = 200_000L
        const val MIN_RECORDS = 60
        const val PAGE_SIZE = 20
        const val MAX_PAGES = 10_000
        const val MIN_PAGES = 3

        /** 硬上界：单次运行最多从磁盘读这么多字节。表达式里不许出现任何文件的长度。 */
        const val SCAN_BYTES = 4L * 1024 * 1024

        /** 硬上界：骨架最多这么多条，挡住「一份全是 1 字节行的病态文件」。 */
        const val MAX_RECORDS = 8_000

        /** 固定缓冲：整个扫描过程的堆占用就是它 + 上面那两个上界，与文件多大无关。 */
        const val BUFFER_BYTES = 64 * 1024

        /** `'\n'`。写字面量 10 是因为 `const val` 不接受 `Char.code.toByte()`。 */
        const val NEWLINE: Byte = 10

        /** 合成骨架的记录长度环：含 1 字节记录与 128 KB 巨型记录，刻意不等距。 */
        val ADVERSARIAL_LENGTHS = longArrayOf(1, 7, 4_096, 1, 131_072, 23, 2, 900, 65_537)

        /** 137 不是 [PAGE_SIZE] 的整数倍，也不是 [ADVERSARIAL_LENGTHS] 长度的整数倍。 */
        const val ADVERSARIAL_RECORDS = 137
    }

    /** `n <= 0` 不许崩（`older[older.size - n]` 会抛 IndexOutOfBounds）。 */
    @Test
    fun nonPositivePageSizeYieldsAnEmptyRangeInsteadOfCrashing() {
        val sk = skeleton(0, 100, 200)
        assertTrue("n=0", SkeletonScan.rangeBefore(sk, currentStart = 300, n = 0, maxBytes = 0).isEmpty)
        assertTrue("n=-1", SkeletonScan.rangeBefore(sk, currentStart = 300, n = -1, maxBytes = 0).isEmpty)
    }

    /** `currentStart > 0` 但没有更早记录（走 `older.isEmpty()` 那条）。 */
    @Test
    fun noOlderRecordsBelowTheCursorAlsoYieldsEmpty() {
        val sk = skeleton(500, 600)
        assertTrue(SkeletonScan.rangeBefore(sk, currentStart = 500, n = 5, maxBytes = 0).isEmpty)
    }

    // ---- 有界区间取字节 ----------------------------------------------------------

    /**
     * 「只下被请求的字节区间」必须真的有界：只用 `tail -c +N` 会读到 EOF，`endExclusive` 没人消费。
     */
    @Test
    fun rangeContentCommandIsBoundedNotReadToEof() {
        val cmd = SkeletonScan.rangeContentCommand("/tmp/a b.jsonl", ByteRange(100, 350))
        assertTrue("起点应是 1-based 的 tail -c +101：$cmd", cmd.contains("tail -c +101"))
        assertTrue("必须有长度上界（250 字节）：$cmd", cmd.contains("head -c 250"))
        assertTrue("路径要引起来（含空格）：$cmd", cmd.contains("'/tmp/a b.jsonl'"))
    }

    /** 空区间不该发命令：调用方应先判 `isEmpty` 显示「已是最早」。 */
    @Test
    fun anEmptyRangeIsRejectedInsteadOfSilentlyFetchingNothing() {
        var threw = false
        try {
            SkeletonScan.rangeContentCommand("/tmp/a.jsonl", ByteRange.EMPTY)
        } catch (e: IllegalArgumentException) {
            threw = true
        }
        assertTrue("空区间必须显式拒绝", threw)
    }
}
