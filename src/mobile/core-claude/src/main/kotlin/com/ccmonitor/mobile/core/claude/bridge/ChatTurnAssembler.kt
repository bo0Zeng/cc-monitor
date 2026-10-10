package com.ccmonitor.mobile.core.claude.bridge

import com.ccmonitor.mobile.core.claude.model.RenderUnit

/** 一轮对话的状态。以 `res` 帧为准，不看有没有块还开着。 */
sealed interface TurnState {
    /**
     * 还没有任何一轮开始过。
     *
     * 刚起的管道不能自称「生成中」：那样发送键画成停止键，第一句话发不出去，`res` 永远不来。
     * 与 [Done] 不可互换：[Done] 携带上一轮的成败与成本，[Idle] 表示这些还不存在。
     */
    data object Idle : TurnState

    data object Streaming : TurnState

    data class Done(
        val ok: Boolean,
        val why: String?,
        val durationMs: Long?,
        /**
         * 失败时给人看的那句话（`res` 帧的 `raw["result"]`），如 `Not logged in · Please run /login`。
         * [why] 来自 `terminal_reason`，是机器码（如 `api_error`），光显示它看不出要去登录。
         * 这句是上游给的英文，本层不翻译；人话不在 `res.raw.result` 里的失败（如远端没装 claude）为 null。
         */
        val humanText: String? = null,
    ) : TurnState
}

/**
 * 一帧被路由到哪儿。路由决策只有 [ChatTurnAssembler.route] 一处；调用方另判一遍会与它不一致，
 * 表现是已读的正文倒退。
 */
sealed interface FrameRoute {
    /** 属于主对话流，且能定位到某个块。 */
    data class MainBlock(
        val key: String,
    ) : FrameRoute

    /** 子 agent 产物（`p` 非空）：收下但不渲染。 */
    data class Sidechain(
        val parentToolUseId: String,
    ) : FrameRoute

    /** 会话级/控制帧，不属于任何块。 */
    data object Session : FrameRoute
}

/**
 * 帧流 → [RenderUnit]。
 *
 * 另一个生产者是 `RecordClassifier`（JSONL 记录 → RenderUnit），两者共用一个渲染器，
 * 所以这里不新造渲染单元。
 *
 * - `at`/`tt` 到达时无条件覆盖 delta 拼出来的内容；`res` 到达时封口所有仍开着的块
 *   （中断时有块永远等不到 `be`）。
 * - 块 key 带显式前缀 [KEY_PREFIX]，不靠 id 的形状区分：SDK 出错时 `message_id` 就是 UUID，
 *   与 classifier 的 `uuid#i` 同形，撞 key 会让 `LazyColumn` 抛异常。
 * - 顺序与身份分离：[order] 存 key 的排列（用户消息与助手块共用这一条），内容在 [blocks]/[userTexts]，
 *   往顶部插更老的一段只动 [order]。
 */
class ChatTurnAssembler {
    private val order = mutableListOf<String>()

    /** [order] 的身份索引：防止 [prependOrder] 先插了 key、块的帧到达时又追加一次。 */
    private val orderSet = hashSetOf<String>()
    private val blocks = mutableMapOf<String, Block>()
    private val userTexts = mutableMapOf<String, RenderUnit.UserText>()

    /** tool_use_id → 块 key。`tr` 帧只带 `id`，没有 `m`/`i`，只能靠它回填。 */
    private val toolKeyById = mutableMapOf<String, String>()

    /** 先于 `tu` 到达的孤儿 `tr`，建块时回放，否则工具卡永远转圈。 */
    private val orphanResults = mutableMapOf<String, BridgeFrame.ToolResult>()

    /**
     * `m` 缺失时的消息分代（中途接流、收到第一个 `message_start` 之前）。
     * 两条 `m=null, i=0` 的帧会撞同一个 key，索引不再前进就认为换了一条消息。
     */
    private var nullEpoch = 0
    private var lastNullIndex = -1

