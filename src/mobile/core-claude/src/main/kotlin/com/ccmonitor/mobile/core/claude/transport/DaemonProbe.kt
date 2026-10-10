package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.flow.firstOrNull
import kotlinx.coroutines.withTimeout

/**
 * 后端能力探测：拿 `hello`，据它决定后续能发哪些 flag。
 *
 * 要知道对端支持什么得先拿到 `hello.capabilities`，而要可靠拿到 `hello` 就不能先发任何 flag：
 * 对端剥不掉的 flag 会让它落进一次性查询、exit 2、永不发 hello，客户端无限重连。
 * 所以探测一律用裸流命令（[DaemonCommands.stream] 不带 want）。
 *
 * 拿不到 hello 有三种可能（没部署 / 路径错 / 发错 flag），表现都是什么都没有，
 * 所以必须带可诊断信息上抛，不能静默返回 null 让上层一直转圈。
 */
class DaemonProbe(
    private val channel: RemoteCommandChannel,
    private val daemonPath: String,
) {
    /**
     * 探测结果。[hello] 为 null 表示**探测失败**，[failure] 给出可诊断的原因。
     */
    data class Result(
        val hello: JsonlFrame.Hello?,
        val failure: String? = null,
        /**
         * 真正被探测的那条路径。
         *
         * 没填路径时 [DaemonLocator] 从候选表里挑一条，后续所有命令都必须用这一条
         * （用 `daemonPath` 会拿占位名去拼命令）。
         */
        val resolvedPath: String? = null,
    ) {
        val ok: Boolean get() = hello != null

        /** 对端声明的能力；探测失败或对端未声明 ⇒ 空集（一个 flag 都不发）。 */
        val capabilities: List<String> get() = hello?.capabilities ?: emptyList()

        /** 对端声明会发哪些帧型；用于「该不该订阅某类事件」的降级判断。 */
        val emits: List<String> get() = hello?.emits ?: emptyList()

        /** 对端自报的版本，判断对端版本的唯一依据。 */
        val buildId: String? get() = hello?.buildId

        /**
         * 版本闸门：这一版够不够新，能不能用某项 wire 能力。探测失败 ⇒ 没有 `buildId` ⇒ 不放行。
         * 判法只有一份，住在 [WireFeature.isSpokenBy]。
         */
        fun supports(feature: WireFeature): Boolean = feature.isSpokenBy(buildId)
    }

    /**
     * 定位 + 版本闸门合起来的三个失败终态。三句话不共用文案，压成一句会让人往错的方向修：
     * 「没装」要去装，「太旧」要去升级（升级它的不是这个应用），「装了但不说话」多半是参数或权限。
     */
    sealed interface Readiness {
        /** 屏上那句话；只有 [Ready] 没有话要说。 */
        val message: String?

        /** 找到了、够新、也说话了。 */
        data class Ready(
            val path: String,
            val hello: JsonlFrame.Hello,
        ) : Readiness {
            override val message: String? get() = null
        }

        /** ① 候选表逐条问过，一个都不在。 */
        data class NotInstalled(
            val tried: List<DaemonLocator.Candidate>,
        ) : Readiness {
            override val message: String get() = DaemonLocator.notFoundMessage(tried)
        }

        /** ② 找到了，但这一版太旧（或版本号认不出 ⇒ 保守当成太旧）。 */
        data class TooOld(
            val path: String,
            val buildId: String?,
            val feature: WireFeature,
        ) : Readiness {
            override val message: String get() = tooOldMessage(path, buildId, feature)
        }

        /** ③ 找到了，但它没说话；带 [probe] 那句三选一诊断。 */
        data class Speechless(
            val path: String?,
            val diagnosis: String,
        ) : Readiness {
            override val message: String get() = diagnosis
        }
    }

    /**
     * 跑一次裸流模式，等第一帧 `hello`。
     *
     * @param timeoutMs 等 `hello` 的上限。冷启（首次 SSH + 进程启动）比稳态慢得多，
     *   没有默认值，调用方按场景给。
     */
    suspend fun probe(timeoutMs: Long): Result {
        // 填了就只用它，不回退；没填（= 界面上那个占位名）才走候选表，多花一次往返。
        val userPath = DaemonLocator.userPathOrNull(daemonPath)
        if (userPath != null) return probeAt(userPath, timeoutMs)
        return when (val located = DaemonLocator.locate(channel, daemonPath, timeoutMs)) {
            is DaemonLocator.Outcome.Found -> probeAt(located.path, timeoutMs)
            is DaemonLocator.Outcome.NotFound -> Result(null, DaemonLocator.notFoundMessage(located.tried))
            // 「问不出来」不许说成「没装」，否则会让人去改一个本来就对的配置。
            is DaemonLocator.Outcome.ChannelFailed -> Result(null, "找 daemon 装在哪这一步就失败了：${located.why}")
        }
    }

    /**
     * 在一条确定的路径上拿 `hello`。定位已经做完（或直接指定），这里只管说话那一半。
     */
    private suspend fun probeAt(
        path: String,
        timeoutMs: Long,
    ): Result {
        // 裸命令：此刻还不知道对端能力，发任何 flag 都可能触发无限重连。
        val cmd = DaemonCommands.stream(path)
        return try {
            val hello =
                withTimeout(timeoutMs) {
                    DaemonTransport(channel, cmd)
                        .frames()
                        .firstOrNull { it is JsonlFrame.Hello } as? JsonlFrame.Hello
                }
            if (hello != null) {
                Result(hello, resolvedPath = path)
            } else {
                // 流结束了却没有 hello，最典型的是后端落进了查询模式（exit 2、stdout 空）
                Result(
                    null,
                    "daemon 流结束但从未发出 hello。最可能：① 路径不对 ② daemon 未部署 " +
                        "③ 命令里混进了它不认识的参数（落进一次性查询模式 → exit 2）。命令：$cmd",
                    resolvedPath = path,
                )
            }
        } catch (e: TimeoutCancellationException) {
            // TimeoutCancellationException 是 CancellationException 的子类，必须排在下面那条重抛之前，
            // 否则超时会被当成调用方取消重抛出去。
            Result(
                null,
                "等 hello 超时（${timeoutMs}ms）。daemon 可能未启动或路径不对。" +
                    "命令：$cmd（${e.message}）",
            )
        } catch (e: kotlin.coroutines.cancellation.CancellationException) {
            // 必须先行重抛，且排在 IllegalStateException 之前：JVM 上 CancellationException
            // 继承 IllegalStateException。不重抛的话离屏取消会被报成一个假故障，且取消不再向上传播。
            throw e
        } catch (e: java.io.IOException) {
            // 通道级失败（SSH 断、进程起不来）：带上原始异常，不许只说「连接失败」
            Result(null, "探测 daemon 的通道失败：${e::class.simpleName}: ${e.message}。命令：$cmd")
        } catch (e: IllegalStateException) {
            Result(null, "探测 daemon 状态异常：${e.message}。命令：$cmd")
        }
    }

    /**
     * 探测后直接给出可用的流模式命令，能力协商收在这一处。
     *
     * @param want 想要的能力；对端没声明的静默降级（旧后端是正常情况不是错误）。
     * @return 命令串；探测失败返回 null（调用方须处理，别拿 null 拼命令）。
     */
    suspend fun streamCommand(
        timeoutMs: Long,
        want: Collection<String>,
    ): Pair<String?, Result> {
        val r = probe(timeoutMs)
        // 必须用定位定下来的那条路径：拿 `daemonPath` 拼会把界面上的占位名送到远端。
        val cmd = if (r.ok) DaemonCommands.stream(r.resolvedPath ?: daemonPath, r.capabilities, want) else null
        return cmd to r
    }

    /**
     * 接控制面之前先问一句：三个失败终态，三句不同的话。拿不到退出码，所以「太旧」与「不存在」
     * 分两半答：
     *
     * | 要分辨什么 | 谁答 | 怎么答 |
     * |---|---|---|
     * | 在不在 | [DaemonLocator] | `command -v` / `[ -x ]`，不执行候选 |
     * | 是哪一版 | 本函数 | 裸流 `hello.buildId`（[WireFeature] 门槛表） |
     * | 在、也够新，但说不出话 | [probe] | 三选一诊断 |
     *
     * 不发 `--daemon-probe` 探版本：它本身就要求够新的版本，旧对端不认识它会落进一次性查询。
     *
     * 与 [probe] 的差别：本函数总是先定位（哪怕设置里填了路径），因为「填的那条不存在」
     * 和「它在但不说话」必须分开说。
     */
    suspend fun ready(
        feature: WireFeature,
        timeoutMs: Long,
    ): Readiness =
        when (val located = DaemonLocator.locate(channel, daemonPath, timeoutMs)) {
            is DaemonLocator.Outcome.NotFound -> Readiness.NotInstalled(located.tried)
            is DaemonLocator.Outcome.ChannelFailed ->
                Readiness.Speechless(null, "找 daemon 装在哪这一步就失败了：${located.why}")
            is DaemonLocator.Outcome.Found -> readyAt(located.path, feature, timeoutMs)
        }

    private suspend fun readyAt(
        path: String,
        feature: WireFeature,
        timeoutMs: Long,
    ): Readiness {
        val r = probeAt(path, timeoutMs)
        val hello = r.hello ?: return Readiness.Speechless(path, r.failure ?: "daemon 没说话（无更多信息）")
        // 认不出 / 没说一律落进「不够新」，绝不放行。
        return if (r.supports(feature)) Readiness.Ready(path, hello) else Readiness.TooOld(path, r.buildId, feature)
    }

    companion object {
        /**
         * 三个失败终态之一：找到了但太旧。
         *
         * 「升级它的不是这个应用」是刻意的：`~/.cc-monitor/bin/` 与别的客户端共用、谁后放谁生效，
         * 我们不动它，所以界面上不放一个做不到的动作按钮。
         */
        fun tooOldMessage(
            path: String,
            buildId: String?,
            feature: WireFeature,
        ): String =
            buildString {
                append("这台服务器上的 cc-monitor 是 ")
                append(buildId ?: "一个说不出版本号的版本")
                append("，")
                append(feature.what)
                append("这项要 ≥ ")
                append(feature.minBuildId)
                append("。\n它装在：")
                append(path)
                append("\n**升级它的不是这个应用** —— 这个落点是和别的客户端共用的，我们不动它。")
            }
    }
}

