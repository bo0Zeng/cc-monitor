/**
 * 合成会话记录：照 Claude Code 落盘的**原始 jsonl** 写（一行一条），交真后端解析成记录成品 —— 台架不写成品
 * （步骤 · 结果 · 卡型 · 用量 · 上下文 · 需手动 …… 一律是真后端读了这些行算出来的）。
 * 正文全是编的占位，不取任何真会话。各种行的形状照后端解析器的判据样本（`tests/backend/agents/claudecode/record_of_tests.rs`）。
 */

type Json = Record<string, unknown>;

/** 造 assistant 那一条时传进来的块（Claude 的原词）。 */
export type RawBlock =
  | { type: "text"; text: string }
  | { type: "thinking"; thinking: string; signature?: string }
  | { type: "tool_use"; id: string; name: string; input: Json };

/** 人以外的来话（原文怎么写那一形由后端认）。 */
export type From =
  | { kind: "agentMessage"; from: string; name?: string; handback?: boolean; body: string }
  | { kind: "taskNotification"; taskId: string; status: string; summary?: string; toolUseId?: string }
  | { kind: "compactSummary"; body?: string }
  | { kind: "slashCommand"; name: string; args?: string };

export interface ToolOpts {
  /** 派出子 agent（`Task` / `Agent`）：子 agent 的那一段记录（写进 `<会话>/subagents/agent-<id>.jsonl`；结果里带它的 `agentId`）。 */
  child?: Convo;
  error?: boolean;
  /** 工具调用前同一条 assistant 里先说的那一句。 */
  lead?: string;
  /** 后台起的命令（入参带 `run_in_background`）：当场回的结果旁边说它的任务号。 */
  bg?: string;
  /** 后台派出子 agent（入参带 `run_in_background`）：当场回的那次结果是「已起」（`status: async_launched`），不是它收场。 */
  launched?: boolean;
}

let uid = 0;
const nextUuid = (): string => {
  uid += 1;
  return `00000000-0000-4000-8000-${uid.toString(16).padStart(12, "0")}`;
};

/** Claude 的项目目录名：工作目录里的 `/` · `\` · `.` · `:` 换成 `-`。 */
export const projectDir = (cwd: string): string => cwd.replace(/[/\\.:]/g, "-");

/** 那个会话的主记录（相对家目录）。 */
export const jsonlOf = (cwd: string, sid: string): string => `.claude/projects/${projectDir(cwd)}/${sid}.jsonl`;

/** 一段对话：按时间往后排。 */
export class Convo {
  /** 一行一条：对象照 JSON 写；串原样写（演认不出 / 读不懂的那一行）。 */
  readonly lines: (Json | string)[] = [];
  /** 这段对话派出的子 agent：agentId → 它那一段。 */
  readonly children = new Map<string, Convo>();
  private t: number;
  private toolN = 0;
  private parent: string | null = null;
  /** 子 agent 那一段（`sid` 是父会话的；文件住父会话那一份旁边）。 */
  readonly agentId: string | null;
  /** 这一份文件最后写的那一刻（epoch ms；盘上的修改时刻）。缺 ⇒ 开页那一刻。 */
  writtenAt: number | undefined;

  constructor(
    readonly sid: string,
    readonly cwd: string,
    start = "2026-10-01T09:00:00Z",
    readonly model = "claude-sonnet-4-5",
    /** 子 agent 那一段：每条带 `isSidechain` 与 `agentId`。 */
    agentId: string | null = null,
  ) {
    this.t = Date.parse(start);
    this.agentId = agentId;
  }

  /** 一个子 agent 的那一段（同一个会话 id、同一个工作目录；从父这一刻往后排）。 */
  child(agentId: string): Convo {
    return new Convo(this.sid, this.cwd, new Date(this.t).toISOString(), this.model, agentId);
  }

  /** 盘上那份文件（相对家目录）。 */
  file(): string {
    const main = jsonlOf(this.cwd, this.sid);
    return this.agentId === null ? main : `${main.slice(0, -".jsonl".length)}/subagents/agent-${this.agentId}.jsonl`;
  }

  /** 原文（一行一条）。 */
  text(): string {
    return this.textFrom(0);
  }

  /** 第 `from` 条起的原文（开页之后接着写的那几行）。 */
  textFrom(from: number): string {
    const tail = this.lines.slice(from);
    return tail.map((l) => (typeof l === "string" ? l : JSON.stringify(l))).join("\n") + (tail.length > 0 ? "\n" : "");
  }

  /** 这一段与它派出的子 agent 那几段：相对家目录的路径 → 原文。 */
  files(): Record<string, string> {
    const out: Record<string, string> = { [this.file()]: this.text() };
    for (const c of this.children.values()) Object.assign(out, c.files());
    return out;
  }

