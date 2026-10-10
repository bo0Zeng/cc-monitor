package com.ccmonitor.mobile.ui.chat

import androidx.compose.runtime.Composable
import androidx.compose.runtime.produceState
import com.ccmonitor.mobile.agent.agentKindOrDefault
import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import com.ccmonitor.mobile.core.claude.command.ClaudeInvocation
import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.core.claude.transport.ClaudePaths
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.data.repo.LauncherRepository
import com.ccmonitor.mobile.core.data.repo.SettingsRepository
import kotlinx.coroutines.flow.first
import org.koin.compose.koinInject

/**
 * 这条对话在哪个目录、用哪条命令跑。
 *
 * cwd 决定 Claude 能不能看见项目的 `CLAUDE.md`、代码和 git 仓，
 * 也决定会话记录落进 `~/.claude/projects/<编码后的 cwd>/` 哪个目录；给错了，对话就和会话发现、续聊对不上。
 * 本文件不做新判定：工作目录取 `Host.defaultWorkingDir`，命令交给 `ClaudeInvocation.resolveLaunchCommand`。
 */
data class ChatLaunchContext(
    /** Claude 在哪个目录里干活。null = 那台主机没配默认工作目录。 */
    val workdir: String?,
    /** 用哪条命令起（`cc`/`cct`/自定义）。恒非空 —— 候选链兜底 `claude`。 */
    val launchCommand: String,
    /**
     * 用户显式覆盖过的 Claude 配置目录；null 表示没覆盖，听远端自己的。
     * 见 `ClaudeInvocation.pipeInvocation` 的 `claudeDir` 参数。
     */
    val claudeDir: String? = null,
    /**
     * 这台机器上的账号用户选过没有。
     *
     * 与 [claudeDir] 不是一回事：显式选了「听远端自己的」时 [claudeDir] 也是 null，但这里是 `true`。
     * 拿 `claudeDir == null` 当「没选过」会把这种用户永远挡在门外，判据见 [hasChosenAccount]。
     */
    val accountChosen: Boolean = false,
) {
    companion object {
        /** 还没读出来时的占位。注意：不能拿它起管道，那等于用错的 cwd 起。 */
        val PENDING = ChatLaunchContext(workdir = null, launchCommand = "")
    }
}

/**
 * 读出这台主机的启动上下文。
 *
 * @return `null` 表示还在读（DB 往返）。调用方必须等，拿 `null` 当「没有」去起管道就会跑在错的目录。
 */
@Composable
fun rememberChatLaunchContext(hostId: String): ChatLaunchContext? {
    val hosts = koinInject<HostRepository>()
    val launchers = koinInject<LauncherRepository>()
    val settings = koinInject<SettingsRepository>()
    return produceState<ChatLaunchContext?>(initialValue = null, hostId) {
        val host = runCatching { hosts.get(hostId) }.getOrNull()
        val candidates = runCatching { launchers.observeForHost(hostId).first().map { it.command } }.getOrNull().orEmpty()
        val appDefault = runCatching { settings.getDefaultClaudeDir() }.getOrNull()
        value =
            ChatLaunchContext(
                workdir = host?.defaultWorkingDir?.trim()?.ifEmpty { null },
                // 白名单与 fail-closed 兜底都在 resolveLaunchCommand 里，这里不重复。
                launchCommand = ClaudeInvocation.resolveLaunchCommand(candidates),
                // 优先级：主机覆盖 > 应用默认 > 远端自己的，见 [ClaudePaths.resolveClaudeDir]。
                claudeDir = explicitClaudeDir(host?.claudeDir, appDefault),
                // 取原始字段而非上面的解析结果，并按 agent 种类分档，见 [accountChosenFor]。
                accountChosen = accountChosenFor(host?.agentKindOrDefault(), host?.claudeDir, appDefault),
            )
    }.value
}

/**
 * 解析出显式覆盖值；没覆盖过就是 null。
 *
 * 判据是解析结果不等于默认表达式 `ClaudePaths.DEFAULT_CLAUDE_DIR`，而不是主机字段非空：应用级默认同样算覆盖。
 * 默认表达式的意思是「听远端自己的」，这时什么都不设才对，硬塞一个值会覆盖远端的选择。
 */
fun explicitClaudeDir(
    hostClaudeDir: String?,
    appDefault: String?,
): String? = ClaudePaths.resolveClaudeDir(hostClaudeDir, appDefault).takeIf { it != ClaudePaths.DEFAULT_CLAUDE_DIR }

/**
 * 这台机器上的账号用户选过没有。
 *
 * 没选就起，命令会跑成远端默认账号；那个账号没登录时只回一句 `api_error`，用户看不出要去选账号。
 * 所以起之前就拦住并说人话（`ChatRoute` 的 `NO_ACCOUNT_CHOSEN`）。
 *
 * 判据取原始字段，不取 [explicitClaudeDir]：后者回答「传什么给远端」，「没覆盖」与「没选过」在它那里同为 null。
 * 不管的情形：选了目录不等于那个目录登录着；用 `CLAUDE_CONFIG_DIR` 选号的远端需手动填绝对路径。
 *
 * @param hostClaudeDir `hosts.claudeDir` 的原值（每主机覆盖）
 * @param appDefault 应用级默认（设置屏那个），同样算「选过」
 */
fun hasChosenAccount(
    hostClaudeDir: String?,
    appDefault: String?,
): Boolean = !hostClaudeDir.isNullOrBlank() || !appDefault.isNullOrBlank()

/**
 * 账号门只对需要它的档生效。
 *
 * Codex 服务器的编辑页上配置目录是可选的，只读档本来就不起新对话，对它们拦门是一句假要求。
 * [kind] 为 `null`（主机读失败）时按缺省档，也就是 Claude 档处理：宁可多问一次，也不放一台 Claude 服务器没账号跑。
 *
 * @param kind 这台服务器的 agent 种类；`null` 表示没读出来。
 * @param hostClaudeDir `hosts.claudeDir` 的原值，原样转交 [hasChosenAccount]。
 * @param appDefault 应用级默认，原样转交 [hasChosenAccount]。
 */
fun accountChosenFor(
    kind: AgentKind?,
    hostClaudeDir: String?,
    appDefault: String?,
): Boolean {
    // 走不走账号门由档案决定；读不出种类时用缺省档，即保守的一侧。
    val profile = kind?.let(AgentProfile::of) ?: AgentProfile.DEFAULT
    return !profile.requiresAccountGate || hasChosenAccount(hostClaudeDir, appDefault)
}
