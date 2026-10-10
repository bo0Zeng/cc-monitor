package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import com.ccmonitor.mobile.core.claude.command.ClaudeInvocation
import com.ccmonitor.mobile.core.remote.shellQuote

/**
 * tmux 这一面的唯一命令产地。纯函数，只造串不执行，别处只拿造好的结果；谁来跑、跑在哪是
 * `TmuxBackend` 的事。聊天屏不经这里：起会话、往会话里打字走那台核心的 `session-new` · `terminal-input`。
 *
 * 收的是 tmux 语法（子命令、flag、格式串、哨兵），不收用户的字：种子按钮的默认命令（如 `"tmux new -A -s main"`）、
 * 按钮显示名、菜单文案都是用户可改的内容，不和远端说话，不放这里。
 * 比对本对象哨兵的几支解析（[isResumeForegroundShell]、[paneConfirmedNoChild]、[tmuxKillSucceeded]）也在这里，
 * 命令和解析同住才不会一边改了另一边没改。注意：读 [tmuxListCommand] 输出的 `parseTmuxList` 在 `ui/session/TmuxParse.kt`，
 * 改格式串要一起改。
 *
 * 不做的事：
 * - 不造会话名，名字由调用方给；会话名前缀常量也不在这里（`cc-` / `cx-` 在各家 invocation）。
 * - 不管 agent 载荷：`--resume` 那截向 `AgentProfile.invocation` 要。
 * - 不改 tmux server 的全局状态：每条 `set` 都带 `-t` 指名一个会话，不许 `-g` / `-s`，不碰服务器级选项。
 *   那台机器上的 tmux server 是共享的，cc-monitor 的 `cc-*` 会话和电脑上开的会话都在上面。
 */
@Suppress("TooManyFunctions") // 一个协议面一个网关；tmux 子命令就有这么多支，拆开就又散了
object TmuxCommands {
    // ---- 移动端显示设置（只对我们进的那一个会话） --------------------------------

    /**
     * 移动端适配：进一个会话之前，只对那一个会话设两条会话级选项。
     * - `status off`：关掉底部状态栏，它占一行，键盘弹起时挤输入框。
     * - `mouse on`：终端见 alt-screen 加鼠标模式会上报滚轮，滑动就能滚 tmux 里的 Claude / vim / less。
     *   长按仍是终端自己的选区复制：终端只转发滚轮，不转发按键和拖拽，触发不了 copy-mode。
     *
     * 不设服务器级的 `terminal-overrides`：它没有会话作用域，`-gu` 会把整台 server 的值重置成默认，
     * 连用户 `.tmux.conf` 里自己加的（如真彩 `Tc` / `RGB`）一起抹掉。
     * 注意：server 上若留着禁备用屏的 override，手机上全屏 TUI 的滚轮上报会退化，要用户自己删掉或重启 server。
     *
     * 注意：在 tmux 管理器里主动附着一个不是我们起的会话（如电脑上的 `cc-*`）时，这两条照样设在那个会话上，
     * 电脑那头看它也会没了状态栏。会话级选项由挂在同一会话上的客户端共享；彻底解法是附着时起会话组
     * （`new-session -t <它> -s <我们的>`），尚未做。
     *
     * 用 [exactTarget] 精确匹配：裸 `-t <名>` 会按前缀命中兄弟会话，把设置落到别人的会话上。
     *
     * @return 空名字 → `null`：`=:` 会被解析成「当前会话」，那不是我们点名的会话。
     */
    fun tmuxMobileSetup(name: String): String? {
        val t = exactTarget(name) ?: return null
        return "tmux set -t $t status off 2>/dev/null; tmux set -t $t mouse on 2>/dev/null"
    }

    // ---- 进入 / 新建 -------------------------------------------------------------

    /**
     * 等价于 `tmux new -A -s <name> [-c <wd>]`：有则 attach，无则新建。
     * 拆成 `new-session -d` → [tmuxMobileSetup] → `attach`，保证移动端设置在 attach 之前已生效；
     * 直接 `new -A` 会先建会话再 attach，来不及设。`-c` 只在新建时生效。
     */
    fun tmuxNewOrAttach(
        name: String,
        workingDir: String? = null,
    ): String {
        val wd = workingDir?.let { " -c ${shQuote(it)}" } ?: ""
        return listOfNotNull(
            "tmux new-session -d -s ${shQuote(name)}$wd 2>/dev/null",
            tmuxMobileSetup(name),
            "tmux attach -t ${shQuote(name)}",
        ).joinToString("; ")
    }

