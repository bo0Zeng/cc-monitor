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

/** `session-open` 的成品：落在哪 · 哪个 tmux 会话 · 要在终端里敲的那一行（那台渲好的）· 第一句的结局。 */
@WireOf("Opened")
@JsonClass(generateAdapter = true)
data class Opened(
    val landed: Landed,
    val session: String?,
    val line: String?,
    val sid: String?,
    val cwd: String?,
    val note: String?,
    val prompt: FirstSay?,
    val arrival: ArriveAsk?,
)

/** 等报到那一问（`session-open` 回包里的 `arrival`，原样交回 `session-arrive`）。 */
@WireOf("ArriveAsk")
@JsonClass(generateAdapter = true)
data class ArriveAsk(
    val by: ArriveBy,
    val since: Long,
    val session: String?,
)

/** 认报到：`kind` 是 `sid`（带 [sid]）或 `cwd`（带 [cwd]）。 */
@WireOf("ArriveBy")
@JsonClass(generateAdapter = true)
data class ArriveBy(
    val kind: String,
    val sid: String?,
    val cwd: String?,
)

/** `session-arrive` 的回话：`arrived`（带 [sid]）· `missed`（带 [said]：那台写好的那一句 · 那个终端还在 ⇒ [terminal]）。 */
@WireOf("ArriveEnd")
@JsonClass(generateAdapter = true)
data class ArriveEnd(
    val kind: String,
    val sid: String?,
    val said: String?,
    val terminal: String?,
)

@JsonClass(generateAdapter = false)
enum class Landed {
    @Json(name = "started")
    STARTED,

    @Json(name = "in_place")
    IN_PLACE,

    @Json(name = "running")
    RUNNING,

    @Json(name = "idle")
    IDLE,

    @Json(name = "window")
    WINDOW,
}

/** `authorized-keys-add`：那一行公钥并进去了，还是本来就在。 */
@WireOf("KeyAdded")
@JsonClass(generateAdapter = true)
data class KeyAdded(
    val outcome: PushOutcome,
)

@JsonClass(generateAdapter = false)
enum class PushOutcome {
    @Json(name = "added")
    ADDED,

    @Json(name = "already")
    ALREADY,
}

@WireOf("OpenFacts")
@JsonClass(generateAdapter = true)
data class OpenFacts(
    val recent: List<RecentDir>,
    val tmux: Boolean,
    val agents: List<AgentFacts>,
)

@WireOf("AgentFacts")
@JsonClass(generateAdapter = true)
data class AgentFacts(
    val kind: String,
    val text: String,
    val models: List<AgentPick>,
    val modelDefault: AgentPick?,
    val modelNote: String?,
    val permissionModes: List<AgentPick>,
)

