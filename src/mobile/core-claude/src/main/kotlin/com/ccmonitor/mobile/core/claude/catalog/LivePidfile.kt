package com.ccmonitor.mobile.core.claude.catalog

// 自扫记录树那一族（`ClaudeSessionCatalog`）选 pidfile 用；那一族随「列表与正文改读核心」整块删时一起删。

/**
 * 同一 sid 有多个 pidfile 时选当前那个（规则与后端会话表一致）：
 * 1. kind 白名单：只有 `interactive` 或 kind 缺失算 rank 1，其余（bg 及任何未知值）一律 rank 0，
 *    这样 CC 给后台进程换了名字也照样被压下去；
 * 2. 同 rank 时，两侧 procStart 都可解析且不等才比 procStart（大=晚起），否则 pid 大者胜，
 *    消掉 glob 顺序的任意性。
 *
 * 空列表 → null。
 */
fun resolveLivePidfile(pidfiles: List<SessionPidfile>): SessionPidfile? = pidfiles.maxWithOrNull(livePidfileOrder)

/**
 * 某 sid 的当前活会话 pidfile：取 [resolveLivePidfile] 的赢家，但赢家不是 interactive 级（只剩 bg）时为 null。
 * 父进程退出、后台 pidfile 残留时，这个 sid 不算活会话。判活和点灯都走这个函数。
 */
fun resolveActivePidfile(pidfiles: List<SessionPidfile>): SessionPidfile? =
    resolveLivePidfile(pidfiles)?.takeIf { kindRank(it) == 1 }

/** kind 白名单 rank：interactive 或缺失=1，其余=0。 */
private fun kindRank(p: SessionPidfile): Int = if (p.kind == null || p.kind == "interactive") 1 else 0

/** 活会话 pidfile 排序，最大者为当前。见 [resolveLivePidfile]。 */
private val livePidfileOrder =
    Comparator<SessionPidfile> { a, b ->
        val rankDiff = kindRank(a) - kindRank(b)
        if (rankDiff != 0) {
            rankDiff
        } else {
            val pa = a.procStart?.toLongOrNull()
            val pb = b.procStart?.toLongOrNull()
            if (pa != null && pb != null && pa != pb) pa.compareTo(pb) else a.pid.compareTo(b.pid)
        }
    }
