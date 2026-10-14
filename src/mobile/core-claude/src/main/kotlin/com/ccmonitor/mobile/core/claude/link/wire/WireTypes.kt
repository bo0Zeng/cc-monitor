package com.ccmonitor.mobile.core.claude.link.wire

import com.squareup.moshi.Json
import com.squareup.moshi.JsonClass

/*
 * 手机读的那几份回包：每个类只写手机读的那几格（出口声明），形状的本体是 `cmd-wire` 里那一份 Rust 类型。
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
    val recent: List<RecentDir>,
    val agents: List<AgentFacts>,
)

@WireOf("AgentFacts")
@JsonClass(generateAdapter = true)
data class AgentFacts(
    val kind: String,
    val text: String,
    val models: List<NewSessionPick>,
    val permissionModes: List<NewSessionPick>,
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
    val model: String?,
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

@JsonClass(generateAdapter = false)
enum class Tone {
    @Json(name = "plain")
    PLAIN,

    @Json(name = "fail")
    FAIL,

    @Json(name = "now")
    NOW,

    @Json(name = "need")
    NEED,

    @Json(name = "warn")
    WARN,

    @Json(name = "busy")
    BUSY,
}

@WireOf("NeedsList")
@JsonClass(generateAdapter = true)
data class NeedsList(
    val waiting: List<NeedsRow>,
)

@WireOf("NeedsRow")
@JsonClass(generateAdapter = true)
data class NeedsRow(
    val sid: String,
    val needs: Needs,
)

@WireOf("Needs")
@JsonClass(generateAdapter = true)
data class Needs(
    val kind: String,
    val text: String,
    val tone: Tone,
    val rank: Long,
    val waitedMs: Long?,
)

@WireOf("HistoryList")
@JsonClass(generateAdapter = true)
data class HistoryListPage(
    val rows: List<HistoryRow>,
    val notice: String?,
    val truncated: Boolean,
)

@WireOf("HistoryRow")
@JsonClass(generateAdapter = true)
data class HistoryRow(
    val sessionId: String,
    val label: String,
    val atText: String,
    val sectionText: String,
    val projectPath: String,
    val jsonlPath: String,
    val agent: String,
    val agentTag: String?,
    val status: String,
)

/*
 * 读记录那两条（`history-read` · `history-lines`）连同通用记录本身（`cmd-wire::record`）。
 * 通用记录有标签联合（`t` · `speaker.kind` · 块的 `type`）：Moshi 不直接解标签联合 ⇒ 声明成「标签 ＋ 各支的格全可空」的平铺类，
 * 判据照 TS 各支的并集核（只在一部分支里有的格这里必是可空）。流上 `line` 帧的 `record` 也装进同一个 [LineRecord]。
 */

@WireOf("HistoryRead")
@JsonClass(generateAdapter = true)
data class HistoryReadPage(
    val rows: List<ReadRow>,
    val next: Long,
    val eof: Boolean,
)

@WireOf("ReadRow")
@JsonClass(generateAdapter = true)
data class ReadRow(
    val end: Long?,
    val record: LineRecord?,
)

@WireOf("HistoryLines")
@JsonClass(generateAdapter = true)
data class HistoryLinesPage(
    val next: Long,
    val eof: Boolean,
    val lines: List<LineRow>,
)

@WireOf("RecordLine")
@JsonClass(generateAdapter = true)
data class LineRow(
    val seq: Long,
    val record: LineRecord,
)

@WireOf("LineRecord")
@JsonClass(generateAdapter = true)
data class LineRecord(
    val agent: String,
    val id: String,
    val timeText: String?,
    val t: RecordKind,
    val who: SaidBy?,
    val blocks: List<RecordBlock>?,
    val results: Map<String, StepResult>?,
    val autoReply: Boolean?,
    val steps: Map<String, ToolStep>?,
    val text: String?,
    val excerpt: String?,
)

@JsonClass(generateAdapter = false)
enum class RecordKind {
    @Json(name = "said")
    SAID,

    @Json(name = "reply")
    REPLY,

    @Json(name = "retry")
    RETRY,

    @Json(name = "title")
    TITLE,

    @Json(name = "queued")
    QUEUED,

    @Json(name = "unread")
    UNREAD,
}

@WireOf("UserText")
@JsonClass(generateAdapter = true)
data class SaidBy(
    val speaker: Speaker,
    val text: String,
)

