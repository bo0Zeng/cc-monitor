/**
 * 合成会话记录：直接造后端的成品（通用记录 `LineRecord`：said / reply / retry / title / queued），
 * 正文全是编的占位，不取任何真会话。成品里没有、假后端几问要用的那几样（用量）记在旁边一张表里（`usageOf`）。
 */
import { hm } from "./clock";
import type { LineRecord } from "../../../src/frontend/ui/generated/LineRecord";
import type { Block } from "../../../src/frontend/ui/generated/Block";
import type { ApiReason } from "../../../src/frontend/ui/generated/ApiReason";
import type { ToolCard } from "../../../src/frontend/ui/generated/ToolCard";
import type { ChildRunTag } from "../../../src/frontend/ui/generated/ChildRunTag";
import type { ToolStep } from "../../../src/frontend/ui/generated/ToolStep";
import type { StepResult } from "../../../src/frontend/ui/generated/StepResult";
import type { Speaker } from "../../../src/frontend/ui/generated/Speaker";
import RECORD_GOLDEN from "../../__fixtures__/record.golden.jsonl?raw";

/** 核心金样那几条（字 · 摘录 · 提问卡与计划卡的 `steps[].ask` 都是后端真写的，这里不另写）：`case` ⇒ 记录。 */
const GOLDEN: Record<string, LineRecord> = Object.fromEntries(
  RECORD_GOLDEN.split("\n")
    .filter((l) => l.trim() !== "")
    .map((l) => JSON.parse(l) as { case: string; record: LineRecord })
    .map((g) => [g.case, g.record]),
);
/** 台架照核心金样直接取的那几条（别的还是这里合成）。 */
type GoldenCase = "unread-unknown" | "unread-parse-failed" | "reply-ask" | "said-ask-answered" | "said-plan-approved";

