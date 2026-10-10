package com.ccmonitor.mobile.agent

import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.core.data.db.Host

/**
 * 把 Room 里存的 [Host.agentKind]（enum 名字符串；core-data 不依赖 core-claude）读成 [AgentKind]。
 * null 或认不出的名字走缺省档，规则在 [AgentProfile.ofStoredName]；这里只是 Room 边界上的薄读法。
 *
 * 注意：服务器编辑页回显用的是 [AgentProfile.ofStoredNameIgnoringCase]，大小写不敏感，与这里口径不同。
 */
fun Host.agentKindOrDefault(): AgentKind = AgentProfile.ofStoredName(agentKind).kind
