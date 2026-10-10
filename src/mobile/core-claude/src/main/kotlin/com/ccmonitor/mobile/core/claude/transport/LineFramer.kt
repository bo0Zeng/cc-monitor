package com.ccmonitor.mobile.core.claude.transport

import java.io.ByteArrayOutputStream

/**
 * 把字节流（`tail -F` 的 stdout chunk）增量切成**整行 UTF-8 字符串**。
 *
 * **字节级**而非字符级：chunk 可能切在一个多字节 UTF-8 字符中间，也可能切在行中间。
 * 缓冲字节直到遇 `\n`(0x0A)，整行字节一次性 UTF-8 解码（避免半个码点）；行内尾随 `\r` 去掉。
 * 非线程安全（每个 tail 流单线程喂）。
 */
class LineFramer {
    private val buf = ByteArrayOutputStream()

    /**
     * 从流开头到目前已提交完整行（含各自 `\n`）的累计原始字节数，即可安全 resume 的 offset
     * （`tail -c +<consumedBytes+1>` 从下一行续）。缓冲里的残行不计入。
     */
    var consumedBytes: Long = 0L
        private set

    /** 一行 + 该行末尾（含 `\n`）在流中的累计字节 offset。 */
    data class Framed(
        val raw: String,
        val endOffset: Long,
    )

    /** 喂一段字节，返回本次凑齐的所有完整行（不含换行符）。可能为空。 */
    fun feed(chunk: ByteArray): List<String> = feedFramed(chunk).map { it.raw }

    /** 同 [feed]，但每行附精确的字节 offset，供 resume 逐行推进（chunk 中途取消也不丢行）。 */
    fun feedFramed(chunk: ByteArray): List<Framed> {
        if (chunk.isEmpty()) return emptyList()
        val out = ArrayList<Framed>()
        for (b in chunk) {
            if (b == NL) {
                val line = decodeLine()
                consumedBytes += buf.size() + 1 // 本行字节（含尾随 \r，若有）+ 这个 \n
                buf.reset()
                out.add(Framed(line, consumedBytes))
            } else {
                buf.write(b.toInt())
                // 防 OOM：恶意/损坏远端可能发超长无换行行；超上限就强制切断（截断行→解析为 Unknown 丢弃）。
                if (buf.size() >= MAX_LINE_BYTES) {
                    val line = decodeLine()
                    consumedBytes += buf.size() // force-cut：这些字节已消费，但无 \n
                    buf.reset()
                    out.add(Framed(line, consumedBytes))
                }
            }
        }
        return out
    }

    /** 收尾：返回缓冲里剩余的残行（无尾换行），无则 null。调用后缓冲清空。 */
    fun flush(): String? {
        if (buf.size() == 0) return null
        val line = decodeLine()
        buf.reset()
        return line
    }

    /** 把当前缓冲解码成一行，去掉尾随 `\r`（CRLF→LF）。 */
    private fun decodeLine(): String {
        var bytes = buf.toByteArray()
        if (bytes.isNotEmpty() && bytes[bytes.size - 1] == CR) {
            bytes = bytes.copyOf(bytes.size - 1)
        }
        return String(bytes, Charsets.UTF_8)
    }

    private companion object {
        const val NL: Byte = 0x0A
        const val CR: Byte = 0x0D
        const val MAX_LINE_BYTES = 8 * 1024 * 1024 // 单行上限 8MB，超出强切防 OOM
    }
}