    /**
     * prepend 期间用独立的负数分代空间。共用的话，老段里 `m == null` 的块会抬高 [lastNullIndex]，
     * 正在流的块再来一条 `i=0` 就开新分代、换 key 分裂，已读正文倒退。
     */
    private var prepending = false
    private var prependNullEpoch = 0
    private var prependLastNullIndex = -1

    /** 子 agent 的产物：收下但不渲染，与 `RecordClassifier` 对 sidechain 的处置一致。 */
    val sidechain: MutableMap<String, MutableList<BridgeFrame>> = mutableMapOf()

    /** 没有对应渲染单元的帧（`ev`、未知帧型）。不丢弃，留给 debug 面。 */
    val unhandled: MutableList<BridgeFrame> = mutableListOf()

    /** 当前轮次状态，初值 [TurnState.Idle]。 */
    var turn: TurnState = TurnState.Idle
        private set

    var init: BridgeFrame.Init? = null
        private set

    var lastError: BridgeFrame.Err? = null
        private set

    /** 仍开着的块数。`res` 之后必须是 0。 */
    val openBlockCount: Int get() = blocks.values.count { it.open }

    private enum class Kind { TEXT, THINKING, TOOL }

    private class Block(
        val key: String,
        var kind: Kind,
        val delta: StringBuilder = StringBuilder(),
        /** `at`/`tt` 的全文。非 null 时它就是正文，delta 只是过程。 */
        var authoritative: String? = null,
        var open: Boolean = true,
        /**
         * 类型是否已由权威帧（`bs.bt` / `at` / `tt` / `tu`）确定。delta 帧只建议类型，权威帧才决定；
         * 权威帧也能把 TOOL 纠正回 TEXT，否则 `bs(bt="tool_use")` 后跟 `at` 会成一张永久转圈的空卡。
         */
        var kindLocked: Boolean = false,
        var toolUseId: String? = null,
        var toolName: String? = null,
        var toolInput: Map<String, Any?>? = null,
        var toolResult: String? = null,
        var toolOk: Boolean = true,
        var toolDone: Boolean = false,
    ) {
        val text: String get() = authoritative ?: delta.toString()

        /** 见过任何工具身份没有；没见过就不渲染成空的转圈卡。 */
        val hasToolIdentity: Boolean get() = toolUseId != null || toolName != null
    }

    /**
     * 唯一的路由判据。调用方（如缓释器）消费它的结论，不自己再判。
     *
     * 带 `p` 的帧（包括五个流式帧）一律进 sidechain。流式帧漏判的话，子 agent 的增量会被当成主流块，
     * 上层切流时主块已露出的正文清零重来，而带 `p` 的全文帧去了 sidechain，那个块再也得不到纠正。
     */
    fun route(frame: BridgeFrame): FrameRoute {
        frame.parentToolUseId()?.let { return FrameRoute.Sidechain(it) }
        // `tr` 没有 (m,i) 坐标，只能靠 tool_use_id 反查；查不到就是孤儿（先于 `tu` 到达）。
        if (frame is BridgeFrame.ToolResult) {
            return frame.id?.let { toolKeyById[it] }?.let { FrameRoute.MainBlock(it) } ?: FrameRoute.Session
        }
        return blockCoordOf(frame)?.let { (m, i) -> FrameRoute.MainBlock(keyOf(m, i)) } ?: FrameRoute.Session
    }