@WireOf("AgentPick")
@JsonClass(generateAdapter = true)
data class AgentPick(
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

@WireOf("HistoryTail")
@JsonClass(generateAdapter = true)
data class HistoryTailReply(
    val end: Long,
    @Json(name = "split_at") val splitAt: Long,
    val total: Long,
    val how: TailHow,
    val held: Held,
)

@JsonClass(generateAdapter = false)
enum class TailHow {
    @Json(name = "tail")
    TAIL,

    @Json(name = "resume")
    RESUME,

    @Json(name = "handoff")
    HANDOFF,

    @Json(name = "truncated")
    TRUNCATED,

    @Json(name = "rewritten")
    REWRITTEN,
}

@WireOf("Held")
@JsonClass(generateAdapter = true)
data class Held(
    val path: String,
    val next: Long,
    @Json(name = "next_byte") val nextByte: Long,
    val mark: String,
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
    val items: List<NeedsItem>,
    val answered: List<AnsweredRow>,
    val manualUnreadable: String?,
)

/** 需手动的一项（按 `kind` 三臂平铺：会话那一臂只有 [session]，计划那一臂只有 [workspace] · [slice] · [k]，会话外那一臂只有 [manual]）。 */
@WireOf("NeedsItem")
@JsonClass(generateAdapter = true)
data class NeedsItem(
    val kind: String,
    val rank: Long,
    val session: NeedsRow?,
    val workspace: String?,
    val slice: String?,
    val k: Long?,
    val manual: ManualItem?,
)

/** 会话外的一项（`accountLogin` · `mcpLogin` · `secretFile` 三臂平铺；`fix` 为空 ⇒ [why] 是第二行那一句）。 */
@WireOf("ManualItem")
@JsonClass(generateAdapter = true)
data class ManualItem(
    val kind: String,
    val account: String?,
    val server: String?,
    val said: String,
    val fix: ManualFix?,
    val why: String?,
)

@WireOf("ManualFix")
@JsonClass(generateAdapter = true)
data class ManualFix(
    val command: String,
    val args: LoginStartReq,
)

/** `login-start` 的目标（`account` · `mcp` 两臂平铺；号那一臂 [account] 必有，MCP 那一臂 [account] 缺 ＝ 没设账号那一份）。 */
@WireOf("LoginStartReq")
@JsonClass(generateAdapter = true)
data class LoginStartReq(
    val kind: LoginTarget,
    val agent: String,
    val account: String?,
    val server: String?,
    val projectDir: String?,
)

@WireOf("AnsweredRow")
@JsonClass(generateAdapter = true)
data class AnsweredRow(
    val sid: String,
    val said: String,
)

@WireOf("NeedsRow")
@JsonClass(generateAdapter = true)
data class NeedsRow(
    val sid: String,
    val label: String,
    val face: SessionFace,
    val needs: Needs,
)

@WireOf("Needs")
@JsonClass(generateAdapter = true)
data class Needs(
    val kind: String,
    val call: String?,
    val text: String,
    val head: String,
    val headCode: String?,
    val tone: Tone,
    val rank: Long,
    val waitedMs: Long?,
    val ask: NeedsAsk?,
    val manualText: String?,
)

/*
 * 答题口：一条挂着的待答请求（`needs.ask`）与答它那一条（`needs-answer`）。选项的字 · 次序 · 语气 · 要不要附话 · 题目全是后端写好的，
 * 手机只排版；入参也是这里的类型（[Wire.args] 出成线上那一份），与桌面 · 后端同一形。
 */

@WireOf("NeedsAsk")
@JsonClass(generateAdapter = true)
data class NeedsAsk(
    val id: String,
    val kind: AskKind,
    val tool: String,
    val run: NeedsAskRun?,
    val what: String?,
    val detail: String,
    val choices: List<NeedsChoice>,
    val questions: List<NeedsQuestion>,
)

@JsonClass(generateAdapter = false)
enum class AskKind {
    @Json(name = "approve")
    APPROVE,

    @Json(name = "answer")
    QUESTION,

    @Json(name = "plan")
    PLAN,
}

@WireOf("NeedsAskRun")
@JsonClass(generateAdapter = true)
data class NeedsAskRun(
    val id: String,
    val kind: String?,
)

@WireOf("NeedsChoice")
@JsonClass(generateAdapter = true)
data class NeedsChoice(
    val id: String,
    val text: String,
    val tone: Tone,
    val note: NeedsNote,
    val row: NeedsChoiceRow,
)

@JsonClass(generateAdapter = false)
enum class NeedsChoiceRow {
    @Json(name = "main")
    MAIN,

    @Json(name = "hold")
    HOLD,

    @Json(name = "release")
    RELEASE,
}

@JsonClass(generateAdapter = false)
enum class NeedsNote {
    @Json(name = "none")
    NONE,

    @Json(name = "optional")
    OPTIONAL,

    @Json(name = "required")
    REQUIRED,
}

@WireOf("NeedsQuestion")
@JsonClass(generateAdapter = true)
data class NeedsQuestion(
    val id: String,
    val text: String,
    val header: String?,
    val multi: Boolean,
    val options: List<NeedsQuestionOption>,
)

@WireOf("NeedsQuestionOption")
@JsonClass(generateAdapter = true)
data class NeedsQuestionOption(
    val id: String,
    val text: String,
    val detail: String?,
)

@WireOf("AnswerReq")
@JsonClass(generateAdapter = true)
data class AnswerReq(
    val sid: String,
    val ask: String,
    val choice: String,
    val note: String?,
    val picks: Map<String, AskPick>?,
)

@WireOf("AskPick")
@JsonClass(generateAdapter = true)
data class AskPick(
    val options: List<String>?,
    val text: String?,
)

@WireOf("Answered")
@JsonClass(generateAdapter = true)
data class Answered(
    val result: AnswerResult,
)

@JsonClass(generateAdapter = false)
enum class AnswerResult {
    @Json(name = "answered")
    ANSWERED,
}

/** `history-list` 回包的头里业务那一格（[SyncHead.info]）：按目录看时的组 · 注解没并上那一句。 */
@WireOf("HistoryHead")
@JsonClass(generateAdapter = true)
data class HistoryHead(
    val groups: List<HistoryGroup>,
    val notice: String?,
)

/** 按目录看时的一组：几台的组按 `order` 并成一列；时刻照那台写好的 `atText`（读不了的那一组 ⇒ `null`）。 */
@WireOf("HistoryGroup")
@JsonClass(generateAdapter = true)
data class HistoryGroup(
    val key: String,
    val projectName: String,
    val projectPath: String,
    val count: Long,
    val hasLive: Boolean?,
    val order: Long,
    val atText: String?,
    val failed: String?,
)

/** 清单一行：[at] 是并几台时的排序键；[group] 是按目录那一形里属于哪一组（`groups[].key`）。 */
@WireOf("HistoryRow")
@JsonClass(generateAdapter = true)
data class HistoryRow(
    val sessionId: String,
    val label: String,
    val at: Long,
    val atText: String,
    val sectionText: String,
    val starred: Boolean,
    val projectName: String,
    val projectPath: String,
    val group: String,
    val jsonlPath: String,
    val agent: String,
    val agentTag: String?,
    val face: SessionFace,
    val hidden: Hidden?,
    val forkedFromSessionId: String?,
    val forkedFrom: ForkedFrom?,
    val can: Can,
)

/** 这一行能做什么（那台判的）；手机只读删除那一格。 */
@WireOf("Can")
@JsonClass(generateAdapter = true)
data class Can(
    val delete: CanDelete,
)

/** 能不能删：`live` 正在跑（灰着）；`unsure` 说不准在不在跑（确认框多一句）。 */
@JsonClass(generateAdapter = false)
enum class CanDelete {
    @Json(name = "yes")
    YES,

    @Json(name = "live")
    LIVE,

    @Json(name = "unsure")
    UNSURE,
}

/** 分叉来的那一行的父会话（显示标题）；父会话不在那台的清单里 ⇒ 行上没有这一格，写「分叉自其他会话」。 */
@WireOf("ForkedFrom")
@JsonClass(generateAdapter = true)
data class ForkedFrom(
    val label: String,
)

/** 几台搜内容合好的一份（手机核心合的：`cmd_wire::search::merge`，照各台排好的先后并成一列）。 */
@WireOf("Merged")
@JsonClass(generateAdapter = true)
data class Merged(
    val totalHits: Long,
    val sessionCount: Long,
    val truncated: Boolean,
    val sessions: List<MergedRow>,
    val unreadable: Long,
    val skipped: List<String>,
    val down: List<SearchDown>,
)

/** 核心解不开的那一台：交合并时给它起的名字 · serde 那一句。 */
@WireOf("SearchDown")
@JsonClass(generateAdapter = true)
data class SearchDown(
    val origin: String,
    val said: String,
)

/** 合好的一行：[origin] 是交合并时给那台起的名字（手机这一侧每台都交 hostId ⇒ 恒有）。 */
@WireOf("MergedRow")
@JsonClass(generateAdapter = true)
data class MergedRow(
    val origin: String?,
    val atText: String,
    val sessionId: String,
    val title: String,
    val hits: List<SearchHit>,
    val moreHits: Long,
)

/** 一条命中：`offset` 交给 `history-read` 就从那一条读起。 */
@WireOf("SearchHit")
@JsonClass(generateAdapter = true)
data class SearchHit(
    val uuid: String,
    val before: String,
    val matched: String,
    val after: String,
    val offset: Long,
)

/** 隐藏那一格（判定只住后端）：手机只读后端写好的那一句（长按菜单里说为什么隐藏）。不隐藏 ⇒ 行上是 `null`。 */
@WireOf("Hidden")
@JsonClass(generateAdapter = true)
data class Hidden(
    val said: String,
)

/** `history-annotate` 的回包：手机只读改之前单个隐藏那一格（撤销照它交回去，`null` ＝ 照规则）。 */
@WireOf("Annotated")
@JsonClass(generateAdapter = true)
data class Annotated(
    val prev: Hide?,
)

/** 单个隐藏那一格（历史注解 `hide`）。 */
@JsonClass(generateAdapter = false)
enum class Hide {
    @Json(name = "on")
    ON,

    @Json(name = "off")
    OFF,
}

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
    val end: Long,
    val lines: List<LineRow>,
)

