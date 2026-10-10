package com.ccmonitor.mobile.core.claude.catalog

import com.ccmonitor.mobile.core.claude.transport.SessionSignals

/*
 * 会话状态灯，纯逻辑。两个点灯口各管一条路，不混用：
 * - [lightForActivity]：daemon 流，吃后端算好的活动档 `activity`。总览面用它。
 * - [lightForStatus]：直读 pidfile，吃 pidfile 的原词 `status`。阅读面用它。
 */

/** 会话状态灯。[Unknown] 表示说不清（活动档缺席、pidfile 未写 status、认不出的词），显示中性。 */
enum class SessionLight { Working, WaitingInput, Idle, Shell, Stopped, Unknown }

/**
 * daemon 流的点灯口：后端 `session_added` / `session_status` 帧上的活动档 → 灯。
 *
 * | 取值 | 含义 | 灯 |
 * |---|---|---|
 * | `working` | 一轮在跑 | [SessionLight.Working] |
 * | `needs_you` | 在等人（批准、回答、弹窗） | [SessionLight.WaitingInput] |
 * | `idle` | 闲着 | [SessionLight.Idle] |
 * | 缺席或认不出 | 说不清 | [SessionLight.Unknown] |
 *
 * 只翻译，不再判：后端已经判了的，前端不重判。
 *
 * 不带 `alive`：这条路上「活」由帧的生命周期给（还在会话表里就是活），灯说的是 Claude 在干什么，
 * 不是进程活不活，所以这里点不出 `Stopped`，也点不出 `Shell`。
 *
 * 不拿 `status` 兜底：
 * 1. 不是一一对应，后端把 `shell` 并进了 `idle`；
 * 2. 两格缺席的意思相反：`status` 缺席保留旧值，`activity` 缺席即清空，`activity ?: status`
 *    会在后端说不清时把一个过期的 `status` 重新点亮；
 * 3. 不为不发 `activity` 的后端留回落分支。
 */
fun lightForActivity(activity: String?): SessionLight =
    when (activity) {
        ACTIVITY_WORKING -> SessionLight.Working
        // 与等待态共用同一个常量：总览「需手动」那一区与聊天屏拦不拦上行判的是同一件事。
        SessionSignals.ACTIVITY_NEEDS_YOU -> SessionLight.WaitingInput
        ACTIVITY_IDLE -> SessionLight.Idle
        else -> SessionLight.Unknown
    }

/** 活动档 `working`。 */
private const val ACTIVITY_WORKING = "working"

/** 活动档 `idle`。与 pidfile 原词 `idle` 同形，但不是同一套值。 */
private const val ACTIVITY_IDLE = "idle"

/**
 * 直读 pidfile 的点灯口：pidfile `status` → 灯。[alive]=false（无 pidfile 或进程已死）→ [SessionLight.Stopped]。
 *
 * 注意：daemon 流不许用它。那里的帧上没有 `status`，喂进来恒得 [SessionLight.Unknown]。
 *
 * 取值：busy=Working，waiting=WaitingInput，idle=Idle，shell=Shell；null 或未知 → [SessionLight.Unknown]。
 */
fun lightForStatus(
    status: String?,
    alive: Boolean,
): SessionLight {
    if (!alive) return SessionLight.Stopped
    return when (status) {
        "busy" -> SessionLight.Working
        "waiting" -> SessionLight.WaitingInput
        "idle" -> SessionLight.Idle
        "shell" -> SessionLight.Shell
        else -> SessionLight.Unknown
    }
}

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
