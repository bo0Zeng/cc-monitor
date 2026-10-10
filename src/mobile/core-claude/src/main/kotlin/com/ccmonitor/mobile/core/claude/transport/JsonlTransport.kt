package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import com.ccmonitor.mobile.core.claude.model.AgentKind
import kotlinx.coroutines.flow.Flow

/**
 * 一帧（后端 wire 协议，PROTO_VERSION=1）。β tail 只发 [Line] 和 [Hello]；α（[DaemonTransport]）解析后端的
 * Hello / Line / SessionAdded / SessionStatus / SessionRemoved / TurnEnd / Overflow，两种 tmux 帧按未知 kind 跳过。
 * 字段名统一叫 `sessionId`：wire 里 Line / TurnEnd 用 `session_id`，SessionAdded / Status / Removed 用 `sid`，
 * 由 [DaemonTransport] 按帧型映射。
 */
sealed interface JsonlFrame {
    /**
     * 一行原始 jsonl。[byteOffset] = 本行末尾（含 `\n`）的累计原始字节 = resume 锚点
     * （β 取 [LineFramer.Framed.endOffset]，α 取 wire 的 `byte_offset`）。[seq] = 每条订阅从 0 起的序数，不是 resume 键。
     * [raw] 原样透传，消费方自己 parse。
     */
    data class Line(
        val sessionId: String?,
        val path: String?,
        val seq: Long,
        val raw: String,
        val byteOffset: Long,
    ) : JsonlFrame

    /**
     * 握手帧。β [TailTransport] 发裸 `Hello()`；α 带元信息 + [capabilities] + [emits]。
     * [capabilities] = 流模式可剥离 flag 的 token 集（如 `bg` / `tail-only`，每个对应一个后端剥得掉的 flag）；
     * [emits] = 后端声明会发哪些帧 kind（如 `turn_end`），缺则不依赖该帧、回退 β。
     * 二者正交：往 capabilities 塞帧声明，会让我们发出后端剥不掉的 flag。
     */
    data class Hello(
        val v: Int = 1,
        val buildId: String? = null,
        val hostArch: String? = null,
        val claudeDir: String? = null,
        val capabilities: List<String> = emptyList(),
        val emits: List<String> = emptyList(),
        // codexDir：Codex 配置目录（缺 = Codex 未启用）。kinds：后端声明服务的 agent kind
        // （如 `["claude","codex"]`；缺/空 = 仅 claude），判支持看它，别凭 codexDir 推断。
        val codexDir: String? = null,
        val kinds: List<String> = emptyList(),
    ) : JsonlFrame

    /** α：新会话出现。sid 之外全可缺。[sessionKind] 是原始串，消费方自己套白名单，不预折叠未知 kind。 */
    data class SessionAdded(
        val sessionId: String,
        val path: String?,
        val sessionKind: String? = null,
        val cwd: String? = null,
        val name: String? = null,
        val lines: Int? = null,
        val status: String? = null,
        val waitingFor: String? = null,
        // agentKind：原始串 "claude"|"codex"，缺 = claude（见 [agentKindFromWire]）。
        // livenessConfidence："authoritative"（pidfile）|"heuristic"（mtime），缺 = authoritative（见 [livenessAuthoritative]）。
        val agentKind: String? = null,
        val livenessConfidence: String? = null,
        /**
         * 能不能 attach 进这个会话。缺席 = `true`；只认真布尔，字符串 `"false"` 之类按缺席处理。
         */
        val attachable: Boolean? = null,
        /**
         * 宣告时的活动档（与 [SessionStatus.activity] 同一套值）。`session_added` 上没有 `status`，只有这一格；
         * 它是「连上之前就在等」的唯一到达路径，漏了它，合上手机过一阵回来就永远看不见等待态。
         * 缺席 = 对端说不清，不是「它闲着」。
         */
        val activity: String? = null,
        /**
         * 这条是不是后台会话（不是人坐在终端里对话的那种）。后端只在 `true` 时发，
         * 所以缺席 ≡ `false` ≡ 交互会话，不需要三态（[attachable] 的缺席与 `false` 必须分开，所以它是 `Boolean?`）。
         * 只在 `session_added` 上，状态帧不许把它重置。
         */
        val background: Boolean = false,
    ) : JsonlFrame