@WireOf("RecordLine")
@JsonClass(generateAdapter = true)
data class LineRow(
    val seq: Long,
    val end: Long?,
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
    val cards: Map<String, ToolCard>?,
    val steps: Map<String, ToolStep>?,
    val text: String?,
    val excerpt: String?,
)

/** 一个工具调用画成哪一种卡（核心判的；普通工具卡不列）。 */
@JsonClass(generateAdapter = false)
enum class ToolCard {
    @Json(name = "agent")
    AGENT,

    @Json(name = "interactive")
    INTERACTIVE,

    @Json(name = "diff")
    DIFF,

    @Json(name = "md")
    MD,

    @Json(name = "command")
    COMMAND,
}

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

@WireOf("SessionFacts")
@JsonClass(generateAdapter = true)
data class SessionFacts(
    val modelText: String?,
    val modelPick: String?,
    val permissionModeText: String?,
)

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
    val compact: SessionAct?,
    val clear: SessionAct?,
)

@WireOf("SessionAct")
@JsonClass(generateAdapter = true)
data class SessionAct(
    val say: String,
    val confirm: Boolean,
)

@WireOf("SessionCommand")
@JsonClass(generateAdapter = true)
data class SessionCommand(
    val name: String,
    val text: String?,
    val scope: CommandScope,
    val scopeText: String,
    val needsArgsText: String?,
    val confirmText: String?,
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
    val summary: String,
    val rev: String,
    val error: RotStrategyError?,
    val inUse: List<String>,
    val forNew: Boolean,
    val hand: Boolean,
)