    fun feed(frame: BridgeFrame) {
        // 分代只在这里推进一次：[keyOf] 每帧被调两次（route 一次、建块一次），不能带副作用。
        // 只有「新块开始」的帧推进：增量帧与 `be` 共享同一个 `i`，拿它们判会每帧换一次 key。
        if (frame.startsABlock()) blockCoordOf(frame)?.let { (m, i) -> if (m == null) advanceNullEpoch(i) }

        val r = route(frame)
        if (r is FrameRoute.Sidechain) {
            sidechain.getOrPut(r.parentToolUseId) { mutableListOf() } += frame
            return
        }
        // 不按 route 的结论分派：孤儿 `tr` 的 route 是 Session，但它要走 [applyToolResult] 挂起等回填。
        val handled = feedBlockFrame(frame)
        if (!handled) feedSessionFrame(frame)
        // 上一轮已收口（或还没开始），又来了块级帧或 `init`：新的一轮开始了，常驻管道靠它推回 Streaming。
        // 不认任意帧：`res` 之后合法地还会飘来 `ev`/`rl`，那会让界面无中生有地跳回「生成中」。
        // 写 `!is Streaming` 而不是 `is Done`：第一轮从 [TurnState.Idle] 起步。
        if (turn !is TurnState.Streaming && (handled || frame is BridgeFrame.Init)) turn = TurnState.Streaming
    }

    /** @return 这一帧是否属于块级帧（已处理）。 */
    private fun feedBlockFrame(frame: BridgeFrame): Boolean {
        when (frame) {
            is BridgeFrame.BlockStart ->
                block(frame.m, frame.i, frame.bt.toKind(), locked = frame.bt != null)
                    .bindTool(frame.id, frame.name)

            // delta 只建议类型，权威帧才决定（见 Block.kindLocked）。
            is BridgeFrame.TextDelta -> block(frame.m, frame.i, Kind.TEXT).delta.append(frame.x)
            is BridgeFrame.ThinkingDelta -> block(frame.m, frame.i, Kind.THINKING).delta.append(frame.x)
            // ij 是工具入参的残缺 JSON：拼着但不解析，`tu` 一到就被权威入参取代。
            is BridgeFrame.InputJsonDelta -> block(frame.m, frame.i, Kind.TOOL).delta.append(frame.x)
            is BridgeFrame.BlockEnd -> blocks[keyOf(frame.m, frame.i)]?.open = false

            is BridgeFrame.AssistantText ->
                block(frame.m, frame.i, Kind.TEXT, locked = true).authoritative = frame.x

            is BridgeFrame.ThinkingText ->
                block(frame.m, frame.i, Kind.THINKING, locked = true).authoritative = frame.x

            is BridgeFrame.ToolUse ->
                block(frame.m, frame.i, Kind.TOOL, locked = true)
                    .bindTool(frame.id, frame.name)
                    .also { it.toolInput = frame.inp }

            is BridgeFrame.ToolResult -> applyToolResult(frame)
            else -> return false
        }
        return true
    }

    /** 绑定工具身份，并回放先到的孤儿 `tr`。`bs` 与 `tu` 都带 id/name，回填逻辑只放这一处。 */
    private fun Block.bindTool(
        id: String?,
        name: String?,
    ): Block =
        apply {
            id?.let {
                toolUseId = it
                toolKeyById[it] = key
                orphanResults.remove(it)?.let { orphan -> fill(orphan) }
            }
            name?.let { toolName = it }
        }

    private fun Block.fill(frame: BridgeFrame.ToolResult) {
        toolResult = frame.x
        toolOk = frame.ok
        toolDone = true
        open = false
    }

    private fun applyToolResult(frame: BridgeFrame.ToolResult) {
        val target = frame.id?.let { toolKeyById[it] }?.let { blocks[it] }
        if (target != null) {
            target.fill(frame)
            return
        }
        // `tr` 先于 `tu` 到：挂起来等回填。
        val id = frame.id
        if (id != null) orphanResults[id] = frame else unhandled += frame
    }

    private fun feedSessionFrame(frame: BridgeFrame) {
        when (frame) {
            is BridgeFrame.Init -> init = frame
            // `ut` 与助手块走同一条 order，多轮对话里才不会被顶到最前面。
            is BridgeFrame.UserText -> addUserText(frame.x)

            is BridgeFrame.Result -> {
                // turn 结束就是结束，封口所有开着的块。
                blocks.values.forEach { it.open = false }
                // `result` 不被编码器特化，落在 `raw` 里，从那里捞出来给 UI。
                turn =
                    TurnState.Done(
                        frame.ok,
                        frame.why,
                        frame.dur,
                        humanText = (frame.raw?.get("result") as? String)?.takeIf { it.isNotBlank() },
                    )
            }

            is BridgeFrame.Err -> lastError = frame
            // 剩下的（rl/ev/未知）留痕，不丢弃。
            else -> unhandled += frame
        }
    }