  /** 这一段与它派出的子 agent 那几段里给了写入时刻的：相对家目录的路径 → 修改时刻（epoch ms）。 */
  mtimes(): Record<string, number> {
    const out: Record<string, number> = this.writtenAt === undefined ? {} : { [this.file()]: this.writtenAt };
    for (const c of this.children.values()) Object.assign(out, c.mtimes());
    return out;
  }

  /** 下一条从这一刻往后排（epoch ms；演「两分钟前开始的那一步」之类）。 */
  clock(ms: number): this {
    this.t = ms;
    return this;
  }

  /** 末一条的时刻（epoch ms）。 */
  get lastAt(): number {
    return this.t;
  }

  private at(stepSec: number): string {
    this.t += stepSec * 1000;
    return new Date(this.t).toISOString();
  }

  private push(type: string, stepSec: number, body: Json): string {
    const uuid = nextUuid();
    this.lines.push({
      type,
      uuid,
      parentUuid: this.parent,
      timestamp: this.at(stepSec),
      cwd: this.cwd,
      sessionId: this.sid,
      ...(this.agentId === null ? {} : { isSidechain: true, agentId: this.agentId }),
      ...body,
    });
    this.parent = uuid;
    return uuid;
  }

  title(text: string): this {
    this.lines.push({ type: "ai-title", aiTitle: text, sessionId: this.sid });
    return this;
  }

  user(text: string, opts: { interrupt?: boolean; meta?: boolean } = {}): this {
    const content = opts.interrupt ? "[Request interrupted by user]" : text;
    this.push("user", 45, { message: { role: "user", content }, ...(opts.meta ? { isMeta: true } : {}) });
    return this;
  }

  /** 一条 assistant：`stop: "end_turn"` ＝ 这一轮说完了。 */
  assistant(blocks: RawBlock[], opts: { usage?: Usage; stop?: string } = {}): this {
    this.push("assistant", 20, {
      requestId: `req_${nextUuid().slice(-8)}`,
      message: {
        id: `msg_${nextUuid().slice(-12)}`,
        type: "message",
        role: "assistant",
        model: this.model,
        content: blocks,
        stop_reason: opts.stop ?? (blocks.some((b) => b.type === "tool_use") ? "tool_use" : null),
        usage: opts.usage ?? usage(42_000),
      },
    });
    return this;
  }

  from(f: From): this {
    if (f.kind === "agentMessage") {
      this.push("user", 20, {
        message: { role: "user", content: `<agent-message from="${f.from}">${f.body}</agent-message>` },
        origin: { kind: "peer", from: f.from, ...(f.name ? { name: f.name } : {}), ...(f.handback ? { handback: true } : {}), body: f.body },
      });
    } else if (f.kind === "taskNotification") {
      const tag = (k: string, v: string | undefined) => (v === undefined ? "" : `<${k}>${v}</${k}>`);
      this.push("user", 20, {
        message: { role: "user", content: `<task-notification>${tag("task-id", f.taskId)}${tag("tool-use-id", f.toolUseId)}${tag("status", f.status)}${tag("summary", f.summary)}</task-notification>` },
        origin: { kind: "task-notification" },
      });
    } else if (f.kind === "compactSummary") {
      this.push("user", 20, { message: { role: "user", content: f.body ?? "" }, isCompactSummary: true });
    } else {
      this.push("user", 20, { message: { role: "user", content: `<command-name>${f.name}</command-name><command-args>${f.args ?? ""}</command-args>` } });
    }
    return this;
  }

  say(markdown: string, tokens = 42_000, stop?: string): this {
    return this.assistant([{ type: "text", text: markdown }], { usage: usage(tokens), stop });
  }

  think(thinking: string, then: string): this {
    return this.assistant([
      { type: "thinking", thinking, signature: "c2hvdHM=" },
      { type: "text", text: then },
    ]);
  }

  /** 下一个工具调用的 id（同一条 assistant 里带几次调用时先取 id、自己拼块）。 */
  nextToolId(): string {
    this.toolN += 1;
    return `toolu_${this.sid.slice(0, 4)}${this.agentId ?? ""}${String(this.toolN).padStart(4, "0")}`;
  }

  /** 一次工具调用 ＋ 它的结果（结果是下一条 user 里的 `tool_result`；`null` ＝ 还没有结果）。返回 tool_use 的 id。 */
  tool(name: string, input: Json, result: string | null, opts: ToolOpts = {}): string {
    const id = this.nextToolId();
    const blocks: RawBlock[] = [];
    if (opts.lead) blocks.push({ type: "text", text: opts.lead });
    blocks.push({ type: "tool_use", id, name, input: opts.bg || opts.launched ? { ...input, run_in_background: true } : input });
    this.assistant(blocks);
    this.adopt(opts);
    if (result !== null) this.result(id, result, opts.error ?? false, toolUseResult(name, input, result, opts));
    return id;
  }