@WireOf("RotStrategyId")
@JsonClass(generateAdapter = true)
data class RotStrategyId(
    val id: String,
)

@WireOf("RotStrategyError")
@JsonClass(generateAdapter = true)
data class RotStrategyError(
    val line: Long?,
    val msg: String,
)

/** 登录什么：一个号 · 号里的一个 MCP 服务器。 */
@JsonClass(generateAdapter = false)
enum class LoginTarget {
    @Json(name = "account")
    ACCOUNT,

    @Json(name = "mcp")
    MCP,
}

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
    val name: Any,
    val kind: FileKind,
    val size: Long?,
    @Json(name = "size_text") val sizeText: String?,
    @Json(name = "mtime_text") val mtimeText: String?,
    @Json(name = "link_dir") val linkDir: Boolean?,
    val gates: Gates?,
)

@WireOf("Gates")
@JsonClass(generateAdapter = true)
data class Gates(
    val preview: PreviewGate,
    val edit: Gate,
)

@WireOf("PreviewGate")
@JsonClass(generateAdapter = true)
data class PreviewGate(
    val text: TextPreview?,
    val no: String?,
)

@WireOf("TextPreview")
@JsonClass(generateAdapter = true)
data class TextPreview(
    val cap: Long,
)

@WireOf("Gate")
@JsonClass(generateAdapter = true)
data class Gate(
    val cap: Long?,
    val no: String?,
)

/** `files-ls` 那一问的头（回包本身是 [SyncPatch]：行在 `up`、先后在 `order`，这一格在 `head.info`）。 */
@WireOf("LsHead")
@JsonClass(generateAdapter = true)
data class LsHead(
    val up: Any?,
    val crumbs: List<Crumb>,
    val truncated: Boolean,
    val total: Long,
)

@WireOf("Crumb")
@JsonClass(generateAdapter = true)
data class Crumb(
    val name: Any,
    val path: Any,
)

@WireOf("StatReply")
@JsonClass(generateAdapter = true)
data class StatReply(
    val up: Any?,
    val kind: FileKind,
    val mode: Long?,
)

@WireOf("ReadTextReply")
@JsonClass(generateAdapter = true)
data class ReadTextReply(
    val text: String,
    val sha256: String,
)

// ── 轮换屏（mobrot）：`rotation-view` 那一份 ＋ 写那几条的回包。策略写的字（`StrategyText` ＝ 一句或分语言）声明成 `Any`，取字在 `Rotation.text`。 ──

