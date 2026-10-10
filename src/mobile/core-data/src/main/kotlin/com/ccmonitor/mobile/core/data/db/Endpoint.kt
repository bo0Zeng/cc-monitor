package com.ccmonitor.mobile.core.data.db

/** 一个可连地址（多地址竞速用）。 */
data class Endpoint(
    val host: String,
    val port: Int,
)

/**
 * 解析一行地址 → [Endpoint]。空行/空白/host 为空 → `null`；端口非法/越界回退 [defaultPort]。
 * 支持每行：
 *  - `host` / `host:port`（IPv4 或域名）
 *  - `[IPv6]` / `[IPv6]:port`（带方括号才解析端口）
 *  - 裸 IPv6（多个冒号、无方括号）→ 整体作 host、用 [defaultPort]（无法区分末段是端口）
 *
 * 连接侧 [Host.endpoints] 与主机编辑器的地址预览/校验共用此函数，规则只有一份。
 */
fun parseAddressLine(
    line: String,
    defaultPort: Int,
): Endpoint? {
    val s = line.trim()
    if (s.isEmpty()) return null

    fun mk(
        h: String,
        p: Int,
    ): Endpoint? = h.trim().takeIf { it.isNotEmpty() }?.let { Endpoint(it, p) }

    // 端口合法用之，否则回退 defaultPort（剥掉坏端口）。
    fun withPort(
        h: String,
        raw: String?,
    ): Endpoint? {
        val p = raw?.toIntOrNull()
        return mk(h, if (p != null && p in 1..65535) p else defaultPort)
    }
    return when {
        // [IPv6] 或 [IPv6]:port —— 仅方括号形式解析端口
        s.startsWith('[') -> {
            val close = s.indexOf(']')
            if (close > 0) {
                val h = s.substring(1, close)
                val rest = s.substring(close + 1)
                if (rest.startsWith(':')) withPort(h, rest.substring(1)) else mk(h, defaultPort)
            } else {
                mk(s, defaultPort) // 无闭括号 → 当整体 host
            }
        }
        // 裸 IPv6（多个冒号、无方括号）：末段无法确定是端口 → 整体作 host
        s.count { it == ':' } > 1 -> mk(s, defaultPort)
        // host 或 host:port
        else -> {
            val idx = s.lastIndexOf(':')
            if (idx > 0 && idx < s.length - 1) {
                withPort(s.substring(0, idx), s.substring(idx + 1))
            } else {
                mk(s, defaultPort)
            }
        }
    }
}

/**
 * 主机的全部可连地址：主地址（[Host.host]:[Host.port]）+ [Host.extraAddresses]（换行分隔，逐行 [parseAddressLine]）。
 * 按 host:port 去重、保持顺序（主地址在前）。
 */
fun Host.endpoints(): List<Endpoint> {
    val out = LinkedHashMap<String, Endpoint>()

    fun add(ep: Endpoint) {
        out.putIfAbsent("${ep.host}:${ep.port}", ep)
    }
    if (host.isNotBlank()) add(Endpoint(host.trim(), port))
    extraAddresses?.lineSequence()?.forEach { line -> parseAddressLine(line, port)?.let(::add) }
    return out.values.toList()
}