    /**
     * 进入已存在会话：[alreadyInTmux] 为 true（已在某个 tmux 客户端里）用 `switch-client`，免得嵌套 attach；否则 `attach`。
     * 进之前对要进的会话设 [tmuxMobileSetup]：会话可能刚由前一段命令新建；会话级 `set` 幂等。
     */
    fun tmuxAttachCommand(
        name: String,
        alreadyInTmux: Boolean,
    ): String {
        val enter = if (alreadyInTmux) "tmux switch-client -t ${shQuote(name)}" else "tmux attach -t ${shQuote(name)}"
        return listOfNotNull(tmuxMobileSetup(name), enter).joinToString("; ")
    }

    /**
     * `tmux new-session -d -s <name>`：后台新建一个 detached 会话，不进入。
     * 不设移动端选项：此刻没有客户端挂着，之后从 app 进它时 [tmuxNewOrAttach] / [tmuxAttachCommand] 会设。
     */
    fun tmuxNewDetachedCommand(name: String): String = "tmux new-session -d -s ${shQuote(name)}"

    /**
     * 后台新建一个 detached 会话并在里面跑 [command]（如 claude），不进入。
     *
     * 用「建 shell 会话 + send-keys」而不是 `new-session -d -s n claude` 直接 exec：claude 这类命令常只在交互 shell 的
     * PATH 或别名里，直接 exec 找不到，命令立刻退出，会话跟着没了。send-keys 在交互 shell 里敲，环境完整。
     * 键写进 pane 的 pty，shell 起来后读走，所以不用在两段之间等。
     */
    fun tmuxNewDetachedRunning(
        name: String,
        command: String,
    ): String =
        "tmux new-session -d -s ${shQuote(name)}; " +
            "tmux send-keys -t ${shQuote(name)} ${shQuote(command)} Enter"

    // ---- resume：探测、定位、起、补送 ----
    // agent 载荷（sid 校验、启动命令解析、resume invocation、会话名、切模型）在 core-claude；这里只管 tmux 编排。

    /** 前台命令探测，[tmuxResumeCommand] 和 resume watchdog 共用。会话不存在 → 空输出。 */
    fun tmuxForegroundProbeCommand(name: String): String = "tmux display-message -p -t ${shQuote(name)} '#{pane_current_command}' 2>/dev/null"

    /** 探测输出是不是交互 shell（∈ [RESUME_FOREGROUND_SHELLS]）；watchdog 判「agent 没起来」用。 */
    fun isResumeForegroundShell(output: String): Boolean = output.trim() in RESUME_FOREGROUND_SHELL_SET

    // pane 子进程探测的四态输出，见 [tmuxPaneChildProbeCommand]。

    /** pgrep 明确无子进程 → 可以 kill（agent 真没起）。 */
    const val PANE_PROBE_NO_CHILD = "ATERM_PANE_NOCHILD"

    /** 有子进程 → 可能是包装脚本下的 agent 在跑，不 kill。 */
    const val PANE_PROBE_HAS_CHILD = "ATERM_PANE_HASCHILD"

    /** 没有 pgrep → 判不了，不 kill。 */
    const val PANE_PROBE_NO_PGREP = "ATERM_PANE_NOPGREP"

    /** pane 不存在（没有 pane_pid）→ 没东西可杀。 */
    const val PANE_PROBE_ABSENT = "ATERM_PANE_ABSENT"

    /**
     * kill 之前的防误杀探测。用户的 `cc` 若是不 exec 的包装脚本，`pane_current_command` 可能一直停在 shell 名，
     * 真正的 agent 是它的子进程；直接 `kill-session` 会杀掉正在干活的 agent。
     * 所以先取 `pane_pid` 再 `pgrep -P` 查子进程，输出四态之一，由 [paneConfirmedNoChild] 判定：
     * 只有明确无子进程才 kill，宁可留一个空会话。`pane_pid` 是纯数字，进双引号展开是安全的。
     */
    fun tmuxPaneChildProbeCommand(name: String): String {
        val q = shQuote(name)
        return "p=\$(tmux display-message -p -t $q '#{pane_pid}' 2>/dev/null); " +
            "if [ -z \"\$p\" ]; then echo $PANE_PROBE_ABSENT; " +
            "elif ! command -v pgrep >/dev/null 2>&1; then echo $PANE_PROBE_NO_PGREP; " +
            "elif pgrep -P \"\$p\" >/dev/null 2>&1; then echo $PANE_PROBE_HAS_CHILD; " +
            "else echo $PANE_PROBE_NO_CHILD; fi"
    }

    /** 只有输出明确是 [PANE_PROBE_NO_CHILD] 才可以 kill；其余一律不 kill。 */
    fun paneConfirmedNoChild(output: String): Boolean = output.trim() == PANE_PROBE_NO_CHILD