/**
 * wire 能力 → 最小 `BUILD_ID` 的门槛表，「够不够新」只在这里判。
 *
 * 版本序（`p1r < p1t < p1u < p2a`）是我们按形状推的，不是对端承诺的；对端挪了某项能力的门槛，
 * 这里不会知道。每一行带 [address]，对账时按它去核。
 */
enum class WireFeature(
    /** 门槛：`hello.build_id` 不低于它才算够新。 */
    val minBuildId: String,
    /** 人话：这项能力是干什么的（进「太旧」那句话）。 */
    val what: String,
    /** 这条门槛的出处，对账时按它去核。 */
    val address: String,
) {
    /**
     * 会话消失时分得清「被顶替」还是「真的没了」。
     *
     * 缺 `cause` ⇒ 读成「真的没了」⇒ `/branch`、`/clear` 之后那条对话从总览面无声消失。
     */
    REMOVAL_CAUSE(
        minBuildId = "p1t-removal-cause",
        what = "分得清「对话被顶替」和「对话真的没了」",
        address = "JsonlFrame.SessionRemoved.cause + DaemonSessionSource 里那个 superseded 字面量",
    ),

    /**
     * 一条对话能不能被别的客户端接管。
     *
     * 缺席 ⇒ 读成 `true` ⇒ 会话对别的客户端默认可接管。
     */
    ATTACHABLE(
        minBuildId = "p1v-attachable",
        what = "知道一条对话能不能被接管",
        address = "JsonlFrame.SessionAdded.attachable 的 KDoc（逐字：最小 BUILD_ID = p1v-attachable）",
    ),

    /**
     * 控制面（`launch` / `resolve` / `ping` / `cancel` / `kill` / `bus-*`）。
     *
     * 够新的后端在 hello 里自报 `commands` 列表。那比 buildId 更硬（对端自己说的），
     * 这里先只用 buildId；接控制面时应当先问 `commands`，本行降级成兜底。
     */
    CONTROL_PLANE(
        minBuildId = "p2d-relay",
        what = "发得出 launch / resolve / ping 这类控制命令",
        address = "后端 hello 的 commands 字段从 p2d-relay 起才有；更早的版本不认这些控制命令",
    ),
    ;

    /**
     * 这台后端（按它自报的 [buildId]）说得出这一项吗。全仓唯一的「够不够新」判定，别在别处另写比较器。
     *
     * 只有 [BuildIdVerdict.NEW_ENOUGH] 算通过，「认不出」与「没说」一律并进不够新。
     * 调用方：[DaemonProbe.Result.supports] 与 `DaemonSessionSource` 的下线分派（帧上的 `cause` 采不采信）。
     */
    fun isSpokenBy(buildId: String?): Boolean = BuildIds.atLeast(buildId, minBuildId) == BuildIdVerdict.NEW_ENOUGH
}