    /**
     * α：会话活着时的状态变化。没有 alive 字段：活死走帧生命周期（[SessionAdded] = 活，本帧 = 活证，
     * [SessionRemoved] = 死或 PID 复用）。后端内部用 pid + 进程启动时间判活防 PID 复用。
     */
    data class SessionStatus(
        val sessionId: String,
        /**
         * 上游 pidfile 的原值（`busy`/`waiting`/`idle`/`shell`）。新后端不发这一格（恒 `null`），
         * 旧后端只有它。判定不许只看它，见 [activity]。
         */
        val status: String? = null,
        val waitingFor: String? = null,
        // 判活可信度，同 [SessionAdded.livenessConfidence]。
        val livenessConfidence: String? = null,
        /**
         * 后端算好的活动档成品（`working` / `needs_you` / `idle`）。
         *
         * 与 [status] 不是一套值，所以两格都收、不折叠：后端把 `shell` 并进了 `idle`，
         * 只读 `activity` 会丢掉「在跑别的命令」那一档。新后端的 `session_added` / `session_status`
         * 上都只有 `activity`，只认 `status` 的消费方会拿到恒 `null`。
         */
        val activity: String? = null,
    ) : JsonlFrame

    data class SessionRemoved(
        val sessionId: String,
        /**
         * 为什么没了（`"gone"` 是默认值、不序列化）。
         *
         * - 缺席 / `"gone"`：会话真的结束了
         * - `"superseded"`：`/branch`、`/clear` 之类原地换了 sid，对话还在、只是换了身份。
         */
        val cause: String? = null,
    ) : JsonlFrame

    /**
     * α：后端拥塞丢了 [dropped] 帧。消费方另起 offset exec 续拉 `[baseline, 当前)` 补齐，补齐前不推进 baseline。
     *
     * [lost] / [lostTruncated] 说丢的是什么。`session_status` 丢了永远补不回来（它是此刻的状态，没有第二份），
     * 消费方必须知道丢的是哪一条的 status，否则手上那个 `waiting` 可能是过期的最后一帧。
     *
     * @property lost 丢帧身份表。缺席 = 空表 ≠ 没丢：旧后端只发 [dropped]，只知道丢了几帧、不知道是谁的。
     * @property lostTruncated 这张表没列全 ⇒ 任何一条的 status 都可能在没列出的那批里，保守方向是全部作废。
     */
    data class Overflow(
        val dropped: Int,
        val lost: List<Lost> = emptyList(),
        val lostTruncated: Boolean = false,
    ) : JsonlFrame {
        /**
         * `overflow.lost` 数组里的一项：`kind` = 丢掉的帧型，
         * `subject` = 那一帧说的是谁（`session_status` 的 subject 就是 sid）。
         */
        data class Lost(
            val kind: String,
            val subject: String? = null,
        )
    }

    /**
     * α：一个 assistant turn 完成。[uuid] = 完成记录的 uuid，供幂等去重。后端按
     * `stop_reason=="end_turn" && !isApiError && !isSidechain` 判一次，客户端不再重复判定。
     */
    data class TurnEnd(
        val sessionId: String,
        val uuid: String,
    ) : JsonlFrame
}

/**
 * wire `agent_kind` → [AgentKind]。`"codex"` → Codex；其余（`"claude"` / 缺 / 未知）→ ClaudeCode
 * （后端对 Claude 省略这个字段）。wire 值是小写串，不是 DB 存的 enum.name。串 ↔ 种类的表在 [AgentProfile.fromWire]。
 */
fun agentKindFromWire(wire: String?): AgentKind = AgentProfile.fromWire(wire).kind

/**
 * wire `liveness_confidence` → 判活是否权威。`"heuristic"`（Codex 按 mtime）→ false；其余（含缺席）→ true。
 * 与 β 侧 `LiveSessionScan.authoritativeLiveness` 同义。
 */
fun livenessAuthoritative(wire: String?): Boolean = wire != "heuristic"

/**
 * 传输抽象：β [TailTransport]（裸 jsonl 行）与 α [DaemonTransport]（后端 wire 帧）。
 *
 * 注意：它不是即插即用的插口。消费方直接 new 具体 transport 并读它的 `currentOffset`
 * （那个成员在具体类上、不在本接口），切 α 要重写调用点。[JsonlFrame.Line.byteOffset] 两边同义，
 * 消费方可以改读它代替 `currentOffset`。
 */
interface JsonlTransport {
    fun frames(): Flow<JsonlFrame>
}
