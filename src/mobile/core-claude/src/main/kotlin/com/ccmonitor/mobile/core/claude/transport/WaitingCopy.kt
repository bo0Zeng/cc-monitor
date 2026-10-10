package com.ccmonitor.mobile.core.claude.transport

/**
 * 「Claude 在等人」这件事在屏幕上说什么：两个屏、两条路径共用的一份文案。
 *
 * 输入框上方那句与 `TmuxSendKeysSink` 的模态拒绝原因必须是同一个常量：投递路径上 `capture-pane`
 * 认出弹窗是发到一半才发现，`session_status` 上的 `waiting` 是发之前就知道，两者是同一个事实的两个时刻；
 * 说成两句会被当成两种故障。那两条路径住在 `:app` 的两个包里，唯一的共同下游是本模块。
 *
 * 机器码一个都不许上屏：`waitingFor` 的值（`permission prompt` / `sandbox request` / …）是英文机器码。
 * [headlineFor] 只翻已知的几个，不认识的走 [UNKNOWN_PREFIX] 原样带回（读不懂每一种等待）。
 */
object WaitingCopy {
    /**
     * 那句共用的话。用在聊天屏等待态时输入框上方（发之前就知道），和 `TmuxSendKeysSink.classify`
     * 的模态拒绝原因（发到一半才发现）。别在任一处再写一遍字面量。
     */
    const val SENDING_NOW_ANSWERS_THE_QUESTION = "电脑上有个问题待答 · 现在发消息会被当成对它的回答"

    /** 需手动条的第二行（那句人话 + 这句）。 */
    const val ANSWER_IT_ON_THE_COMPUTER = "这条要在电脑上回答。"

    /** 不认识的 `waitingFor` 值：原样带回，不猜、不翻译。 */
    const val UNKNOWN_PREFIX = "需手动 · "

    /**
     * `waitingFor` 缺席（在等但没说等什么）时那句它自己的话。不回退成通用句：
     * 缺席可能是上游漏写，回退成通用句会把问题藏起来。
     */
    const val WAITING_BUT_UNSAID = "需手动 · 没说等什么"

    /**
     * 六个已知值 → 人话，键序 = 危险度从高到低（见 [dangerRank]）。键是机器码原文，值里没有英文。
     * 全仓只许有这一张表。
     *
     * 语义来自 Claude Code 自己的产出方：
     *
     * | 值 | 产出条件 | 读自 CC 版本 |
     * |---|---|---|
     * | `input needed` | MCP 服务器在向人要输入 | `2.1.261` |
     * | `permission prompt` | 兜底默认：屏上那个框没登记 `waitingFor` 时都落到它，不是某一个框的名字 | `2.1.261` |
     * | `worker request` | 不是框，子 agent 在要权限 | `2.1.261` |
     * | `sandbox request` | 不是框，沙盒里的命令要联网 | `2.1.261` |
     * | `dialog open` | 本地命令自己画的框 | `2.1.261` |
     * | `goal proposal` | Claude 提议一个会话目标（notification 原文 `Claude proposed a session goal`） | `2.1.261` |
     *
     * 这张表是一个版本的读数，CC 加了新值我们不会知道，所以映射没有穷举分支，认不出的一律走 [UNKNOWN_PREFIX]。
     *
     * 分不出的一件事：纯告知框（按一下知道了）和要做决定的框报的都是 `dialog open`，其余信息不出 CC 进程。
     * 所以这几句话一律说成「有件事在等」，不说成「必须现在做个决定」。
     */
    private val KNOWN =
        mapOf(
            "sandbox request" to "待放行一次越界操作",
            // 故意不具体：这个值是兜底默认，很多种框都落进它，说成具体的权限请求多数情况下是假的。
            "permission prompt" to "等批准一个操作",
            "input needed" to "等一句补充",
            "worker request" to "子任务等批准",
            // 「定个目标」是这件事的内容，「等批准」是要做的动作。
            "goal proposal" to "这次对话要定个目标 · 等批准",
            "dialog open" to "电脑上有个窗口待处理",
        )

    /** 屏上那句人话。[waitingFor] 为 null ⇒ [WAITING_BUT_UNSAID]；不认识 ⇒ [UNKNOWN_PREFIX] + 原文。 */
    fun headlineFor(waitingFor: String?): String {
        val key = waitingFor?.trim()?.ifEmpty { null } ?: return WAITING_BUT_UNSAID
        return KNOWN[key] ?: "$UNKNOWN_PREFIX$key"
    }

    /** 已知机器码的集合。 */
    val knownCodes: Set<String> get() = KNOWN.keys

    /**
     * 已知机器码，按危险度从高到低，就是 [KNOWN] 的键序（`mapOf` 保序）。危险度序只住在这里，别另写一份。
     */
    val knownCodesByDanger: List<String> get() = KNOWN.keys.toList()

    /**
     * 「先看哪个」的排序键，小的排前面。总览面用它排序。
     *
     * 按危险度不按时间：`sandbox request` / `permission prompt` 一旦被误答，Claude 就去做一件没被同意的事；
     * `dialog open` 只是有个窗口开着。认不出的新值与缺席同档，排在全部已知值之后：
     * 不能让一个读不懂的值顶掉一条真的 `sandbox request`。
     *
     * @return `0 ..< knownCodes.size` = 已知值；`knownCodes.size` = 认不出或缺席。
     */
    fun dangerRank(waitingFor: String?): Int {
        val key = waitingFor?.trim()?.ifEmpty { null } ?: return KNOWN.size
        val at = knownCodesByDanger.indexOf(key)
        return if (at >= 0) at else KNOWN.size
    }
}