/** [BuildIds.atLeast] 的三档判定。「认不出」与「太旧」分开表达，再一起并进「不放行」。 */
enum class BuildIdVerdict {
    /** 够新（版本序是推的，见 [WireFeature]）。 */
    NEW_ENOUGH,

    /** 认得出，但比门槛旧。 */
    TOO_OLD,

    /** 认不出（缺席 / 格式变了 / 解析挂了），一律按不放行处理。 */
    UNRECOGNIZED,
}

/**
 * `BUILD_ID` 的比较器。形是 `p<主版本数字><字母段>[-任意后缀]`，如 `p1t-removal-cause`。
 *
 * 先比主版本数字（`p1z` < `p2a`），再比字母段；字母段先比长度再比字典序（`p1z` < `p1aa`）。
 *
 * fail-safe：`build_id` 缺席、格式不认识（`dev` / 空串）或解析挂了，都落 [BuildIdVerdict.UNRECOGNIZED]，
 * 调用方当不够新。反过来会对未知版本的后端发它不认识的参数 ⇒ 一次性查询 ⇒ exit 2 ⇒ 无限重连。
 *
 * 注意：形状像认得出的新标签（如 `p1r2`）不会触发 fail-safe，会得到一个自信的错判。
 */
object BuildIds {
    /** `p` + 主版本数字 + 字母段 + 可选 `-后缀`。后缀是人话标签，不参与比较。 */
    private val SHAPE = Regex("^p([0-9]+)([a-z]+)(?:-.*)?$")

