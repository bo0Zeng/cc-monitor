package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.ccmonitor.mobile.core.remote.shellQuote
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.withTimeout

/**
 * 后端程序装在哪：一份有序候选表 ＋ 远端存在性判定。部署不进 PATH，名字只是猜测，
 * 每个猜测都要远端给出肯定答复才算数。
 *
 * 三条边界：
 * 1. 定位不许把候选跑起来。裸跑 `ccm` 这类启动器会在远端真起一条 Claude 会话；
 *    只许 `command -v --`（PATH 名字）与 `[ -x … ]`（路径）。远端执行通道只搬 stdout、拿不到退出码，
 *    「跑一下看退出码」本来也走不通。
 * 2. `$HOME` 那几条不许走 [shellQuote]：它无条件加单引号，`~` 与 `$HOME` 都不会展开。
 *    `$HOME` 前缀走 [HOME_PREFIX]（双引号，远端展开），后面那截是我们自己的字面常量。
 * 3. 远端把展开后的绝对路径原样打回来；否则手里只有只在远端有意义的 `"$HOME"/…`，
 *    一进 [DaemonCommands.stream] 就被 [shellQuote] 引死。
 *
 * 候选序（是序不是集合）：
 *
 * | # | 候选 | 判定 | 为什么在表上 |
 * |---|---|---|---|
 * | ① | 设置里填的 | 填了就只用它，不回退 | 回退会把显式选择变成猜 |
 * | ② | `"$HOME"/.cc-monitor/bin/ccm` | `[ -x … ]` | 部署落点 |
 * | ③ | PATH 上的 `ccm` | `command -v -- ccm` | 家目录不对时的兜底 |
 *
 * ② 在 ③ 前：② 是确定的路径，③ 是可能指向别的东西的名字。
 *
 * 注意：PATH 上的 `ccm` 可能是一份旧的 bash 启动器，不是后端。敢把它放上表靠三条：
 * 定位不执行任何候选；之后发的第一条命令是 `'<path>' -- --stream` 而不是裸路径；
 * [DaemonProbe] 要收到 `hello` 才算成功，咬错了会响亮地失败。这三条只保证「咬错了不致命」，
 * 不是身份验证。
 *
 * 落点换了目录或改了名，候选全落空，表现与「没装」一样；所以 [notFoundMessage] 必须逐条说出试过哪几个。
 */
object DaemonLocator {
    /**
     * 设置里那一栏没填时摆在界面上的占位名（= `ConversationsRoute.DEFAULT_SOURCE_PATH` 的值）。
     *
     * 它同时是哨兵：`DaemonSessionSource` 收到的 `daemonPath` 等于它，就说明没填 ⇒ 走候选表。
     * 它只参与「填没填」的判断，从不进命令串。手动逐字敲这个名字也会被当成没填。
     */
    const val UNSET_PLACEHOLDER: String = "cc-monitor-remote"

    /**
     * 后端二进制的名字。它与「裸跑会起会话的启动器」同名但不同物：本类不裸跑任何东西。
     */
    const val BACKEND_NAME: String = "ccm"

    /**
     * 共用落点的目录名（相对 `$HOME`）。
     *
     * 只读不写：这个目录与桌面端共用、各自只校验自己上传的字节，谁后放谁生效。
     * 我们一个字节都不写进去；联调用的后端装在 `~/.aterm/bin/`。
     */
    const val SHARED_BIN_DIR: String = ".cc-monitor/bin/"

    /**
     * `$HOME` 前缀 —— **双引号**，交给远端 shell 展开。
     *
     * 逐字是 `"$HOME"/`：`~` 在非交互 shell 的某些位置不展开，裸 `$HOME` 遇到空格会被 word-split。
     */
    const val HOME_PREFIX: String = "\"\$HOME\"/"

    /**
     * 一条候选。
     *
     * @param where 人话住址，进「试过哪几个」那份清单（[notFoundMessage]），不进命令。
     * @param rendered 进远端命令的那一段，已经决定好要不要 quote，调用方不许再动它。
     * @param byName true = PATH 上的名字（`command -v`）；false = 一条路径（`[ -x ]`）。
     */
    data class Candidate(
        val where: String,
        val rendered: String,
        val byName: Boolean,
    )

    /** 定位的三种结局。 */
    sealed interface Outcome {
        /** 试过哪几个（永远填，失败时要逐条说出来）。 */
        val tried: List<Candidate>

        /** 找到了。[path] 是远端打回来的、已展开的路径，可以直接交给 [shellQuote]。 */
        data class Found(
            val path: String,
            val candidate: Candidate,
            override val tried: List<Candidate>,
        ) : Outcome

        /** 命令跑完了，一条都没命中。 */
        data class NotFound(
            override val tried: List<Candidate>,
        ) : Outcome

        /**
         * 命令没跑完（通道断 / 超时 / 没见到成功标记）。
         * 与 [NotFound] 必须分开：把「问不出来」说成「没装」，会让人去改一个本来就对的配置。
         */
        data class ChannelFailed(
            override val tried: List<Candidate>,
            val why: String,
        ) : Outcome
    }

    /**
     * 设置里那一栏的值 → 「真填了吗」。
     *
     * 空 / 全空白 / 等于 [UNSET_PLACEHOLDER] ⇒ `null`（没填，走整张候选表）。
     */
    fun userPathOrNull(configured: String?): String? {
        val t = configured?.trim()
        return if (t.isNullOrEmpty() || t == UNSET_PLACEHOLDER) null else t
    }

