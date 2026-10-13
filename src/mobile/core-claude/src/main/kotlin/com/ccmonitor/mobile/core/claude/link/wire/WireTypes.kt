package com.ccmonitor.mobile.core.claude.link.wire

import com.squareup.moshi.Json
import com.squareup.moshi.JsonClass

/*
 * 手机读的那几份回包：每个类只写手机读的那几格（出口声明），形状的本体是 `cmd-wire::session` 那一份 Rust 类型。
 * 嵌的 Rust 核心照那一份解过才交来，[Wire.bind] 经 Moshi 在编译期生成的解码器装进这里；判据 `WireTypesTest` 对着导出的 TS 逐格核
 * （格在不在 · 型对不对 · 这里不可空的那边必有且不为 null · 每个类都有生成的解码器）。
 * 格式守死（判据按行读）：一个类 `@WireOf` ＋ `@JsonClass(generateAdapter = true)`，主构造一格一行 `[@Json(name = "线上键")] val 名: 型,`；
 * 枚举 `@JsonClass(generateAdapter = false)`，每个常量都写 `@Json(name = "线上字")`（Moshi 自带的混淆规则保的就是这样的枚举）。
 */

@WireOf("Sent")
@JsonClass(generateAdapter = true)
data class Sent(
    val result: SentResult,
    val why: String?,
    val said: String?,
)

@JsonClass(generateAdapter = false)
enum class SentResult {
    @Json(name = "delivered")
    DELIVERED,

    @Json(name = "unsure")
    UNSURE,

    @Json(name = "refused")
    REFUSED,
}

@WireOf("FirstSay")
@JsonClass(generateAdapter = true)
data class FirstSay(
    val result: String,
    val why: String?,
    val said: String?,
)

@WireOf("SessionNew")
@JsonClass(generateAdapter = true)
data class SessionNewReply(
    val outcome: SessionNewOutcome,
    val session: String?,
    val sid: String?,
    val cwd: String,
    val prompt: FirstSay?,
)

@JsonClass(generateAdapter = false)
enum class SessionNewOutcome {
    @Json(name = "started")
    STARTED,

    @Json(name = "open")
    OPEN,
}

@WireOf("Rendered")
@JsonClass(generateAdapter = true)
data class Rendered(
    val cmd: String,
)

@WireOf("NewFacts")
@JsonClass(generateAdapter = true)
data class NewFacts(
    val agents: List<AgentFacts>,
)

@WireOf("AgentFacts")
@JsonClass(generateAdapter = true)
data class AgentFacts(
    val kind: String,
    val text: String,
    val models: List<NewSessionPick>,
)

@WireOf("NewSessionPick")
@JsonClass(generateAdapter = true)
data class NewSessionPick(
    val value: String,
    val text: String,
)

@WireOf("ModelSwitched")
@JsonClass(generateAdapter = true)
data class ModelSwitched(
    val result: ModeSwitchResult,
    val said: String?,
)

@JsonClass(generateAdapter = false)
enum class ModeSwitchResult {
    @Json(name = "done")
    DONE,

    @Json(name = "unsure")
    UNSURE,

    @Json(name = "refused")
    REFUSED,
}

@WireOf("TerminalsList")
@JsonClass(generateAdapter = true)
data class TerminalsListReply(
    val terminals: List<TerminalItem>,
)

@WireOf("TerminalItem")
@JsonClass(generateAdapter = true)
data class TerminalItem(
    val terminal: String,
    @Json(name = "tmux_name") val name: String,
    val title: String,
    val session: TermSession?,
    val clients: List<Attached>,
)

@WireOf("TermSession")
@JsonClass(generateAdapter = true)
data class TermSession(
    val sid: String,
    val agent: String?,
)

@WireOf("Attached")
@JsonClass(generateAdapter = true)
data class Attached(
    val since: Long,
)

@WireOf("Preview")
@JsonClass(generateAdapter = true)
data class Preview(
    val lines: List<ScreenLine>,
)

@WireOf("ScreenLine")
@JsonClass(generateAdapter = true)
data class ScreenLine(
    val text: String,
)

@WireOf("Killed")
@JsonClass(generateAdapter = true)
data class Killed(
    val killed: Boolean,
)

@WireOf("TailPlan")
@JsonClass(generateAdapter = true)
data class TailPlan(
    val end: Long,
    @Json(name = "split_at") val splitAt: Long,
    @Json(name = "tail_from") val tailFrom: Long,
    val total: Long,
)