@WireOf("RotationViewReply")
@JsonClass(generateAdapter = true)
data class RotationViewReply(
    val now: Long,
    val nowText: String,
    val title: String?,
    val strategy: RotStrategyItem?,
    val board: RotBoard,
    val start: String,
    val current: String,
    val lineup: List<String>,
    val off: List<String>,
    val accounts: List<RotAccount>,
    val listed: Int,
    val decision: RotTryDecision?,
    val timeline: RotTryTimeline?,
    val past: RotPast?,
    val hints: List<RotNote>,
)

@WireOf("RotBoard")
@JsonClass(generateAdapter = true)
data class RotBoard(
    val all: List<RotCell>,
    val rows: List<RotRow>,
)

@WireOf("RotRow")
@JsonClass(generateAdapter = true)
data class RotRow(
    val account: String,
    val line: String,
    val hover: String,
    val cells: List<RotCell>,
    val rules: List<String>,
    val marks: List<RotMark>,
)

@WireOf("RotMark")
@JsonClass(generateAdapter = true)
data class RotMark(
    val glyph: String?,
    val dashed: Boolean,
    val tip: String,
)

@WireOf("RotCell")
@JsonClass(generateAdapter = true)
data class RotCell(
    val key: String,
    val kind: RotFormKind,
    val window: String?,
    val label: String,
    val help: String?,
    val v: Any?,
    val num: Double?,
    val from: RotFrom,
    val nullable: String?,
    val choices: List<RotChoice>?,
    val min: Double?,
    val max: Double?,
    val segs: List<RotDaySeg>?,
    val lead: String?,
    val under: String?,
    val same: String?,
)

@JsonClass(generateAdapter = false)
enum class RotFrom {
    @Json(name = "own")
    OWN,

    @Json(name = "all")
    ALL,

    @Json(name = "default")
    DEFAULT,

    @Json(name = "same")
    SAME,
}

@WireOf("RotChoice")
@JsonClass(generateAdapter = true)
data class RotChoice(
    val v: String,
    val label: String,
)