    /**
     * `ut` 帧没有身份（没有 uuid/message_id），按正文内容去重，prepend 接缝重叠时同一条不显示两遍。
     *
     * 注意：用户真说了两遍同样的话会被当成一条；要根治需要协议给 `ut` 带 uuid。
     */
    private fun addUserText(text: String) {
        val key = "${KEY_PREFIX}ut#${text.hashCode()}"
        if (userTexts.containsKey(key)) return
        userTexts[key] = RenderUnit.UserText(key, text, null)
        if (orderSet.add(key)) order += key
    }

    /** 主对话流的渲染单元，按到达顺序（用户消息与助手块交错）。 */
    fun units(): List<RenderUnit> = order.mapNotNull { userTexts[it] ?: blocks[it]?.toUnit() }

    /**
     * 把更早的一段帧插到前面（上滑翻历史）。
     *
     * 照常 feed，再把新增的 key 挪到最前：边界去重、孤儿 `tr` 回填、块类型判定都复用同一份逻辑。
     * 老段自带的 `res`/`err`/`init` 不能改写此刻这一轮，所以快照再还原，开着的块重新打开。
     *
     * @return 实际新增的单元数。0 表示这一段全是已载过的，上层据此判到顶，否则会无限重拉。
     */
    fun prependFrames(olderFrames: List<BridgeFrame>): Int {
        val known = orderSet.toHashSet()
        val turnBefore = turn
        val errorBefore = lastError
        val initBefore = init
        val openBefore = blocks.filterValues { it.open }.keys.toList()

        prepending = true
        // 先递减：从 0 起的话第一条老块的 key 是 `?0#0`，与实时流第一个 m==null 块相撞。
        prependNullEpoch--
        prependLastNullIndex = -1
        try {
            olderFrames.forEach { feed(it) }
        } finally {
            prepending = false
        }

        turn = turnBefore
        // 只还原、不继承：老段里过去的 `err` 不是当前的失败原因。
        lastError = errorBefore
        // init 承载命令清单，被老段覆盖会让选择器显示过时内容。
        init = initBefore ?: init
        openBefore.forEach { blocks[it]?.open = true }

        // 新增的 key 挪到最前面，保持彼此的相对顺序。
        val added = order.filter { it !in known }
        if (added.isEmpty()) return 0
        val addedSet = added.toHashSet()
        order.removeAll(addedSet)
        order.addAll(0, added)
        return added.size
    }

    /** 只调整 key 排列的低层入口。日常用 [prependFrames]。 */
    fun prependOrder(olderKeys: List<String>) {
        // 去重走 [orderSet]：翻历史必然与已加载的部分重叠；`add` 也顺带去掉入参自身的重复。
        order.addAll(0, olderKeys.filter { orderSet.add(it) })
    }

    private fun blockCoordOf(frame: BridgeFrame): Pair<String?, Int>? =
        when (frame) {
            is BridgeFrame.BlockStart -> frame.m to frame.i
            is BridgeFrame.TextDelta -> frame.m to frame.i
            is BridgeFrame.ThinkingDelta -> frame.m to frame.i
            is BridgeFrame.InputJsonDelta -> frame.m to frame.i
            is BridgeFrame.BlockEnd -> frame.m to frame.i
            is BridgeFrame.AssistantText -> frame.m to frame.i
            is BridgeFrame.ThinkingText -> frame.m to frame.i
            is BridgeFrame.ToolUse -> frame.m to frame.i
            // tr 没有 (m,i)，靠 tool_use_id 回填。
            is BridgeFrame.ToolResult -> null
            else -> null
        }

