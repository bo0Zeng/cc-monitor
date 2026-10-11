package com.ccmonitor.mobile.ui.nav

import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import com.ccmonitor.mobile.core.claude.model.AgentKind

/**
 * UI 上的产品名。上屏一律走这里，不硬写「Claude」：硬写的字在 Codex 服务器上是假话。
 * 名字本身住 `AgentProfile.displayName`，这里是 `:app` 层的读法。
 */
fun agentDisplayName(kind: AgentKind): String = AgentProfile.of(kind).displayName
