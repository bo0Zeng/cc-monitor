import { renderMarkdown, renderPlainText } from "../render";
import { parseSlashCommand, buildSlashCommandCard } from "./slash";
import { isCompactSummary, buildCompactSummaryCard } from "./compact";
import { isAgentTool, buildAgentCard } from "./subagent";

// === Rust 端 JsonlRecord 的 TS 镜像 ===

export interface ApiMessage {
  role: string;
  content: unknown; // string | ContentBlock[]
  model?: string;
  usage?: Usage;
}

export interface Usage {
  input_tokens: number;
  cache_creation_input_tokens?: number;
  cache_read_input_tokens?: number;
  output_tokens: number;
}

export type ContentBlock =
  | { type: "text"; text: string }
  | { type: "thinking"; thinking: string; signature?: string }
  | { type: "tool_use"; id: string; name: string; input: unknown }
  | {
      type: "tool_result";
      tool_use_id: string;
      content: unknown;
      is_error?: boolean;
    };

export type JsonlRecord =
  | {
      type: "user";
      uuid: string;
      timestamp: string;
      message: ApiMessage;
      cwd?: string;
      sessionId?: string;
    }
  | {
      type: "assistant";
      uuid: string;
      timestamp: string;
      message: ApiMessage;
      sessionId?: string;
    }
  | { type: "ai-title"; aiTitle: string; sessionId: string }
  | {
      type: "system";
      subtype?: string;
      durationMs?: number;
      messageCount?: number;
      timestamp: string;
    };

// === 卡片渲染 ===

/**
 * 渲染上下文，沿调用链向下传递。
 *
 * 字段命名保持稳定 —— 跨模块（cards/subagent、tabs）依赖。
 */
export interface RenderContext {
  /** 父 JSONL 路径，subagent 模块用它定位 `<parent>/subagents/` 目录 */
  parentPath: string;
  /**
   * tool_use_id → tool_name 映射。tool_use 出现在 assistant 消息，tool_result
   * 出现在下一条 user 消息，跨消息不能就地反查；TabManager（或 subagent 嵌套
   * 渲染）持有这张 Map 跨 renderMessage 调用累积。renderBlock 在 tool_use
   * 时写入，在 tool_result 时读取来标注工具名。
   */
  toolUseNames: Map<string, string>;
}

export type RenderResult =
  | { kind: "skip" }
  /** 普通独立卡片（user 或含 text 的 assistant） */
  | { kind: "card"; element: HTMLElement }
  /**
   * 工具组成员：assistant 消息全部由 thinking/tool_use/tool_result 构成，没 text。
   * TabManager 会把连续的 tool-group 合并到同一个外层折叠卡。
   * `units` 是每个块单独的折叠条元素。
   */
  | { kind: "tool-group"; timestamp: string; units: HTMLElement[] };

export function renderMessage(rec: JsonlRecord, ctx: RenderContext): RenderResult {
  switch (rec.type) {
    case "user": {
      const text = extractText(rec.message.content);
      if (!text.trim()) return { kind: "skip" };
      if (isCompactSummary(text)) {
        return {
          kind: "card",
          element: buildCompactSummaryCard(text, rec.timestamp, formatTime),
        };
      }
      const slash = parseSlashCommand(text);
      if (slash) {
        return {
          kind: "card",
          element: buildSlashCommandCard(slash, rec.timestamp, formatTime),
        };
      }
      return { kind: "card", element: buildUserCard(rec, text) };
    }
    case "assistant": {
      const blocks = normalizeBlocks(rec.message.content);
      const meaningful = blocks.filter((b) => {
        if (b.type === "text") return b.text.trim().length > 0;
        if (b.type === "thinking") return b.thinking.trim().length > 0;
        return true;
      });
      if (meaningful.length === 0) return { kind: "skip" };

      const hasText = meaningful.some((b) => b.type === "text");
      if (hasText) {
        return {
          kind: "card",
          element: buildAssistantCard(rec, meaningful, ctx),
        };
      }
      // 全是 thinking / tool_use / tool_result → 工具组成员
      return {
        kind: "tool-group",
        timestamp: rec.timestamp,
        units: meaningful.map((b) => renderBlock(b, rec.timestamp, ctx)),
      };
    }
    case "ai-title":
    case "system":
      return { kind: "skip" };
    default:
      return { kind: "skip" };
  }
}

