package com.ccmonitor.mobile.core.claude.link.wire

/*
 * 手机读的那几份回包：每个类只写手机读的那几格（出口声明），形状的本体是 `cmd-wire::session` 那一份 Rust 类型。
 * 嵌的 Rust 核心照那一份解过才交来，[Wire.bind] 只是装进这里；判据 `WireTypesTest` 对着导出的 TS 逐格核
 * （格在不在 · 型对不对 · 这里不可空的那边必有且不为 null）。
 * 格式守死（判据按行读）：一个类一个 `@WireOf`，主构造一格一行 `[@Key("线上键")] val 名: 型,`。
 */

@WireOf("Sent")
data class Sent(
    val result: SentResult,
    val why: String?,
    val said: String?,
)

enum class SentResult { DELIVERED, UNSURE, REFUSED }

@WireOf("FirstSay")
data class FirstSay(
    val result: String,
    val why: String?,
    val said: String?,
)

@WireOf("SessionNew")
data class SessionNewReply(
    val outcome: SessionNewOutcome,
    val session: String?,
    val sid: String?,
    val cwd: String,
    val prompt: FirstSay?,
)

enum class SessionNewOutcome { STARTED, OPEN }

@WireOf("Rendered")
data class Rendered(
    val cmd: String,
)

@WireOf("NewFacts")
data class NewFacts(
    val agents: List<AgentFacts>,
)

@WireOf("AgentFacts")
data class AgentFacts(
    val kind: String,
    val text: String,
    val models: List<NewSessionPick>,
)

@WireOf("NewSessionPick")
data class NewSessionPick(
    val value: String,
    val text: String,
)

@WireOf("ModelSwitched")
data class ModelSwitched(
    val result: ModeSwitchResult,
    val said: String?,
)

enum class ModeSwitchResult { DONE, UNSURE, REFUSED }

@WireOf("TerminalsList")
data class TerminalsListReply(
    val terminals: List<TerminalItem>,
)

@WireOf("TerminalItem")
data class TerminalItem(
    val terminal: String,
    @Key("tmux_name") val name: String,
    val title: String,
    val session: TermSession?,
    val clients: List<Attached>,
)

@WireOf("TermSession")
data class TermSession(
    val sid: String,
    val agent: String?,
)

@WireOf("Attached")
data class Attached(
    val since: Long,
)

@WireOf("Preview")
data class Preview(
    val lines: List<ScreenLine>,
)

@WireOf("ScreenLine")
data class ScreenLine(
    val text: String,
)

@WireOf("Killed")
data class Killed(
    val killed: Boolean,
)

@WireOf("TailPlan")
data class TailPlan(
    val end: Long,
    @Key("split_at") val splitAt: Long,
    @Key("tail_from") val tailFrom: Long,
    val total: Long,
)