    /** 排序键；认不出 ⇒ null。 */
    fun rank(buildId: String?): Pair<Int, String>? {
        val m = SHAPE.matchEntire(buildId?.trim().orEmpty()) ?: return null
        val major = m.groupValues[1].toIntOrNull() ?: return null
        return major to m.groupValues[2]
    }

    /** 三档判定。[min] 是我们自己的常量，必须认得出（认不出说明表写坏了，那是我们的 bug）。 */
    fun atLeast(
        buildId: String?,
        min: String,
    ): BuildIdVerdict {
        val theirs = rank(buildId) ?: return BuildIdVerdict.UNRECOGNIZED
        val ours = requireNotNull(rank(min)) { "门槛表里这条 BUILD_ID 我们自己都认不出：$min" }
        val byMajor = theirs.first.compareTo(ours.first)
        val cmp = if (byMajor != 0) byMajor else compareLetters(theirs.second, ours.second)
        return if (cmp >= 0) BuildIdVerdict.NEW_ENOUGH else BuildIdVerdict.TOO_OLD
    }

    /** 字母段：先长度后字典序（`z` < `aa`）。纯字典序会把 `aa` 排到 `b` 前面。 */
    private fun compareLetters(
        a: String,
        b: String,
    ): Int = if (a.length != b.length) a.length.compareTo(b.length) else a.compareTo(b)
}