/** 工具组外层折叠卡 —— TabManager 维护一组，连续的 tool-group 都追加进来 */
export interface ToolGroup {
  root: HTMLDetailsElement;
  body: HTMLElement;
  summary: HTMLElement;
  count: number;
  startedAt: string;
}

export function buildToolGroup(startedAt: string): ToolGroup {
  const root = document.createElement("details");
  root.className = "card card-tool-group";

  const summary = document.createElement("summary");
  summary.className = "card-tool-group-summary";
  root.appendChild(summary);

  const body = document.createElement("div");
  body.className = "card-tool-group-body";
  root.appendChild(body);

  const group: ToolGroup = { root, body, summary, count: 0, startedAt };
  updateToolGroupSummary(group);
  return group;
}

export function addToToolGroup(group: ToolGroup, units: HTMLElement[]): void {
  for (const u of units) group.body.appendChild(u);
  group.count += units.length;
  updateToolGroupSummary(group);
}

function updateToolGroupSummary(group: ToolGroup): void {
  group.summary.textContent = `🔧 工具调用 · ${group.count} 个 · 自 ${formatTime(group.startedAt)}`;
}

function buildUserCard(
  rec: Extract<JsonlRecord, { type: "user" }>,
  text: string,
): HTMLElement {
  const card = document.createElement("div");
  card.className = "card card-user";
  card.appendChild(cardHeader("用户", rec.timestamp));

  const body = document.createElement("div");
  body.className = "card-body";
  body.innerHTML = renderPlainText(text);
  card.appendChild(body);
  return card;
}

function buildAssistantCard(
  rec: Extract<JsonlRecord, { type: "assistant" }>,
  meaningful: ContentBlock[],
  ctx: RenderContext,
): HTMLElement {
  const card = document.createElement("div");
  card.className = "card card-assistant";
  card.appendChild(cardHeader("Claude", rec.timestamp, rec.message.model));

  const body = document.createElement("div");
  body.className = "card-body";
  for (const block of meaningful) {
    body.appendChild(renderBlock(block, rec.timestamp, ctx));
  }
  card.appendChild(body);
  return card;
}

function renderBlock(block: ContentBlock, timestamp: string, ctx: RenderContext): HTMLElement {
  switch (block.type) {
    case "text": {
      const div = document.createElement("div");
      div.className = "block-text";
      div.innerHTML = renderMarkdown(block.text);
      return div;
    }
    case "thinking": {
      return makeCollapsible(
        "block-thinking",
        `💭 思考 · ${block.thinking.length} 字`,
        () => {
          const body = document.createElement("div");
          body.className = "block-body block-body-md";
          body.innerHTML = renderMarkdown(block.thinking);
          return body;
        },
      );
    }
    case "tool_use": {
      // 记下 id → name，给下一条消息的 tool_result 反查用
      ctx.toolUseNames.set(block.id, block.name);

      // Agent / Task tool_use → 折叠卡内嵌渲染 subagent JSONL
      if (isAgentTool(block.name)) {
        return buildAgentCard(
          block.input as Parameters<typeof buildAgentCard>[0],
          timestamp,
          ctx,
          renderMessage,
        );
      }
      const summary = summarizeInput(block.input);
      return makeCollapsible(
        "block-tool-use",
        `🔧 ${block.name}  ${summary}`,
        () => {
          const pre = document.createElement("pre");
          pre.className = "block-body block-body-json";
          pre.textContent = prettyJson(block.input);
          return pre;
        },
      );
    }
    case "tool_result": {
      const text = renderResultContent(block.content);
      const toolName = ctx.toolUseNames.get(block.tool_use_id) ?? "工具";
      const exitCode = block.is_error ? extractExitCode(text) : null;
      const preview = firstLinePreview(text, 80);
      const statusIcon = block.is_error ? "✗" : "✓";
      const exitTag = exitCode !== null ? ` exit ${exitCode}` : "";
      const cls = block.is_error
        ? "block-tool-result block-error"
        : "block-tool-result";
      const summaryText = preview
        ? `${statusIcon} ${toolName}${exitTag}  ·  ${preview}`
        : `${statusIcon} ${toolName}${exitTag}  ·  ${approximateSize(block.content)}`;
      return makeCollapsible(cls, summaryText, () => {
        const pre = document.createElement("pre");
        pre.className = "block-body block-body-result";
        pre.textContent = text;
        return pre;
      });
    }
    default: {
      // 未知 block.type（如 server_tool_use / web_search_tool_result 等扩展类型）
      // 不抛异常，给出可识别占位以便诊断。
      const unknownType = (block as { type?: unknown }).type;
      console.warn("renderBlock: unknown block type", unknownType, block);
      const placeholder = document.createElement("div");
      placeholder.className = "block-unknown";
      placeholder.textContent = `⚠️ 未知 block 类型: ${String(unknownType)}`;
      return placeholder;
    }
  }
}