@WireOf("Speaker")
@JsonClass(generateAdapter = true)
data class Speaker(
    val kind: SpeakerKind,
    val name: String?,
    val args: String?,
    val command: String?,
    val stdout: String?,
    val stderr: String?,
)

@JsonClass(generateAdapter = false)
enum class SpeakerKind {
    @Json(name = "human")
    HUMAN,

    @Json(name = "slashCommand")
    SLASH_COMMAND,

    @Json(name = "bashInput")
    BASH_INPUT,

    @Json(name = "bashOutput")
    BASH_OUTPUT,

    @Json(name = "commandOutput")
    COMMAND_OUTPUT,

    @Json(name = "taskNotification")
    TASK_NOTIFICATION,

    @Json(name = "agentMessage")
    AGENT_MESSAGE,

    @Json(name = "peerSession")
    PEER_SESSION,

    @Json(name = "coordinator")
    COORDINATOR,

    @Json(name = "agentTask")
    AGENT_TASK,

    @Json(name = "system")
    SYSTEM,

    @Json(name = "compactSummary")
    COMPACT_SUMMARY,

    @Json(name = "interrupt")
    INTERRUPT,

    @Json(name = "toolResult")
    TOOL_RESULT,
}

@WireOf("Block")
@JsonClass(generateAdapter = true)
data class RecordBlock(
    val type: BlockType,
    val text: String?,
    val id: String?,
    val name: String?,
    val input: Any?,
    @Json(name = "for") val of: String?,
    val content: List<RecordBlock>?,
    val isError: Boolean?,
)

@JsonClass(generateAdapter = false)
enum class BlockType {
    @Json(name = "text")
    TEXT,

    @Json(name = "thinking")
    THINKING,

    @Json(name = "tool_use")
    TOOL_USE,

    @Json(name = "tool_result")
    TOOL_RESULT,

    @Json(name = "image")
    IMAGE,
}

@WireOf("StepResult")
@JsonClass(generateAdapter = true)
data class StepResult(
    val text: String,
    val file: String?,
    val patch: List<Hunk>?,
)

@WireOf("PatchHunk")
@JsonClass(generateAdapter = true)
data class Hunk(
    val oldStart: Int,
    val oldLines: Int,
    val newStart: Int,
    val newLines: Int,
    val lines: List<String>,
)

@WireOf("ToolStep")
@JsonClass(generateAdapter = true)
data class ToolStep(
    val arg: String?,
    val text: String?,
)

// ── 会话中途：切权限档 · 这条会话能用的命令与 Skill（聊天屏输入那一圈读的） ──────────

@WireOf("ModeSwitched")
@JsonClass(generateAdapter = true)
data class ModeSwitched(
    val result: ModeSwitchResult,
    val modeText: String?,
    val said: String?,
)

@WireOf("SessionCommands")
@JsonClass(generateAdapter = true)
data class SessionCommands(
    val commands: List<SessionCommand>,
    val skills: List<SessionSkill>,
    val problems: List<String>,
)

@WireOf("SessionCommand")
@JsonClass(generateAdapter = true)
data class SessionCommand(
    val name: String,
    val text: String?,
    val scope: CommandScope,
    val scopeText: String,
    val confirm: Boolean,
)

@JsonClass(generateAdapter = false)
enum class CommandScope {
    @Json(name = "builtin")
    BUILTIN,

    @Json(name = "project")
    PROJECT,

    @Json(name = "user")
    USER,
}

@WireOf("SessionSkill")
@JsonClass(generateAdapter = true)
data class SessionSkill(
    val name: String,
    val text: String?,
    val say: String,
)

// ── 新建会话那几屏（mobui2）：最近的目录 · 目录在不在 · 只列文件夹 · 轮换策略目录 · 在 app 里登录 ──

@WireOf("RecentDir")
@JsonClass(generateAdapter = true)
data class RecentDir(
    val cwd: String,
    val lastText: String,
)

@WireOf("DirChecked")
@JsonClass(generateAdapter = true)
data class DirChecked(
    val exists: Boolean,
)

@WireOf("FilesPathReply")
@JsonClass(generateAdapter = true)
data class FilesPathReply(
    val path: Any,
)

@WireOf("RotationStrategiesReply")
@JsonClass(generateAdapter = true)
data class RotationStrategiesReply(
    val strategies: List<RotStrategyItem>,
    val forNew: String,
)