@WireOf("RotDaySeg")
@JsonClass(generateAdapter = true)
data class RotDaySeg(
    val from: Long,
    val to: Long,
    val v: Double,
    val text: String,
    val tip: String,
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

@WireOf("RotAccount")
@JsonClass(generateAdapter = true)
data class RotAccount(
    val account: String,
    val w5h: WinFact,
    val w7d: WinFact,
    val blocked: FactBlocked?,
    val plate: RotPlate,
)

@WireOf("RotPlate")
@JsonClass(generateAdapter = true)
data class RotPlate(
    val color: Int,
    val label: String,
    val text: String,
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
    val why: List<RotTryRow>,
)

@WireOf("RotTryRow")
@JsonClass(generateAdapter = true)
data class RotTryRow(
    val account: String,
    val why: String,
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
    val text: String,
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
    val text: String,
)

@WireOf("RotNote")
@JsonClass(generateAdapter = true)
data class RotNote(
    val hint: RotHint,
    val tone: Tone,
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

    @Json(name = "older")
    OLDER,

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

@WireOf("ProfilesBook")
@JsonClass(generateAdapter = true)
data class ProfilesBook(
    val fingerprint: String?,
    val profiles: List<ProfileRow>,
)

@WireOf("ProfileRow")
@JsonClass(generateAdapter = true)
data class ProfileRow(
    val name: String,
    val usable: Boolean,
    val start: String,
)

@WireOf("ProfilesWriteDone")
@JsonClass(generateAdapter = true)
data class ProfilesWriteDone(
    val wrote: Boolean,
    val reload: String?,
)

/** `accounts-list`：这台账号库里的号（号牌字 · 5h 那一格 · 选不了的那一句都是那台写好的）。 */
@WireOf("AccountsList")
@JsonClass(generateAdapter = true)
data class AccountsList(
    val meta: ListMeta,
    val accounts: List<AccountRow>,
)

@WireOf("ListMeta")
@JsonClass(generateAdapter = true)
data class ListMeta(
    val effectiveDefault: String?,
    val home: String?,
)

@WireOf("AccountRow")
@JsonClass(generateAdapter = true)
data class AccountRow(
    val name: String,
    val label: String,
    val color: Int,
    val plate: String,
    val quota: String?,
    val unselectable: String?,
    val selectable: Boolean,
    val login: Boolean,
    val configDir: String?,
    val mode: String,
    val authKind: String,
    val badge: Badge,
)

/** 号行右边那一枚（[warn] ⇒ 要人动手的那一种，界面只画这一种）。 */
@WireOf("Badge")
@JsonClass(generateAdapter = true)
data class Badge(
    val text: String,
    val warn: Boolean,
)

/**
 * `quota-read`：这台的额度账。读到的三态 · 读不出 / 一个号都没有时那一句（[text]）与复制详情；出过数的号在 [accounts]、
 * 没出过数的在 [unseen]（各照回包先后）；结论两行 [verdict]；位名表 [names]。字与语气全是那台写好的。
 */
@WireOf("QuotaRead")
@JsonClass(generateAdapter = true)
data class QuotaReadReply(
    val state: QuotaReadState,
    val text: String?,
    val detail: String?,
    val accounts: List<QuotaAccount>,
    val unseen: List<QuotaUnseen>,
    val verdict: QuotaVerdict?,
    val names: QuotaNames,
)

@JsonClass(generateAdapter = false)
enum class QuotaReadState {
    @Json(name = "present")
    PRESENT,

    @Json(name = "absent")
    ABSENT,

    @Json(name = "unreadable")
    UNREADABLE,
}

/** 位名表（`5h` ⇒ 屏上那个字）。 */
@WireOf("QuotaNames")
@JsonClass(generateAdapter = true)
data class QuotaNames(
    val slots: Map<String, String>,
)

/** 「现在还能不能发」：大字 · 小字（说不出 ⇒ `null`）· 语气。 */
@WireOf("QuotaVerdict")
@JsonClass(generateAdapter = true)
data class QuotaVerdict(
    val text: String,
    val sub: String?,
    val tone: UsageTone,
)

/** 出过数的一个号：写好的几行（[rows]，第一行是卡头）· 两条计量条那几格 · 数旧没有 · 用量那一格。 */
@WireOf("QuotaAccount")
@JsonClass(generateAdapter = true)
data class QuotaAccount(
    val account: String,
    val kind: QuotaKind,
    val rows: List<List<QuotaCell>>,
    val slots: List<SlotShow>,
    val stale: Boolean,
    val usage: Usage,
)

/** 一个语义位那一格：条长 · 字 · 语气 · 重置那一格与距今（说不出 ⇒ `null`）· 「1% ≈ 多少」（量不到 ⇒ `null`）。 */
@WireOf("SlotShow")
@JsonClass(generateAdapter = true)
data class SlotShow(
    val slot: String,
    val pct: Int?,
    val text: String,
    val tone: UsageTone,
    val resetText: String?,
    val resetsAtRelText: String?,
    val perPctText: String?,
)

/** 行里的一格。 */
@WireOf("QuotaCell")
@JsonClass(generateAdapter = true)
data class QuotaCell(
    val text: String,
    val tone: Tone,
)

@JsonClass(generateAdapter = false)
enum class QuotaKind {
    @Json(name = "sub")
    SUB,

    @Json(name = "api")
    API,
}

@WireOf("QuotaUnseen")
@JsonClass(generateAdapter = true)
data class QuotaUnseen(
    val account: String,
    val kind: QuotaKind,
    val rows: List<List<QuotaCell>>,
    val usage: QuotaUnseenUsage,
)

/** 用量那一格：连成的一句 ＋ 语气（状态栏按钮 · 切号下拉同一份）。 */
@WireOf("Usage")
@JsonClass(generateAdapter = true)
data class Usage(
    val text: String,
    val tone: UsageTone,
)

@WireOf("QuotaUnseenUsage")
@JsonClass(generateAdapter = true)
data class QuotaUnseenUsage(
    val text: String,
    val tone: UsageTone,
)

@JsonClass(generateAdapter = false)
enum class UsageTone {
    @Json(name = "plain")
    PLAIN,

    @Json(name = "fail")
    FAIL,

    @Json(name = "warn")
    WARN,
}

/** `prefs-read` · `prefs-set`（写完读回）：这台的偏好；读不懂（[state] 为 unreadable）也照答那一份。 */
@WireOf("PrefsReply")
@JsonClass(generateAdapter = true)
data class PrefsReply(
    val state: PrefsState,
    val reason: String?,
    val detail: String?,
    val prefs: Prefs,
)

@JsonClass(generateAdapter = false)
enum class PrefsState {
    @Json(name = "absent")
    ABSENT,

    @Json(name = "ok")
    OK,

    @Json(name = "unreadable")
    UNREADABLE,
}

@WireOf("Prefs")
@JsonClass(generateAdapter = true)
data class Prefs(
    val models: Map<String, String>,
    val childTab: TabPref,
    val bgTab: TabPref,
)

/** 一种会话进不进标签栏（也管手机的「最近」列不列它）。 */
@JsonClass(generateAdapter = false)
enum class TabPref {
    @Json(name = "show")
    SHOW,

    @Json(name = "hide")
    HIDE,
}

/** 登录失败的 `data`：还能怎么办（那台照码定）。 */
@WireOf("LoginAgain")
@JsonClass(generateAdapter = true)
data class LoginAgain(
    val again: LoginAgainHow,
)

@JsonClass(generateAdapter = false)
enum class LoginAgainHow {
    @Json(name = "paste")
    PASTE,

    @Json(name = "restart")
    RESTART,

    @Json(name = "none")
    NONE,
}

// 流上的会话帧（核心按 `cmd-wire` 那一份类型重写过的一行）。

@WireOf("SessionAdded")
@JsonClass(generateAdapter = true)
data class SessionAddedFrame(
    val sid: String,
    val path: String?,
    val cwd: String?,
    val name: String?,
    @Json(name = "agent_kind") val agentKind: String,
    val attachable: Boolean?,
    val face: SessionFace,
)

@WireOf("SessionStatus")
@JsonClass(generateAdapter = true)
data class SessionStatusFrame(
    val sid: String,
    val path: String?,
    val face: SessionFace,
)

@WireOf("SessionState")
@JsonClass(generateAdapter = true)
data class SessionStateFrame(
    val sid: String,
    val face: SessionFace,
)

@WireOf("SessionRemoved")
@JsonClass(generateAdapter = true)
data class SessionRemovedFrame(
    val sid: String,
    val by: String?,
)

@WireOf("SessionFileReread")
@JsonClass(generateAdapter = true)
data class SessionFileRereadFrame(
    @Json(name = "session_id") val sid: String,
)

/** 一条会话此刻的整份脸（那台一处判好）：点 · 短名 · 语气 · 能做什么；那台看不见时 [settled] 为真的照旧作数。 */
@WireOf("SessionFace")
@JsonClass(generateAdapter = true)
data class SessionFace(
    val dot: SessionDot,
    val live: FaceLive,
    val settled: Boolean,
    val name: String,
    val tone: Tone,
    val can: SessionCan,
)

/** 进程在不在：在、且那台判得出在干什么才带 [activity]。 */
@WireOf("FaceLive")
@JsonClass(generateAdapter = true)
data class FaceLive(
    val activity: SessionActivityWord?,
)

/** 会话那颗点（线上名与桌面状态点同一套）。 */
@JsonClass(generateAdapter = false)
enum class SessionDot {
    @Json(name = "running")
    RUNNING,

    @Json(name = "needs-you")
    NEEDS_YOU,

    @Json(name = "background")
    BACKGROUND,

    @Json(name = "idle")
    IDLE,

    @Json(name = "exited")
    EXITED,

    @Json(name = "ended")
    ENDED,

    @Json(name = "gone")
    GONE,

    @Json(name = "unknown")
    UNKNOWN,
}

@JsonClass(generateAdapter = false)
enum class SessionActivityWord {
    @Json(name = "working")
    WORKING,

    @Json(name = "needs_you")
    NEEDS_YOU,

    @Json(name = "idle")
    IDLE,

    @Json(name = "background_work")
    BACKGROUND_WORK,
}

/** 这条会话此刻能做的（手机只读换模型 · 切权限档那两样）。 */
@WireOf("SessionCan")
@JsonClass(generateAdapter = true)
data class SessionCan(
    val model: Allow,
    val permission: Allow,
)

/** 一样此刻能不能做：不能 ⇒ [why] 是那台写好的那一句。 */
@WireOf("Allow")
@JsonClass(generateAdapter = true)
data class Allow(
    val can: CanWord,
    val why: String?,
)

@JsonClass(generateAdapter = false)
enum class CanWord {
    @Json(name = "yes")
    YES,

    @Json(name = "no")
    NO,
}

@WireOf("Line")
@JsonClass(generateAdapter = true)
data class LineFrame(
    @Json(name = "session_id") val sid: String,
    val path: String,
    val seq: Long,
    @Json(name = "byte_offset") val byteOffset: Long,
)

@WireOf("Changed")
@JsonClass(generateAdapter = true)
data class ChangedFrame(
    val topic: String,
    val key: String?,
)

@WireOf("LiveCard")
@JsonClass(generateAdapter = true)
data class LiveCardFrame(
    val sid: String,
    val run: String?,
    val card: Long,
    val n: Long,
    val op: LiveOpWord,
    val rid: String?,
    val phase: LivePhaseWord?,
    val clipped: Boolean?,
    val blocks: List<LiveBlockCell>?,
    val i: Long?,
    val block: String?,
    val tool: String?,
    val s: String?,
)

@WireOf("LiveBlock")
@JsonClass(generateAdapter = true)
data class LiveBlockCell(
    val kind: String,
    val text: String?,
    val tool: String?,
)

@JsonClass(generateAdapter = false)
enum class LiveOpWord {
    @Json(name = "open")
    OPEN,

    @Json(name = "block")
    BLOCK,

    @Json(name = "text")
    TEXT,

    @Json(name = "awaiting")
    AWAITING,

    @Json(name = "close")
    CLOSE,
}

@JsonClass(generateAdapter = false)
enum class LivePhaseWord {
    @Json(name = "streaming")
    STREAMING,

    @Json(name = "awaiting")
    AWAITING,
}

/** 一台的状态成品（判定只住后端 `deploy::machine`，手机嵌的核心跑同一份）：手机只读哪一态 · 问题行那一句 · 复制详情 · 修法。 */
@WireOf("MachineState")
@JsonClass(generateAdapter = true)
data class MachineState(
    val state: MachineStateKind,
    val said: String,
    val detail: String?,
    val fixes: List<MachineFixButton>,
)

@JsonClass(generateAdapter = false)
enum class MachineStateKind {
    @Json(name = "up")
    UP,

    @Json(name = "connecting")
    CONNECTING,

    @Json(name = "installing")
    INSTALLING,

    @Json(name = "updating")
    UPDATING,

    @Json(name = "down")
    DOWN,

    @Json(name = "host_key_changed")
    HOST_KEY_CHANGED,

    @Json(name = "needs_update")
    NEEDS_UPDATE,

    @Json(name = "newer")
    NEWER,

    @Json(name = "disabled")
    DISABLED,

    @Json(name = "unsupported")
    UNSUPPORTED,

    @Json(name = "unknown")
    UNKNOWN,
}

/** 一颗修法按钮：做什么 ＋ 按钮上的字（后端写好）。 */
@WireOf("MachineFixButton")
@JsonClass(generateAdapter = true)
data class MachineFixButton(
    val fix: MachineFix,
    val label: String,
)

@JsonClass(generateAdapter = false)
enum class MachineFix {
    @Json(name = "retry")
    RETRY,

    @Json(name = "conn_settings")
    CONN_SETTINGS,

    @Json(name = "push_key")
    PUSH_KEY,

    @Json(name = "compare_fingerprint")
    COMPARE_FINGERPRINT,

    @Json(name = "update")
    UPDATE,

    @Json(name = "connect")
    CONNECT,

    @Json(name = "install")
    INSTALL,
}

/** 带键集合的一次答（`cmd_wire::sync::Patch`）：手上那份照它换见 [SyncHand]。项与头里业务那一格在这里不解，原样（`Map` 形）留着。 */
@WireOf("SyncPatch")
@JsonClass(generateAdapter = true)
data class SyncPatch(
    val held: String,
    val whole: Boolean,
    val same: Boolean,
    val order: List<String>?,
    val up: Map<String, Any>,
    val head: SyncHead?,
)

/** 这一问的头：整份有几项（不止这一窗）＋ 业务那一格。 */
@WireOf("SyncHead")
@JsonClass(generateAdapter = true)
data class SyncHead(
    val total: Long,
    val info: Any?,
)