/**
 * 构造 <details><summary>summaryText</summary><body></body></details>。
 * body 用 lazy 函数生成，首次展开时才渲染（renderMarkdown 不便宜）。
 */
function makeCollapsible(
  cls: string,
  summaryText: string,
  bodyFactory: () => HTMLElement,
): HTMLElement {
  const d = document.createElement("details");
  d.className = `block-collapsible ${cls}`;

  const s = document.createElement("summary");
  s.className = "block-summary";
  s.textContent = summaryText;
  d.appendChild(s);

  let rendered = false;
  d.addEventListener("toggle", () => {
    if (d.open && !rendered) {
      d.appendChild(bodyFactory());
      rendered = true;
    }
  });
  return d;
}

function prettyJson(v: unknown): string {
  try {
    return JSON.stringify(v, null, 2);
  } catch {
    return String(v);
  }
}

/** tool_result.content 可能是 string / ContentBlock[] / object */
function renderResultContent(content: unknown): string {
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    const parts: string[] = [];
    for (const item of content) {
      if (!item || typeof item !== "object") {
        parts.push(prettyJson(item));
        continue;
      }
      const t = (item as { type?: string }).type;
      if (t === "text" && typeof (item as { text?: unknown }).text === "string") {
        parts.push((item as { text: string }).text);
      } else if (t === "image") {
        // 图片是给模型看的 base64，监控视图不展开
        const src = (item as { source?: { media_type?: string } }).source;
        parts.push(`[image ${src?.media_type ?? "unknown"}]`);
      } else {
        parts.push(prettyJson(item));
      }
    }
    return parts.join("\n");
  }
  return prettyJson(content);
}

/** 第一行非空预览，截到 max 字符 */
function firstLinePreview(text: string, max: number): string {
  const firstLine = text.split("\n").find((l) => l.trim().length > 0) ?? "";
  const trimmed = firstLine.trim();
  if (trimmed.length <= max) return trimmed;
  return trimmed.slice(0, max - 1) + "…";
}

/** 从 Bash 失败 tool_result 文本里抠 exit code（Claude Code 会把它写成 "Exit code N" 一行） */
function extractExitCode(text: string): number | null {
  const m = text.match(/Exit code (\d+)/);
  return m ? Number(m[1]) : null;
}

// === helpers ===

function cardHeader(
  role: string,
  timestamp: string,
  model?: string,
): HTMLElement {
  const h = document.createElement("div");
  h.className = "card-header";
  const r = document.createElement("span");
  r.className = "role";
  r.textContent = role;
  h.appendChild(r);
  const t = document.createElement("span");
  t.className = "ts";
  t.textContent = formatTime(timestamp);
  h.appendChild(t);
  if (model) {
    const m = document.createElement("span");
    m.className = "model";
    m.textContent = model;
    h.appendChild(m);
  }
  return h;
}

function formatTime(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  } catch {
    return iso;
  }
}

function normalizeBlocks(content: unknown): ContentBlock[] {
  if (typeof content === "string") {
    return [{ type: "text", text: content }];
  }
  if (Array.isArray(content)) {
    return content.filter((c): c is ContentBlock =>
      Boolean(c) && typeof (c as { type?: unknown }).type === "string",
    );
  }
  return [];
}

function extractText(content: unknown): string {
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return content
      .map((b) =>
        b && typeof b === "object" && (b as { type?: string }).type === "text"
          ? String((b as { text?: unknown }).text ?? "")
          : "",
      )
      .filter(Boolean)
      .join("\n");
  }
  return "";
}

function summarizeInput(input: unknown): string {
  if (input === null || input === undefined) return "";
  if (typeof input === "string") return truncate(input, 60);
  try {
    return truncate(JSON.stringify(input), 60);
  } catch {
    return "";
  }
}

function truncate(s: string, n: number): string {
  return s.length > n ? s.slice(0, n) + "…" : s;
}

function approximateSize(content: unknown): string {
  if (typeof content === "string") {
    return `${content.length} chars`;
  }
  try {
    return `${JSON.stringify(content).length} chars`;
  } catch {
    return "";
  }
}