    /**
     * resume 一个 agent 会话：在远端按 sid 定位会话文件、读出权威 cwd，在该 cwd 新建 detached 会话 [name]，
     * send-keys 进那一家的 resume 载荷，再进入。随 pty 一行执行，Kotlin 侧没有往返。
     *
     * 随 agent 变的三样都问档案 [profile]，这里不出现任何一家的名字：定位命令（`sessionLocator.findBySessionIdCommand`）、
     * 载荷（`invocation.resumeInvocation`）、新建时打不打 `@ccm_sid`（`hasCcmIdentity`）。
     *
     * cwd：`j=$(定位命令)` → `grep -a -m1 -o '"cwd":"…"' | sed` 取文件里第一个 cwd（Codex 的在 session_meta 首行）
     * → 空则用 [fallbackCwd]。`[ -n "$cwd" ] && [ -d "$cwd" ]` 不过就不建会话，免得留下空会话，并在 pty 打印错误。
     * cwd 一律双引号变量展开，含空格和元字符安全；路径里含 `"` 时 sed 取不准，接受。
     * 注入：[sessionId] 先过 `isValidSessionId`，[fallbackCwd] 经 [shQuote]。
     *
     * 用 send-keys 不直接 exec，理由同 [tmuxNewDetachedRunning]。[launchCommand] 为 null 时用那一家的内置命令。
     * 包在 tmux 里：tab 关掉、SSH 断线时 agent 不死，回来 attach 即续。
     *
     * 按 `new-session` 成败分支发送，覆盖三态：
     * - 新建成功 → 无条件发。新 pane 里不可能有 agent 在跑；而且 `new-session -d` 刚返回时前台命令可能瞬时是 `tmux`
     *   （login shell 还没接管），此时用白名单判前台会漏发，watchdog 随后误杀空会话。
     * - 新建失败、前台是闲置 shell（会话残留、agent 已退）→ 补发。
     * - 新建失败、前台是 node 等（agent 正在跑）→ 不发，不往运行中的 agent 里敲命令。
     */
    fun tmuxResumeCommand(
        profile: AgentProfile,
        name: String,
        sessionId: String,
        agentDir: String,
        fallbackCwd: String?,
        alreadyInTmux: Boolean,
        launchCommand: String? = null,
    ): String {
        require(profile.invocation.isValidSessionId(sessionId)) { "非法 ${profile.displayName} sessionId: $sessionId" }
        val q = shQuote(name)
        val locate = "j=\$(${profile.sessionLocator.findBySessionIdCommand(agentDir, sessionId)})"
        // `grep -m1 -o` 取的是第一个含 cwd 的行，那一行可能输出多个片段；`head -n 1` 保证 `$cwd` 只有一行。
        val extract = "cwd=\$([ -n \"\$j\" ] && grep -a -m1 -o '\"cwd\":\"[^\"]*\"' -- \"\$j\" 2>/dev/null | head -n 1 | sed 's/.*\"cwd\":\"//;s/\"\$//')"
        val fallback = "[ -z \"\$cwd\" ] && cwd=${shQuote(fallbackCwd.orEmpty())}"
        val payload = shQuote(profile.invocation.resumeInvocation(launchCommand, sessionId))
        val send = "tmux send-keys -t $q $payload Enter"
        // 新建会话打 user-option `@ccm_sid=<完整 sid>`：cc-monitor 按它定位会话（不按 `cc-<sid8>` 名），没有它就不能
        // attach / kill。session 作用域（`-t` 指名）；sid 已过 isValidSessionId，字符集正是 cc-monitor 接受的。
        // 打不打由 [AgentProfile.hasCcmIdentity] 答。
        // 注意：cc-monitor 现行协议里 `@ccm_sid` 是它自己写的事实标记，建会话的一方该写 `@ccm_sid_expect`，这里仍写 `@ccm_sid`。
        // 用 `;` 接 send：标记是次要的，不许挡住 resume；老 tmux 没有 @-option 时 set 失败，`&&` 会把人丢进空 shell。
        val tagThenSend =
            if (profile.hasCcmIdentity) {
                "tmux set-option -t $q @ccm_sid ${shQuote(sessionId)} 2>/dev/null; $send"
            } else {
                send
            }
        // 按 `new-session` 成败分支，见 KDoc。`@ccm_sid` 只在新建分支打；会话已存在说明是重连，建时已经打过。
        val createAndSend =
            "if tmux new-session -d -s $q -c \"\$cwd\" 2>/dev/null; then $tagThenSend; " +
                "else case \"\$(${tmuxForegroundProbeCommand(name)})\" in ($RESUME_FOREGROUND_SHELLS) $send;; esac; fi"
        val fail = "echo 'aterm: resume 失败——无法确定工作目录（会话 ${sessionId.take(8)}）'"
        return "$locate; $extract; $fallback; " +
            "if [ -n \"\$cwd\" ] && [ -d \"\$cwd\" ]; then $createAndSend; ${tmuxAttachCommand(name, alreadyInTmux)}; " +
            "else $fail; fi"
    }