    /** 纯函数：只读当前分代，不推进（推进在 [feed] 里做一次）。 */
    private fun keyOf(
        m: String?,
        i: Int,
    ): String {
        if (m != null) return "$KEY_PREFIX$m#$i"
        return "$KEY_PREFIX?${if (prepending) prependNullEpoch else nullEpoch}#$i"
    }

    /** m 缺失时索引不再前进，就认为换了一条消息，开新分代。 */
    private fun advanceNullEpoch(i: Int) {
        if (prepending) {
            if (i <= prependLastNullIndex) prependNullEpoch--
            prependLastNullIndex = i
        } else {
            if (i <= lastNullIndex) nullEpoch++
            lastNullIndex = i
        }
    }

    private fun block(
        m: String?,
        i: Int,
        kind: Kind,
        locked: Boolean = false,
    ): Block {
        val key = keyOf(m, i)
        return blocks
            .getOrPut(key) {
                // 这个 key 可能已被 prependOrder 插进顺序里。
                if (orderSet.add(key)) order += key
                Block(key, kind, kindLocked = locked)
            }.also {
                // 权威帧可以纠正类型；delta 只在还没定下来时提建议。
                if (locked) {
                    it.kind = kind
                    it.kindLocked = true
                } else if (!it.kindLocked) {
                    it.kind = kind
                }
            }
    }

    private fun String?.toKind() =
        when (this) {
            "thinking" -> Kind.THINKING
            "tool_use" -> Kind.TOOL
            else -> Kind.TEXT
        }

    private fun Block.toUnit(): RenderUnit? =
        when (kind) {
            Kind.TEXT -> text.takeIf { it.isNotEmpty() }?.let { RenderUnit.AssistantMarkdown(key, it, null) }
            Kind.THINKING -> text.takeIf { it.isNotEmpty() }?.let { RenderUnit.Thinking(key, it, null) }
            // 没见过工具身份就不渲染，否则是 name="?"、永远转圈的空卡。
            Kind.TOOL ->
                if (!hasToolIdentity) {
                    null
                } else {
                    RenderUnit.ToolCall(
                        key = key,
                        toolUseId = toolUseId ?: key,
                        name = toolName ?: "?",
                        input = toolInput.orEmpty(),
                        resultText = toolResult,
                        isError = !toolOk,
                        // pending 看有没有拿到结果，不看块开没开：中断留下的块被 res 封了口，但没有结果。
                        pending = !toolDone,
                        sourceUuid = null,
                    )
                }
        }

    companion object {
        /** 帧侧 key 的显式前缀，保证不与 classifier 的 `uuid#i` 相撞。 */
        const val KEY_PREFIX = "f:"
    }
}

/** 这一帧是否标志「一个新块开始」（而不是往已有块里追加）。 */
private fun BridgeFrame.startsABlock(): Boolean =
    this is BridgeFrame.BlockStart ||
        this is BridgeFrame.AssistantText ||
        this is BridgeFrame.ThinkingText ||
        this is BridgeFrame.ToolUse

/** `p` = `parent_tool_use_id`：非空表示这帧是子 agent 产的。 */
internal fun BridgeFrame.parentToolUseId(): String? =
    when (this) {
        is BridgeFrame.AssistantText -> p
        is BridgeFrame.ThinkingText -> p
        is BridgeFrame.ToolUse -> p
        is BridgeFrame.ToolResult -> p
        is BridgeFrame.UserText -> p
        // 五个流式帧也带 `p`，漏了子 agent 的增量会按主流块路由（见 [ChatTurnAssembler.route]）。
        is BridgeFrame.BlockStart -> p
        is BridgeFrame.TextDelta -> p
        is BridgeFrame.ThinkingDelta -> p
        is BridgeFrame.InputJsonDelta -> p
        is BridgeFrame.BlockEnd -> p
        else -> null
    }
