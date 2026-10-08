/**
 * 合成会话记录：按 Claude Code 记录的结构造（user / assistant / tool_use / tool_result / thinking …），
 * 正文全是编的占位，不取任何真会话。
 */
import { hm } from "./clock";
import type { JsonlRecord } from "../../../src/frontend/ui/generated/JsonlRecord";
import type { ToolCard } from "../../../src/frontend/ui/generated/ToolCard";
import type { ChildRunTag } from "../../../src/frontend/ui/generated/ChildRunTag";
import type { Usage } from "../../../src/frontend/ui/generated/Usage";
import type { ToolStep } from "../../../src/frontend/ui/generated/ToolStep";
import type { StepResult } from "../../../src/frontend/ui/generated/StepResult";
import type { Speaker } from "../../../src/frontend/ui/generated/Speaker";

// 假后端也出记录成品里过程那几格（`toolSteps` · `toolResults` · `apiReason`），口径照真后端 `agents/claudecode/steps.rs` 的那张表抄一份小的。
const PATH_ARG: Record<string, string> = { Read: "file_path", Edit: "file_path", Write: "file_path", MultiEdit: "file_path" };
const MAIN_ARG: Record<string, string> = { Bash: "command", Grep: "pattern", Glob: "pattern", WebFetch: "url", WebSearch: "query", Task: "description", Agent: "description" };
const BARE = new Set(["TodoWrite", "ExitPlanMode", "AskUserQuestion"]);
function fakeStep(name: string, input: Record<string, unknown>): ToolStep {
  const str = (k: string) => (typeof input[k] === "string" ? (input[k] as string).split(/\s+/).join(" ") : undefined);
  if (PATH_ARG[name]) return { tool: name, arg: str(PATH_ARG[name]), path: true, known: true };
  if (MAIN_ARG[name]) return { tool: name, arg: str(MAIN_ARG[name]), note: MAIN_ARG[name] === "description" ? undefined : str("description"), known: true };
  return { tool: name, known: BARE.has(name) };
}
function fakeResult(name: string, input: Record<string, unknown>, content: string, isError: boolean): StepResult {
  if (isError) return { ok: false };
  const lines = (s: unknown) => (typeof s === "string" && s.length > 0 ? s.split("\n").length : 0);
  if (name === "Edit") return { ok: true, added: lines(input.new_string), removed: lines(input.old_string) };
  if (name === "Write") return { ok: true, added: lines(input.content), removed: 0 };
  if (name === "Read") return { ok: true, lines: lines(content) };
  if (name === "Bash") return { ok: true, lines: lines(content) };
  if (content.startsWith("User has approved your plan")) return { ok: true, answer: { kind: "approved" } };
  const picked = [...content.matchAll(/"[^"]*"="([^"]*)"/g)].map((m) => m[1]);
  if (picked.length > 0) return { ok: true, answer: { kind: "picked", options: picked } };
  return { ok: true };
}

type Block = Record<string, unknown> & { type: string };

let uid = 0;
const nextUuid = (): string => {
  uid += 1;
  return `00000000-0000-4000-8000-${uid.toString(16).padStart(12, "0")}`;
};

export interface ToolOpts {
  card?: ToolCard;
  child?: ChildRunTag;
}

/** 一段对话：按时间往后排，每条记录挂在上一条后面。 */
/** 人粘贴进来的块：照后端 `claudecode/text.rs::pasted_spans` 那一形（UTF-16 下标 · 正文那一截 · 行数）。没有 ⇒ 缺。 */
function pastedOf(text: string): { id?: string; start: number; end: number; bodyStart: number; bodyEnd: number; lines: number }[] | undefined {
  const out = [];
  const re = /<pasted_content(?: id="([^"]*)")?>([\s\S]*?)<\/pasted_content(?: id="[^"]*")?>/g;
  for (let m = re.exec(text); m; m = re.exec(text)) {
    const bodyStart = m.index + m[0].indexOf(">") + 1;
    const body = m[2];
    out.push({ ...(m[1] ? { id: m[1] } : {}), start: m.index, end: m.index + m[0].length, bodyStart, bodyEnd: bodyStart + body.length, lines: body.replace(/^[\r\n]+|[\r\n]+$/g, "").split("\n").length });
  }
  return out.length > 0 ? out : undefined;
}

