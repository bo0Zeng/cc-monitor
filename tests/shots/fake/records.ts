/**
 * 合成会话记录：按 Claude Code 记录的结构造（user / assistant / tool_use / tool_result / thinking …），
 * 正文全是编的占位，不取任何真会话。
 */
import type { JsonlRecord } from "../../../src/frontend/ui/generated/JsonlRecord";
import type { ToolCard } from "../../../src/frontend/ui/generated/ToolCard";
import type { ChildRunTag } from "../../../src/frontend/ui/generated/ChildRunTag";
import type { Usage } from "../../../src/frontend/ui/generated/Usage";

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

  title(text: string): this {
    this.records.push({ type: "ai-title", aiTitle: text, sessionId: this.sid });
    return this;
  }

  user(text: string, opts: { interrupt?: boolean; meta?: boolean } = {}): this {
    const uuid = nextUuid();
    this.records.push({
      type: "user",
      uuid,
      timestamp: this.stamp(45),
      message: { role: "user", content: text, model: null, usage: null },
      cwd: this.cwd,
      sessionId: this.sid,
      isMeta: opts.meta ?? false,
      parentUuid: this.prev,
      forkedFrom: null,
      userText: { clean: text, interrupt: opts.interrupt ?? false },
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
      timestamp: this.stamp(),
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
    });
    this.prev = uuid;
    return this;
  }

  say(markdown: string, tokens = 42_000): this {
    return this.assistant([{ type: "text", text: markdown }], { usage: usage(tokens) });
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
    if (result !== null) this.result(id, result, opts.error ?? false);
    return id;
  }

  result(id: string, content: string, isError = false): this {
    const uuid = nextUuid();
    this.records.push({
      type: "user",
      uuid,
      timestamp: this.stamp(8),
      message: {
        role: "user",
        content: [{ type: "tool_result", tool_use_id: id, content, is_error: isError }],
        model: null,
        usage: null,
      },
      cwd: this.cwd,
      sessionId: this.sid,
      isMeta: false,
      parentUuid: this.prev,
      forkedFrom: null,
      userText: { clean: "", interrupt: false },
    });
    this.prev = uuid;
    return this;
  }

  apiError(status: number, text: string): this {
    const uuid = nextUuid();
    this.records.push({
      type: "assistant",
      uuid,
      timestamp: this.stamp(),
      message: { role: "assistant", content: [{ type: "text", text }], model: "<synthetic>", usage: null },
      sessionId: this.sid,
      requestId: null,
      parentUuid: this.prev,
      forkedFrom: null,
      isApiErrorMessage: true,
      error: { type: "api_error" },
      apiErrorStatus: status,
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
      timestamp: this.stamp(5),
      sessionId: this.sid,
      uuid,
      parentUuid: this.prev,
      level: "error",
      retryAttempt: attempt,
      maxRetries: max,
      error: { status: 529, error: { type: "overloaded_error", message: "Overloaded" } },
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
      timestamp: this.stamp(1),
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
    this.records.push({ type: "queue-operation", operation: "remove", content: text, timestamp: this.stamp(3) });
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