@WireOf("RotStrategyItem")
@JsonClass(generateAdapter = true)
data class RotStrategyItem(
    val id: String,
    val description: String,
    val rev: String,
    val error: RotStrategyError?,
    val inUse: List<String>,
    val forNew: Boolean,
)

@WireOf("RotStrategyError")
@JsonClass(generateAdapter = true)
data class RotStrategyError(
    val line: Long?,
    val msg: String,
)

@WireOf("LoginStarted")
@JsonClass(generateAdapter = true)
data class LoginStarted(
    val job: String,
    val url: String,
    val paste: PasteKind,
    val auto: Boolean,
)

@JsonClass(generateAdapter = false)
enum class PasteKind {
    @Json(name = "code")
    CODE,

    @Json(name = "redirectUrl")
    REDIRECT_URL,
}

@JsonClass(generateAdapter = false)
enum class FileKind {
    @Json(name = "dir")
    DIR,

    @Json(name = "file")
    FILE,

    @Json(name = "symlink")
    SYMLINK,

    @Json(name = "other")
    OTHER,
}

@WireOf("LsEntry")
@JsonClass(generateAdapter = true)
data class LsEntry(
    val path: Any,
    val kind: FileKind,
    val size: Long?,
    @Json(name = "mtime_text") val mtimeText: String?,
    @Json(name = "link_dir") val linkDir: Boolean?,
)

@WireOf("LsReply")
@JsonClass(generateAdapter = true)
data class LsReply(
    val entries: List<LsEntry>,
    val truncated: Boolean,
    val total: Long,
)

@WireOf("StatReply")
@JsonClass(generateAdapter = true)
data class StatReply(
    val mode: Long?,
)

@WireOf("ReadTextReply")
@JsonClass(generateAdapter = true)
data class ReadTextReply(
    val text: String,
    val sha256: String,
)

@WireOf("HexBytes")
@JsonClass(generateAdapter = true)
data class HexBytes(
    val b16: String,
)

@WireOf("ReadChunkReply")
@JsonClass(generateAdapter = true)
data class ReadChunkReply(
    val size: Long,
    val eof: Boolean,
    val content: HexBytes,
)

@WireOf("FilesDeleteReply")
@JsonClass(generateAdapter = true)
data class FilesDeleteReply(
    val remaining: Long,
)

// ── 轮换屏（mobrot）：`rotation-view` 那一份 ＋ 写那几条的回包。策略写的字（`StrategyText` ＝ 一句或分语言）声明成 `Any`，取字在 `Rotation.text`。 ──

@WireOf("RotationViewReply")
@JsonClass(generateAdapter = true)
data class RotationViewReply(
    val now: Long,
    val nowText: String,
    val strategy: RotStrategyItem?,
    val form: List<RotFormCell>,
    val settings: Any?,
    val dirty: Boolean,
    val start: String,
    val current: String,
    val accounts: List<Fact>,
    val listed: Int,
    val decision: RotTryDecision?,
    val timeline: RotTryTimeline?,
    val past: RotPast?,
    val baseRev: String,
    val hints: List<RotNote>,
)

@WireOf("RotFormCell")
@JsonClass(generateAdapter = true)
data class RotFormCell(
    val key: String,
    val kind: RotFormKind,
    val scope: RotFormScope,
    val inherit: Boolean?,
    val label: Any,
    val help: Any?,
    val windows: List<String>?,
    val min: Double?,
    val max: Double?,
    val unit: String?,
    val default: Any?,
    val choices: List<RotFormChoice>?,
    val nullable: Any?,
)

@JsonClass(generateAdapter = false)
enum class RotFormKind {
    @Json(name = "percent")
    PERCENT,

    @Json(name = "minutes")
    MINUTES,

    @Json(name = "toggle")
    TOGGLE,

    @Json(name = "choice")
    CHOICE,

    @Json(name = "daySegments")
    DAY_SEGMENTS,
}

@JsonClass(generateAdapter = false)
enum class RotFormScope {
    @Json(name = "all")
    ALL,

    @Json(name = "each")
    EACH,
}

@WireOf("RotFormChoice")
@JsonClass(generateAdapter = true)
data class RotFormChoice(
    val v: String,
    val label: Any,
)