    /**
     * 候选表，顺序就是判定顺序。
     *
     * @param userPath 显式填的那条；非 null ⇒ 只有它一条，不回退。
     */
    fun candidates(userPath: String?): List<Candidate> {
        if (userPath != null) return listOf(userCandidate(userPath))
        return listOf(
            // ② 部署落点。`$HOME` 前缀走 HOME_PREFIX（双引号），不许走 shellQuote。
            Candidate("$HOME_PREFIX$SHARED_BIN_DIR$BACKEND_NAME", "$HOME_PREFIX$SHARED_BIN_DIR$BACKEND_NAME", byName = false),
            // ③ 有歧义：`~/.local/bin/` 里可能有一份同名的旧 bash 启动器，命中它不等于咬到了后端。
            //    敢留它的理由见类注释。
            Candidate("PATH 上的 $BACKEND_NAME", shellQuote(BACKEND_NAME), byName = true),
        )
    }

    /**
     * 用户填的那条怎么渲染。
     *
     * - `~/x` 与 `$HOME/x` → [HOME_PREFIX] + quote 掉剩下那截（直接 [shellQuote] 会把 `~` 引死）。
     * - 带 `/` 的其余值 → 当路径，整条 quote。
     * - 不带 `/` → 当 PATH 上的名字。
     */
    private fun userCandidate(userPath: String): Candidate {
        val homeRelative =
            when {
                userPath.startsWith("~/") -> userPath.removePrefix("~/")
                userPath.startsWith("\$HOME/") -> userPath.removePrefix("\$HOME/")
                else -> null
            }
        return when {
            homeRelative != null ->
                Candidate("设置里填的：$userPath", HOME_PREFIX + shellQuote(homeRelative), byName = false)
            userPath.contains('/') ->
                Candidate("设置里填的：$userPath", shellQuote(userPath), byName = false)
            else ->
                Candidate("设置里填的：$userPath（当成 PATH 上的名字）", shellQuote(userPath), byName = true)
        }
    }

    /**
     * 拼定位命令。整条只由固定模板 + 受控插值组成，不许多出任何片段（如 `|| X`）。
     */
    fun probeCommand(candidates: List<Candidate>): String =
        DaemonCommands.locate(
            candidates.mapIndexed { i, c -> DaemonCommands.presence(i, c.rendered, c.byName) },
        )

    /**
     * 读远端回答。
     *
     * - 没见到 [DaemonCommands.QUERY_OK_MARKER] ⇒ [Outcome.ChannelFailed]（命令没跑完）。
     * - 跑完了但没有 HIT 行 ⇒ [Outcome.NotFound]。
     * - 有 HIT 行 ⇒ 取第一条（输出序 = 候选序 ⇒ 第一条优先级最高）。
     */
    fun readAnswer(
        stdout: String,
        candidates: List<Candidate>,
    ): Outcome {
        val body =
            DaemonCommands.bodyOrNullIfFailed(stdout)
                ?: return Outcome.ChannelFailed(
                    candidates,
                    "定位命令没有跑完（没见到 ${DaemonCommands.QUERY_OK_MARKER} 标记）",
                )
        val hit =
            body
                .lineSequence()
                .map { it.trim() }
                .firstOrNull { it.startsWith("${DaemonCommands.LOCATE_HIT_MARKER} ") }
                ?: return Outcome.NotFound(candidates)
        // 形：`ATERM_LOC_HIT <序号> <远端展开后的绝对路径>`
        val rest = hit.removePrefix("${DaemonCommands.LOCATE_HIT_MARKER} ")
        val index = rest.substringBefore(' ').toIntOrNull()
        val path = rest.substringAfter(' ', missingDelimiterValue = "").trim()
        if (index == null || index !in candidates.indices || path.isEmpty()) {
            return Outcome.ChannelFailed(candidates, "定位命令的回答读不懂：$hit")
        }
        return Outcome.Found(path, candidates[index], candidates)
    }

    /**
     * 跑一次定位。
     *
     * @param configured 设置里那一栏的原值（可能是 [UNSET_PLACEHOLDER]）。
     * @param timeoutMs 上限。定位比等 `hello` 便宜得多，但冷启的 SSH 握手一样要算进去。
     */
    suspend fun locate(
        channel: RemoteCommandChannel,
        configured: String?,
        timeoutMs: Long,
    ): Outcome {
        val candidates = candidates(userPathOrNull(configured))
        val cmd = probeCommand(candidates)
        return try {
            val out =
                withTimeout(timeoutMs) {
                    val sb = StringBuilder()
                    channel.exec(cmd).collect { sb.append(String(it, Charsets.UTF_8)) }
                    sb.toString()
                }
            readAnswer(out, candidates)
        } catch (e: TimeoutCancellationException) {
            // TimeoutCancellationException 是 CancellationException 的子类，必须排在下面那条重抛之前。
            Outcome.ChannelFailed(candidates, "定位超时（${timeoutMs}ms）：${e.message}")
        } catch (e: kotlin.coroutines.cancellation.CancellationException) {
            // 取消不是故障，原样向上传播：否则离屏会被报成假故障，且破坏结构化并发。
            throw e
        } catch (e: java.io.IOException) {
            Outcome.ChannelFailed(candidates, "定位的通道失败：${e::class.simpleName}: ${e.message}")
        }
    }

    /**
     * 三个失败终态之一：找不到。与「太旧」「没说话」不共用文案，压成一句会让人往错的方向修。
     * 逐条列出试过哪几个，否则既不知道找过哪里，也不知道该往哪里放。
     */
    fun notFoundMessage(tried: List<Candidate>): String =
        buildString {
            append("这台服务器上没找到 cc-monitor。找过这 ${tried.size} 个地方：\n")
            for (c in tried) append("  · ").append(c.where).append('\n')
            append("去设置里填一条它的路径（填绝对路径；开头的 ~/ 和 \$HOME/ 会自动展开）。")
        }
}