  private adopt(opts: ToolOpts): void {
    if (opts.child?.agentId) this.children.set(opts.child.agentId, opts.child);
  }

  /** 一次调用的结果（`tur` ＝ 记录级那一格 `toolUseResult`；缺 ⇒ 照工具名与入参现编，同 [`tool`]）。 */
  answer(id: string, name: string, input: Json, content: string, opts: ToolOpts = {}): this {
    this.adopt(opts);
    return this.result(id, content, opts.error ?? false, toolUseResult(name, input, content, opts));
  }

  result(id: string, content: string, isError = false, tur?: unknown): this {
    this.push("user", 8, {
      message: { role: "user", content: [{ type: "tool_result", tool_use_id: id, content, is_error: isError }] },
      ...(tur === undefined ? {} : { toolUseResult: tur }),
    });
    return this;
  }

  apiError(status: number, text: string): this {
    this.push("assistant", 20, {
      isApiErrorMessage: true,
      apiErrorStatus: status,
      error: status === 429 ? "rate_limit" : "unknown",
      message: { id: `msg_${nextUuid().slice(-12)}`, type: "message", role: "assistant", model: "<synthetic>", content: [{ type: "text", text }], stop_reason: "stop_sequence", usage: usage(0) },
    });
    return this;
  }

  retry(attempt: number, max: number): this {
    this.push("system", 5, { subtype: "api_error", level: "error", retryInMs: 1000 * attempt, retryAttempt: attempt, maxRetries: max, error: { status: 529, error: { type: "overloaded_error" } } });
    return this;
  }

  /** 用户在一轮跑着的时候打进来、排着队的那句话。 */
  queued(text: string): this {
    this.lines.push({ type: "queue-operation", operation: "enqueue", content: text, timestamp: this.at(3), sessionId: this.sid });
    return this;
  }

  /** 后端认不出的一行（新版 Claude 写了一种这一版不认的记录）。 */
  unknown(kind = "shots-unknown-kind"): this {
    this.lines.push({ type: kind, uuid: nextUuid(), timestamp: this.at(5), sessionId: this.sid });
    return this;
  }

  /** 读不懂的一行（写到一半断掉的 JSON）。 */
  broken(): this {
    this.at(5);
    this.lines.push('{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"写到一半');
    return this;
  }
}

/** 工具结果的记录级那一格（`toolUseResult`：改文件的补丁 · 读了几行 · 计划 · 回答 · 派出的子 agent）。照 Claude Code 落盘的原词。 */
function toolUseResult(name: string, input: Json, content: string, opts: ToolOpts): unknown {
  if (opts.error) return undefined;
  if (opts.bg) return { stdout: "", stderr: "", interrupted: false, isImage: false, backgroundTaskId: opts.bg };
  const str = (k: string): string => (typeof input[k] === "string" ? (input[k] as string) : "");
  if (name === "Edit") {
    const old = str("old_string").split("\n");
    const neu = str("new_string").split("\n");
    return { filePath: str("file_path"), oldString: str("old_string"), newString: str("new_string"), structuredPatch: [{ oldStart: 1, oldLines: old.length, newStart: 1, newLines: neu.length, lines: [...old.map((l) => "-" + l), ...neu.map((l) => "+" + l)] }] };
  }
  if (name === "Write") return { type: "create", filePath: str("file_path"), content: str("content"), structuredPatch: [] };
  if (name === "Read") return { type: "text", file: { filePath: str("file_path"), content, numLines: content.split("\n").length, startLine: 1, totalLines: content.split("\n").length } };
  if (name === "Bash") return { stdout: content, stderr: "", interrupted: false, isImage: false };
  if (name === "ExitPlanMode") return { plan: str("plan"), isAgent: false };
  if (name === "AskUserQuestion") {
    const answers: Record<string, string> = {};
    for (const m of content.matchAll(/"([^"]*)"="([^"]*)"/g)) answers[m[1]] = m[2];
    return { questions: input.questions, answers };
  }
  if ((name === "Task" || name === "Agent") && opts.child?.agentId) return { status: opts.launched ? "async_launched" : "completed", agentId: opts.child.agentId, content: [{ type: "text", text: content }] };
  return undefined;
}

export interface Usage {
  input_tokens: number;
  cache_creation_input_tokens: number;
  cache_read_input_tokens: number;
  output_tokens: number;
}

export function usage(contextTokens: number): Usage {
  return {
    input_tokens: 12,
    cache_creation_input_tokens: Math.round(contextTokens * 0.05),
    cache_read_input_tokens: Math.round(contextTokens * 0.95),
    output_tokens: 640,
  };
}
