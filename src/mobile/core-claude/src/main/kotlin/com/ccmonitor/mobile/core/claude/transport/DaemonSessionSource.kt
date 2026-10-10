package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow

/**
 * 把「探测 → 按能力起流 → 维护会话表」收成一条流。
 *
 * 负责：
 * 1. 能力协商：先 [DaemonProbe] 拿 `hello`，据 `capabilities` 决定发哪些 flag。
 * 2. 维护会话表：`session_added` / `session_status` / `session_removed` 增量 → 快照。
 * 3. 把降级说出来：`overflow` 与探测失败都作为显式状态发出。
 *
 * 不负责：UI；补齐 overflow 丢的行；`line` 帧正文。
 *
 * 代价：两次 exec（探测一次、主流一次）。探测那条流拿到 `hello` 后被取消、真正杀掉，
 * 但多一次 SSH 往返和后端冷启。`want ∩ capabilities` 为空时其实可以继续用第一条流，目前没做。
 *
 * overflow 必须显式上抛：后端拥塞时丢行并发 `overflow`，静默忽略的话 UI 会显示一份看起来完整、
 * 实际缺行的会话表。所以它变成 [State.dropped]，由上层决定补齐、标脏还是提示。
 */
class DaemonSessionSource(
    private val channel: RemoteCommandChannel,
    private val daemonPath: String,
) {
    /** 后端视角的一条会话。字段取 wire 的子集，只放总览面要用的。 */
    data class Session(
        val sessionId: String,
        val path: String? = null,
        val cwd: String? = null,
        val name: String? = null,
        val kind: String? = null,
        val agentKind: String? = null,
        val status: String? = null,
        val waitingFor: String? = null,
        val livenessConfidence: String? = null,
        /**
         * attach 进去对人有没有意义，不是「活着吗」。`false` = 不给 attach、跳转、「杀死空 tmux」这几个动作；
         * 省略 = `true`。它决定动作可不可用，不决定这条会话显不显示。只在 `session_added` 上出现。
         */
        val attachable: Boolean = true,
        /**
         * 此刻在干什么的成品档（wire 原串 `working` / `needs_you` / `idle`）。
         * `session_added` / `session_status` 上没有 `status`，只有这一格，总览面的红绿灯读它。
         *
         * 缺席即清空（与 [waitingFor] 同、与 [status] 不同）：对端说不清就不发，
         * 保留上一次的档会让灯停在过去；变中性才对。
         *
         * 后端把 `idle` 与 `shell` 都并成 `idle`，「在跑别的命令」这一档在这条路上拿不到；
         * pidfile 那条路（`ClaudeSessionCatalog`）还有，要补在那一侧补，别在这里混读两套值。
         */
        val activity: String? = null,
        /**
         * 这条是不是后台会话（见 [JsonlFrame.SessionAdded.background]）。缺席 ≡ `false` ≡ 交互会话。
         * 同 [attachable]：只在 `session_added` 上，状态帧不许把它重置。
         */
        val background: Boolean = false,
    )

    /**
     * 一次快照。每来一帧就发一个新的，上层按需节流。
     *
     * @property dropped 非 null 表示数据不完整（overflow 丢过行），UI 必须显示。只给数，文案归 app。
     */
    data class State(
        val hello: JsonlFrame.Hello? = null,
        val sessions: Map<String, Session> = emptyMap(),
        /**
         * 被顶替（`/branch`、`/clear` 原地换 sid）而下线的会话，按 sid 存最后一份快照。
         * 旧 sid 不是死了，收到就直接归档，不要再去查 tmux 快照（对这个场景恒错）。
         * 真没了的（`gone`，缺字段即是它）不进这里。
         */
        val superseded: Map<String, Session> = emptyMap(),
        /**
         * 下线了，而这台后端说不出为什么。`cause` 缺席在 wire 上等于 `"gone"`，而不会发 `cause` 的
         * 旧后端也长这个样，靠帧本身分不开，只能看对端版本。不分的话 `/clear` 一下那条对话会从总览面
         * 无声消失。所以是三态：[superseded]（被顶替）· 丢掉（真没了）· 本格（说不出口），
         * UI 要给本格自己的一区。
         */
        val removedUnknown: Map<String, Session> = emptyMap(),
        val fatal: String? = null,
        /**
         * 原始异常，供上层按类型区分故障（如 `ConnectionDeadException` → 「未连接」空态，其余 → 「加载失败」）。
         */
        val cause: Throwable? = null,
        /** 累计丢行数（overflow）。null = 从未丢过。 */
        val dropped: Int? = null,
    ) {
        val capabilities: List<String> get() = hello?.capabilities ?: emptyList()

        /**
         * 上层补齐完丢失的行后调它清除降级标志。本类自己永远不清：它不负责补齐。
         *
         * 重连就算补齐：对端每条新流都先做一次全量扫描，当前所有会话被重新宣告一遍。
         */
        fun acknowledgeDegraded(): State = copy(dropped = null)
    }

    /**
     * 连上 daemon 并持续发快照。
     *
     * @param want 想要的流模式能力；对端没声明的静默降级（旧后端是正常情况）。
     * @param probeTimeoutMs 等 `hello` 的上限。冷启（SSH + 进程启动）比稳态慢得多。
     *
     * 探测失败 ⇒ 发一个 `fatal` 快照后结束。不静默返回空流，那样上层只会永远转圈。
     */
    fun states(
        // 默认要 tail-only：不要它的话常驻流的大部分流量是列表用不上的 `line` 正文帧。
        // 多要一个 token 不会打破旧后端：streamCommand 会与对端宣告的 capabilities 求交集。
        want: Collection<String> = listOf(DaemonCommands.CAP_BG, DaemonCommands.CAP_TAIL_ONLY),
        probeTimeoutMs: Long = 15_000,
    ): Flow<State> =
        flow {
            val (cmd, probe) = DaemonProbe(channel, daemonPath).streamCommand(probeTimeoutMs, want)
            if (cmd == null) {
                emit(State(fatal = probe.failure ?: "daemon 探测失败（无更多信息）"))
                return@flow
            }

            var st = State(hello = probe.hello)
            emit(st)

            // 主流也必须有出口：中途断开要变成 fatal（与探测失败同一套契约），
            // 正常结束也要说出来，否则 UI 看到一张停在过去、却自称健康的会话表。
            try {
                DaemonTransport(channel, cmd).frames().collect { fr ->
                    st = st.apply(fr)
                    emit(st)
                }
            } catch (e: kotlin.coroutines.cancellation.CancellationException) {
                throw e // 取消不是故障，必须向上传播
            } catch (e: java.io.IOException) {
                emit(st.copy(fatal = "daemon 流中断：${e::class.simpleName}: ${e.message}", cause = e))
                return@flow
            }
            // 流正常结束 = 后端退出了，不是「一切正常」。
            emit(st.copy(fatal = "daemon 流已结束（对端退出）。此后的会话表不再更新。"))
        }

    private companion object {
        /**
         * 与后端 `session_removed.cause` 的被顶替取值逐字一致。漂了不报错而是静默退化：
         * 认不出就落回 `gone`，`/clear` 又变回「会话崩了」。
         */
        const val CAUSE_SUPERSEDED = "superseded"

        /**
         * 同一个 sid 会收到不止一条 `session_added`（第二个 pidfile 会再宣告一次）。
         * 所以是并入而不是重建：新帧带了什么就更新什么，没带的保留，否则两条 added 之间
         * 到达的 `session_status` 更新会被抹掉。
         */
        private fun State.addSession(fr: JsonlFrame.SessionAdded): State {
            val prev = sessions[fr.sessionId]
            return copy(
                sessions =
                    sessions +
                        (
                            fr.sessionId to
                                Session(
                                    sessionId = fr.sessionId,
                                    path = fr.path ?: prev?.path,
                                    cwd = fr.cwd ?: prev?.cwd,
                                    name = fr.name ?: prev?.name,
                                    kind = fr.sessionKind ?: prev?.kind,
                                    agentKind = fr.agentKind ?: prev?.agentKind,
                                    status = fr.status ?: prev?.status,
                                    waitingFor = fr.waitingFor ?: prev?.waitingFor,
                                    livenessConfidence = fr.livenessConfidence ?: prev?.livenessConfidence,
                                    // 缺席 = true。只在 `session_added` 里出现，所以只在这里读。
                                    attachable = fr.attachable ?: prev?.attachable ?: true,
                                    // 活动档缺席即清空，见 [Session.activity]。
                                    activity = fr.activity,
                                    // 同 attachable：只在这一支读；缺席 ≡ false ≡ 交互会话。
                                    background = fr.background,
                                )
                        ),
            )
        }

        /**
         * 下线原因是三态（见 [State.removedUnknown]）：
         *
         * | 对端会不会说 `cause`（[WireFeature.REMOVAL_CAUSE]） | 帧上的 `cause` | 落到哪 |
         * |---|---|---|
         * | 会说（`build_id` ≥ `p1t-removal-cause`） | `"superseded"` | [State.superseded]：被顶替 |
         * | 会说 | 缺席 / `"gone"` | 丢掉：真没了 |
         * | 不会说 / 认不出 / 没带 `build_id` | 任何 | [State.removedUnknown]：说不出口 |
         *
         * 第三行不看 `cause`：旧后端不发这个字段，它的缺席与新后端的缺席在 wire 上一模一样。
         */
        private fun State.removeSession(fr: JsonlFrame.SessionRemoved): State {
            // 留住最后一份快照，只剩一个 sid 对 UI 没用
            val last = sessions[fr.sessionId] ?: Session(fr.sessionId)
            val rest = sessions - fr.sessionId
            if (!WireFeature.REMOVAL_CAUSE.isSpokenBy(hello?.buildId)) {
                return copy(sessions = rest, removedUnknown = removedUnknown + (fr.sessionId to last))
            }
            return copy(
                sessions = rest,
                superseded = if (fr.cause == CAUSE_SUPERSEDED) superseded + (fr.sessionId to last) else superseded,
            )
        }

        /** `overflow.lost` 里那个「丢的是状态帧」的帧型名（对端 `LostFrame.kind`）。 */
        private const val KIND_SESSION_STATUS = "session_status"

        /**
         * 丢掉的状态帧 ⇒ 那条对话的状态当场作废。`session_status` 是「此刻的状态」，丢了没有第二份可读，
         * 手上那个值可能是过期的最后一帧。作废 = `status` · `activity` · `waitingFor` 一起清成 null
         * （灯变中性）。只清状态，不删对话。
         *
         * 两种射程：`lost` 里点了名的只作废那几条；点不出名的（`lost_truncated`，或某项没带 subject）
         * 全部作废。代价是列表被截断时整张表短暂变成「说不好」；拥塞会触发自动重连，
         * 新流会重新宣告所有会话。
         */
        private fun State.invalidateLostStatus(fr: JsonlFrame.Overflow): State {
            val statusLosses = fr.lost.filter { it.kind == KIND_SESSION_STATUS }
            val unscoped = fr.lostTruncated || statusLosses.any { it.subject == null }
            val named = statusLosses.mapNotNull { it.subject }.toSet()
            if (!unscoped && named.isEmpty()) return this
            return copy(
                sessions =
                    sessions.mapValues { (sid, s) ->
                        if (unscoped || sid in named) {
                            s.copy(status = null, activity = null, waitingFor = null)
                        } else {
                            s
                        }
                    },
            )
        }

        /** 增量帧 → 新快照。未知帧型原样跳过（后端加新帧不该让我们崩）。 */
        fun State.apply(fr: JsonlFrame): State =
            when (fr) {
                is JsonlFrame.Hello ->
                    // 重启会再发一个 hello；以最新的为准，但保留已知会话表
                    copy(hello = fr)

                is JsonlFrame.SessionAdded -> addSession(fr)

                is JsonlFrame.SessionStatus -> {
                    // 状态帧可能先于 added 到达（事件驱动，顺序不保证）⇒ 凭空建一条，丢掉的话状态永远不更新。
                    val prev = sessions[fr.sessionId] ?: Session(fr.sessionId)
                    copy(
                        sessions =
                            sessions +
                                (
                                    fr.sessionId to
                                        prev.copy(
                                            status = fr.status ?: prev.status,
                                            // 活动档缺席即清空（见 [Session.activity]）。
                                            activity = fr.activity,
                                            // status/liveness 缺席时保留旧值；waitingFor 缺席即清空，
                                            // 否则「不再等待」之后 UI 会一直显示等待。
                                            waitingFor = fr.waitingFor,
                                            livenessConfidence =
                                                fr.livenessConfidence ?: prev.livenessConfidence,
                                        )
                                ),
                    )
                }

                is JsonlFrame.SessionRemoved -> removeSession(fr)

                is JsonlFrame.Overflow ->
                    // 拥塞丢了行，UI 必须知道拿到的是残缺数据；丢的若是状态帧，那条对话的状态当场作废。
                    // 本类不清这个标志（它不负责补齐），清除权交给上层的 [State.acknowledgeDegraded]。
                    // 只出结构化事实，文案归 app。
                    copy(dropped = (dropped ?: 0) + fr.dropped).invalidateLostStatus(fr)

                // Line / TurnEnd 属会话正文与轮次；本类只管有哪些会话
                else -> this
            }
    }
}