// 假后端也出记录成品里过程那几格（`steps` · `results` · 报错原因），口径照真后端 `agents/claudecode/steps.rs` 的那张表抄一份小的。
const PATH_ARG: Record<string, string> = { Read: "file_path", Edit: "file_path", Write: "file_path", MultiEdit: "file_path" };
const MAIN_ARG: Record<string, string> = { Bash: "command", Grep: "pattern", Glob: "pattern", WebFetch: "url", WebSearch: "query", Task: "description", Agent: "description" };
const BARE = new Set(["TodoWrite", "ExitPlanMode", "AskUserQuestion"]);
function fakeStep(name: string, input: Record<string, unknown>): ToolStep {
  const str = (k: string) => (typeof input[k] === "string" ? (input[k] as string).split(/\s+/).join(" ") : undefined);
  if (PATH_ARG[name]) return { tool: name, arg: str(PATH_ARG[name]), path: true, known: true };
  if (MAIN_ARG[name]) return { tool: name, arg: str(MAIN_ARG[name]), note: MAIN_ARG[name] === "description" ? undefined : str("description"), known: true };
  return { tool: name, known: BARE.has(name) };
}
// 右侧那一句（`text` · `timed`）是核心写的（`StepResult::written`）：假后端不替它写，只给「都说不上 ⇒ 写耗时」那一形；记录换成真后端出之后才有真句子。
const UNSAID = { text: "", timed: "{dur}" };
function fakeResult(name: string, input: Record<string, unknown>, content: string, isError: boolean): StepResult {
  return { ...counts(name, input, content, isError), ...UNSAID };
}
function counts(name: string, input: Record<string, unknown>, content: string, isError: boolean): Omit<StepResult, "text"> {
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

/** 造 assistant 那一条时传进来的块：thinking 照 Claude 的叫法给，其余就是成品块。 */
type InBlock = Block | { type: "thinking"; thinking: string; signature?: string };

let uid = 0;
const nextUuid = (): string => {
  uid += 1;
  return `00000000-0000-4000-8000-${uid.toString(16).padStart(12, "0")}`;
};

/** Claude 那一家记在回复上的用量（成品里没有；假后端算「上下文用了多少」时读）。 */
export interface Usage {
  input_tokens: number;
  cache_creation_input_tokens: number;
  cache_read_input_tokens: number;
  output_tokens: number;
}

/** 成品里没有的用量：回复那一条 ⇒ 它的用量。 */
export const usageOf = new WeakMap<LineRecord, Usage>();

export interface ToolOpts {
  card?: ToolCard;
  child?: ChildRunTag;
}

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

const reasonOf = (status: number): ApiReason =>
  status === 429 ? "quota" : status === 401 || status === 403 ? "auth" : status >= 500 ? "overloaded" : "unknown";

/** 一段对话：按时间往后排。 */
export class Convo {
  readonly records: LineRecord[] = [];
  private t: number;
  private toolN = 0;
  private titles = 0;

  constructor(
    readonly sid: string,
    readonly cwd: string,
    start = "2026-10-01T09:00:00Z",
    readonly model = "claude-sonnet-4-5",
  ) {
    this.t = Date.parse(start);
  }

  /** 下一条的时刻 ＋ 它的钟面（真后端解析时填 `timeText`）。 */
  private at(stepSec = 20): { at: string; timeText: string } {
    this.t += stepSec * 1000;
    return { at: new Date(this.t).toISOString(), timeText: hm(this.t) };
  }

  title(text: string): this {
    this.titles += 1;
    this.records.push({ agent: "claude", id: `@title-${this.titles}`, t: "title", text, by: "agent" });
    return this;
  }

  user(text: string, opts: { interrupt?: boolean; meta?: boolean } = {}): this {
    this.records.push({
      agent: "claude",
      id: nextUuid(),
      ...this.at(45),
      t: "said",
      who: opts.interrupt
        ? { speaker: { kind: "interrupt" }, text: "" }
        : opts.meta
          ? { speaker: { kind: "system", body: text.trim() }, text: "" }
          : { speaker: { kind: "human" }, text, pasted: pastedOf(text) },
      blocks: [{ type: "text", text }],
      cwd: this.cwd,
    });
    return this;
  }

  /** 一条 assistant：`blocks` 里的 tool_use 用 [`tool`] 造。`stop: "end_turn"` ＝ 这一轮说完了。 */
  assistant(blocks: InBlock[], opts: { cards?: Record<string, ToolCard>; runs?: Record<string, ChildRunTag>; usage?: Usage; stop?: string } = {}): this {
    const out: Block[] = blocks.map((b) => (b.type === "thinking" && "thinking" in b ? { type: "thinking", text: b.thinking } : (b as Block)));
    const uses = out.filter((b): b is Extract<Block, { type: "tool_use" }> => b.type === "tool_use");
    const rec: LineRecord = {
      agent: "claude",
      id: nextUuid(),
      ...this.at(),
      t: "reply",
      blocks: out,
      model: this.model,
      autoReply: false,
      endsTurn: opts.stop === "end_turn",
      cards: opts.cards,
      steps: uses.length > 0 ? Object.fromEntries(uses.map((b) => [b.id, fakeStep(b.name, b.input as Record<string, unknown>)])) : undefined,
      runs: opts.runs,
    };
    usageOf.set(rec, opts.usage ?? usage(42_000));
    this.records.push(rec);
    return this;
  }

  /** 一条不是人说的 user 记录（事件条：agent 交回 / 来话 · 另一会话 · 后台通知）。 */
  from(speaker: Speaker): this {
    this.records.push({ agent: "claude", id: nextUuid(), ...this.at(20), t: "said", who: { speaker, text: "" }, blocks: [], cwd: this.cwd });
    return this;
  }

  say(markdown: string, tokens = 42_000, stop?: string): this {
    return this.assistant([{ type: "text", text: markdown }], { usage: usage(tokens), stop });
  }

  think(thinking: string, then: string): this {
    return this.assistant([
      { type: "thinking", thinking },
      { type: "text", text: then },
    ]);
  }

  /** 一次工具调用 ＋ 它的结果（结果是下一条 said 里的 tool_result）。返回 tool_use 的 id。 */
  tool(name: string, input: Record<string, unknown>, result: string | null, opts: ToolOpts & { error?: boolean; lead?: string } = {}): string {
    this.toolN += 1;
    const id = `toolu_${this.sid.slice(0, 4)}${String(this.toolN).padStart(4, "0")}`;
    const blocks: InBlock[] = [];
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
    this.records.push({
      agent: "claude",
      id: nextUuid(),
      ...this.at(8),
      t: "said",
      who: { speaker: { kind: "toolResult" }, text: "" },
      blocks: [{ type: "tool_result", for: id, content: [{ type: "text", text: content }], isError }],
      results: { [id]: res ?? { ok: !isError, ...UNSAID } },
      cwd: this.cwd,
    });
    return this;
  }

  apiError(status: number, text: string): this {
    this.records.push({
      agent: "claude",
      id: nextUuid(),
      ...this.at(),
      t: "reply",
      blocks: [{ type: "text", text }],
      autoReply: false,
      endsTurn: false,
      error: { reason: reasonOf(status), status },
    });
    return this;
  }

  retry(attempt: number, max: number): this {
    this.records.push({ agent: "claude", id: nextUuid(), ...this.at(5), t: "retry", reason: "overloaded", attempt, max });
    return this;
  }

  /** 照核心金样那一条（认不出的一行 · 一问一计划与它们的答），只换 id 与时刻；调用 id（`call-ask` · `call-plan`）照金样，问与答对得上。 */
  golden(name: GoldenCase): this {
    const g = GOLDEN[name];
    if (!g) throw new Error(`核心金样里没有 ${name}`);
    this.records.push({ ...g, id: nextUuid(), ...this.at(5) });
    return this;
  }

  /** 用户打断时说的那句话（插进正在跑的那一轮）。 */
  queued(text: string): this {
    this.records.push({ agent: "claude", id: `@queued-${nextUuid().slice(-6)}`, ...this.at(3), t: "queued", who: { speaker: { kind: "human" }, text } });
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