    /**
     * resume 前台探测的交互 shell 白名单（tmux `case` 模式，`|` 分隔），前台命中才发 resume 载荷。
     * 每个 shell 都含裸名和 login 的 `-` 变体。
     * 用白名单而不是「不是 node/claude 就发」：agent 的前台命令依启动方式可能是 node、claude、bun、包装脚本名，
     * 是开放集合；黑名单漏判会把命令敲进运行中的 agent，白名单漏判只是不发。
     */
    const val RESUME_FOREGROUND_SHELLS =
        "bash|-bash|sh|-sh|ash|-ash|dash|-dash|zsh|-zsh|ksh|-ksh|csh|-csh|tcsh|-tcsh|fish|-fish|login"

    /** [RESUME_FOREGROUND_SHELLS] 的集合形态，与 case 模式同源。 */
    private val RESUME_FOREGROUND_SHELL_SET = RESUME_FOREGROUND_SHELLS.split("|").toSet()

    // ---- kill -------------------------------------------------------------------

    /**
     * `tmux kill-session -t <name>`，带成功标记。exec 只拿得到 stdout，拿不到退出码和 stderr；
     * 不带标记的话，会话名对不上、server 已退时 stdout 也是空的，会被当成「已结束」。
     * `2>&1` 让诊断进 stdout，`&& echo` 只在真成功时打标记，判定见 [tmuxKillSucceeded]。
     */
    fun tmuxKillCommand(name: String): String = "tmux kill-session -t ${shQuote(name)} 2>&1 && echo $TMUX_KILL_OK_MARKER"

    /** [tmuxKillCommand] 的成功标记 —— 选一个不可能出现在 tmux 输出里的串。 */
    const val TMUX_KILL_OK_MARKER = "__aterm_kill_ok__"

    /**
     * [tmuxKillCommand] 真成功了吗：输出里有标记才算。
     * 不能反过来用「没有 stderr」：命令根本没跑起来时 stdout 也是空的。
     */
    fun tmuxKillSucceeded(output: String): Boolean = output.contains(TMUX_KILL_OK_MARKER)

    // ---- 查询 / 投递 -------------------------------------------------------------

    /**
     * 把 `/model <model>` 用 send-keys + Enter 投进会话 [session] 的活动 pane，在 Claude 里热切换模型。
     * 注意：只有 Claude 停在输入提示时才即时生效；任务中途会进输入缓冲，SSH 这头看不出来。
     */
    fun tmuxSendModelCommand(
        session: String,
        model: String,
    ): String = "tmux send-keys -t ${shQuote(session)} ${shQuote(ClaudeInvocation.modelCommand(model))} Enter"

    /** 抓会话 [session] 活动 pane 的可见屏，纯文本输出到 stdout（`-p`）。不含 scrollback。 */
    fun tmuxCapturePaneCommand(session: String): String = "tmux capture-pane -t ${shQuote(session)} -p"

    /**
     * 会话活动 pane 的当前路径，即该会话的工作目录。不 attach 读后台会话时，
     * 用它定位 `~/.claude/projects/<enc(cwd)>/` 下的 jsonl。
     */
    fun tmuxSessionCwdCommand(name: String): String = "tmux display-message -p -t ${shQuote(name)} '#{pane_current_path}'"

    /**
     * `tmux ls`，tab 分隔（会话名不能含 tab，可以含 `|`）。没有 server 时 stderr 被吞，输出为空。
     * 输出由 `ui/session/TmuxParse.kt` 的 `parseTmuxList` 解析。
     */
    @Suppress("FunctionOnlyReturningConstant")
    fun tmuxListCommand(): String =
        "tmux ls -F '#{session_name}\t#{session_windows}\t#{?session_attached,1,0}\t#{pane_current_command}' 2>/dev/null"

    /**
     * tmux `-t` 的精确匹配包装，别简化。
     *
     * 裸 `-t <名>` 依次按「精确名 → 名字开头 → glob」解析：只有 `sib-2` 存在时 `-t sib` 会落到 `sib-2`，
     * `cc-abc1234` 会命中 `cc-abc12345`。
     *
     * 尾冒号不能省：`=` 前缀只在 target-session 的解析路径上认；尾冒号把串变成 `session:`（当前 window、活动 pane），`=` 才落在会话名上。
     *
     * @return `null` = target 为空，必须拒：`=:` 会被解析成「当前会话」。门在函数里，调用方绕不过去。
     */
    fun exactTarget(target: String): String? = if (target.isEmpty()) null else shellQuote("=$target:")
}