export class Convo {
  readonly records: JsonlRecord[] = [];
  private t: number;
  private prev: string | null = null;
  private toolN = 0;

  constructor(
    readonly sid: string,
    readonly cwd: string,
    start = "2026-10-01T09:00:00Z",
    readonly model = "claude-sonnet-4-5",
  ) {
    this.t = Date.parse(start);
  }

  private stamp(stepSec = 20): string {
    this.t += stepSec * 1000;
    return new Date(this.t).toISOString();
  }

  /** 下一条的时刻 ＋ 它的钟面（真后端解析时填 `timeText`）。 */
  private at(stepSec = 20): { timestamp: string; timeText: string } {
    const timestamp = this.stamp(stepSec);
    return { timestamp, timeText: hm(this.t) };
  }

  title(text: string): this {
    this.records.push({ type: "ai-title", aiTitle: text, sessionId: this.sid });
    return this;
  }

  user(text: string, opts: { interrupt?: boolean; meta?: boolean } = {}): this {
    const uuid = nextUuid();
    this.records.push({
      type: "user",
      uuid,
      ...this.at(45),
      message: { role: "user", content: text, model: null, usage: null },
      cwd: this.cwd,
      sessionId: this.sid,
      parentUuid: this.prev,
      forkedFrom: null,
      userText: opts.interrupt
        ? { speaker: { kind: "interrupt" }, text: "" }
        : opts.meta
          ? { speaker: { kind: "system", body: text.trim() }, text: "" }
          : { speaker: { kind: "human" }, text, pasted: pastedOf(text) },
    });
    this.prev = uuid;
    return this;
  }

  /** 一条 assistant：`blocks` 里的 tool_use 用 [`tool`] 造。 */
  assistant(blocks: Block[], opts: { cards?: Record<string, ToolCard>; runs?: Record<string, ChildRunTag>; usage?: Usage; stop?: string } = {}): this {
    const uuid = nextUuid();
    this.records.push({
      type: "assistant",
      uuid,
      ...this.at(),
      message: {
        role: "assistant",
        content: blocks,
        model: this.model,
        usage: opts.usage ?? usage(42_000),
        stop_reason: opts.stop,
      },
      sessionId: this.sid,
      requestId: `req_${uuid.slice(-8)}`,
      parentUuid: this.prev,
      forkedFrom: null,
      isApiErrorMessage: false,
      error: null,
      apiErrorStatus: null,
      toolCards: opts.cards,
      childRuns: opts.runs,
      toolSteps: Object.fromEntries(
        blocks.filter((b): b is Block & { type: "tool_use"; id: string; name: string; input: Record<string, unknown> } => b.type === "tool_use").map((b) => [b.id, fakeStep(b.name, b.input)]),
      ),
    });
    this.prev = uuid;
    return this;
  }

  /** 一条不是人说的 user 记录（事件条：agent 交回 / 来话 · 另一会话 · 后台通知）。 */
  from(speaker: Speaker): this {
    const uuid = nextUuid();
    this.records.push({
      type: "user",
      uuid,
      ...this.at(20),
      message: { role: "user", content: "<frame/>", model: null, usage: null },
      cwd: this.cwd,
      sessionId: this.sid,
      parentUuid: this.prev,
      forkedFrom: null,
      userText: { speaker, text: "" },
    });
    this.prev = uuid;
    return this;
  }

  /** `stop: "end_turn"` ＝ 这一轮说完了（一轮的摘要据它认「收尾」）。 */
  say(markdown: string, tokens = 42_000, stop?: string): this {
    return this.assistant([{ type: "text", text: markdown }], { usage: usage(tokens), stop });
  }