@WireOf("Fact")
@JsonClass(generateAdapter = true)
data class Fact(
    val account: String,
    val w5h: WinFact,
    val w7d: WinFact,
    val blocked: FactBlocked?,
)

@WireOf("WinFact")
@JsonClass(generateAdapter = true)
data class WinFact(
    val used: Double,
    val resetsText: String?,
)

@WireOf("FactBlocked")
@JsonClass(generateAdapter = true)
data class FactBlocked(
    val text: String,
)

@WireOf("RotTryDecision")
@JsonClass(generateAdapter = true)
data class RotTryDecision(
    val temp: List<String>,
)

@WireOf("RotTryTimeline")
@JsonClass(generateAdapter = true)
data class RotTryTimeline(
    val switches: List<RotTrySwitch>,
    val uses: List<RotTryUse>,
    val lanes: List<RotTryLane>,
    val ticks: List<RotTryTick>,
)

@WireOf("RotTrySwitch")
@JsonClass(generateAdapter = true)
data class RotTrySwitch(
    val at: Long,
    val atText: String,
    val account: String,
)

@WireOf("RotTryUse")
@JsonClass(generateAdapter = true)
data class RotTryUse(
    val from: Long,
    val to: Long,
    val account: String?,
    val fromTemp: Boolean,
    val capUnknown: Boolean,
    val skipped: List<String>,
)

@WireOf("RotTryLane")
@JsonClass(generateAdapter = true)
data class RotTryLane(
    val account: String,
    val segs: List<RotTryLaneSeg>,
)

@WireOf("RotTryLaneSeg")
@JsonClass(generateAdapter = true)
data class RotTryLaneSeg(
    val from: Long,
    val to: Long,
    val sendable: Boolean,
    val reset: Boolean,
    val fromText: String,
    val left: Double,
)

@WireOf("RotTryTick")
@JsonClass(generateAdapter = true)
data class RotTryTick(
    val at: Long,
    val text: String,
)

@WireOf("RotPast")
@JsonClass(generateAdapter = true)
data class RotPast(
    val from: Long,
    val uses: List<RotPastUse>,
    val lanes: List<RotPastLane>,
)

@WireOf("RotPastUse")
@JsonClass(generateAdapter = true)
data class RotPastUse(
    val from: Long,
    val to: Long,
    val account: String?,
)

@WireOf("RotPastLane")
@JsonClass(generateAdapter = true)
data class RotPastLane(
    val account: String,
    val segs: List<RotPastSeg>,
)

@WireOf("RotPastSeg")
@JsonClass(generateAdapter = true)
data class RotPastSeg(
    val from: Long,
    val to: Long,
    val sendable: Boolean,
    val reset: Boolean,
    val fromText: String,
)

@WireOf("RotNote")
@JsonClass(generateAdapter = true)
data class RotNote(
    val hint: RotHint,
    val text: String,
    val detail: String?,
)

/** 提示是标签联合（`k`）：平铺成「标签 ＋ 各支的格全可空」（同 [LineRecord]）。 */
@WireOf("RotHint")
@JsonClass(generateAdapter = true)
data class RotHint(
    val k: RotHintKind,
    val until: Long?,
    val error: RotStrategyError?,
    val path: String?,
    val zone: String?,
    val files: List<RotOldFile>?,
)

@JsonClass(generateAdapter = false)
enum class RotHintKind {
    @Json(name = "hold")
    HOLD,

    @Json(name = "strategyError")
    STRATEGY_ERROR,

    @Json(name = "tzUnknown")
    TZ_UNKNOWN,

    @Json(name = "oldScripts")
    OLD_SCRIPTS,

    @Json(name = "newer")
    NEWER,

    @Json(name = "wordsBad")
    WORDS_BAD,

    @Json(name = "bookUnreadable")
    BOOK_UNREADABLE,
}

@WireOf("RotOldFile")
@JsonClass(generateAdapter = true)
data class RotOldFile(
    val name: String,
    val path: String,
)

@WireOf("RotSeg")
@JsonClass(generateAdapter = true)
data class RotSeg(
    val from: Long,
    val to: Long,
    val v: Double,
)

@WireOf("RotationDayPaintReply")
@JsonClass(generateAdapter = true)
data class RotationDayPaintReply(
    val segs: List<RotSeg>,
)

@WireOf("RotationSaveReply")
@JsonClass(generateAdapter = true)
data class RotationSaveReply(
    val id: String,
    val rev: String,
)
