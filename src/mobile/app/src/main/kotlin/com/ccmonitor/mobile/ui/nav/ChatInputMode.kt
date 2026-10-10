package com.ccmonitor.mobile.ui.nav

import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import com.ccmonitor.mobile.core.claude.model.AgentKind

/**
 * 一条对话的输入行档位，也是「这台服务器能不能说话」的唯一判定源。
 *
 * 落地路由（[onConnectDestination] / [launchDestination]）也读它：输入行都不出现的档，
 * 不拿聊天屏当落地点，否则 Codex 服务器会落进 Claude 的聊天屏。
 */
enum class ChatInputMode {
    /** 正常：能打字、能发。 */
    Editable,

    /** 灰掉但在屏上：电脑上正在等回答，答完就恢复。与 [Hidden] 的差别是这一档会好起来。 */
    Disabled,

    /**
     * 整条不渲染（Codex 只读档）。上行（`ChatSession` / `UplinkSink` / `PipeLauncher`）只按
     * Claude CLI 的管道形状写，Codex 发不了；灰掉暗示会好起来，所以干脆不出现。
     */
    Hidden,
}

/**
 * 这条对话的输入行档位。不能上行的档（Codex）优先于等待态：灰掉意味着「等会儿就能发」，对它是假的。
 *
 * 注意：生产上只有导航层在用，且只问 `== Hidden`；聊天屏还没按它隐藏输入行，
 * [waitingOnDesktop] 也还没有调用方传。
 *
 * @param waitingOnDesktop 电脑上正在等用户回答（`session_status == "waiting"`）。
 *   缺省 `false`：不知道就当没在等，不平白灰掉能用的输入框。
 */
fun chatInputMode(
    kind: AgentKind,
    waitingOnDesktop: Boolean = false,
): ChatInputMode =
    when {
        !AgentProfile.of(kind).supportsUplink -> ChatInputMode.Hidden
        waitingOnDesktop -> ChatInputMode.Disabled
        else -> ChatInputMode.Editable
    }

/**
 * UI 上的产品名。上屏一律走这里，不硬写「Claude」：硬写的字在 Codex 服务器上是假话。
 * 名字本身住 `AgentProfile.displayName`，这里是 `:app` 层的读法。
 */
fun agentDisplayName(kind: AgentKind): String = AgentProfile.of(kind).displayName