  think(thinking: string, then: string): this {
    return this.assistant([
      { type: "thinking", thinking, signature: "sig" },
      { type: "text", text: then },
    ]);
  }

  /** 一次工具调用 ＋ 它的结果（结果是下一条 user 记录里的 tool_result）。返回 tool_use 的 id。 */
  tool(name: string, input: Record<string, unknown>, result: string | null, opts: ToolOpts & { error?: boolean; lead?: string } = {}): string {
    this.toolN += 1;
    const id = `toolu_${this.sid.slice(0, 4)}${String(this.toolN).padStart(4, "0")}`;
    const blocks: Block[] = [];
    if (opts.lead) blocks.push({ type: "text", text: opts.lead });
    blocks.push({ type: "tool_use", id, name, input });
    this.assistant(blocks, {
      cards: opts.card ? { [id]: opts.card } : undefined,
      runs: opts.child ? { [id]: opts.child } : undefined,
    });
    if (result !== null) this.result(id, result, opts.error ?? false, fakeResult(name, input, result, opts.error ?? false));
    return id;
  }

  result(id: string, content: string, isError = false, res?: StepResult): this {
    const uuid = nextUuid();
    this.records.push({
      type: "user",
      uuid,
      ...this.at(8),
      message: {
        role: "user",
        content: [{ type: "tool_result", tool_use_id: id, content, is_error: isError }],
        model: null,
        usage: null,
      },
      cwd: this.cwd,
      sessionId: this.sid,
      parentUuid: this.prev,
      forkedFrom: null,
      userText: { speaker: { kind: "toolResult" }, text: "" },
      toolResults: { [id]: res ?? { ok: !isError } },
    });
    this.prev = uuid;
    return this;
  }

  apiError(status: number, text: string): this {
    const uuid = nextUuid();
    this.records.push({
      type: "assistant",
      uuid,
      ...this.at(),
      message: { role: "assistant", content: [{ type: "text", text }], model: "<synthetic>", usage: null },
      sessionId: this.sid,
      requestId: null,
      parentUuid: this.prev,
      forkedFrom: null,
      isApiErrorMessage: true,
      error: { type: "api_error" },
      apiErrorStatus: status,
      apiReason: status === 429 ? "quota" : status === 401 || status === 403 ? "auth" : status >= 500 ? "overloaded" : "unknown",
    });
    this.prev = uuid;
    return this;
  }

  retry(attempt: number, max: number): this {
    const uuid = nextUuid();
    this.records.push({
      type: "system",
      subtype: "api_error",
      durationMs: null,
      messageCount: null,
      ...this.at(5),
      sessionId: this.sid,
      uuid,
      parentUuid: this.prev,
      level: "error",
      retryAttempt: attempt,
      maxRetries: max,
      error: { status: 529, error: { type: "overloaded_error", message: "Overloaded" } },
      apiReason: "overloaded",
    });
    // 记录链照真记录接着走（不接 ⇒ 下一条与它同父，界面会当成被 ESC 回退过的分叉）
    this.prev = uuid;
    return this;
  }

  turnDuration(ms: number): this {
    const uuid = nextUuid();
    this.records.push({
      type: "system",
      subtype: "turn_duration",
      durationMs: ms,
      messageCount: this.records.length,
      ...this.at(1),
      sessionId: this.sid,
      uuid,
      parentUuid: this.prev,
      level: null,
      retryAttempt: null,
      maxRetries: null,
      error: null,
    });
    this.prev = uuid;
    return this;
  }

  /** 用户打断时说的那句话（jsonl 里唯一的存在是一条 queue-operation remove）。 */
  queued(text: string): this {
    this.records.push({ type: "queue-operation", operation: "remove", content: text, ...this.at(3) });
    return this;
  }
}

export function usage(contextTokens: number): Usage {
  return {
    input_tokens: 12,
    cache_creation_input_tokens: Math.round(contextTokens * 0.05),
    cache_read_input_tokens: Math.round(contextTokens * 0.95),
    output_tokens: 640,
  };
}
