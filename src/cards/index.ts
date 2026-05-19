import { renderMarkdown, renderPlainText } from "../render";

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

export function renderMessage(rec: JsonlRecord): HTMLElement | null {
  switch (rec.type) {
    case "user":
      return renderUserCard(rec);
    case "assistant":
      return renderAssistantCard(rec);
    case "ai-title":
    case "system":
      return null; // 不入消息流，Tab/状态栏消费
    default:
      return null;
  }
}

function renderUserCard(
  rec: Extract<JsonlRecord, { type: "user" }>,
): HTMLElement {
  const card = document.createElement("div");
  card.className = "card card-user";
  card.appendChild(cardHeader("用户", rec.timestamp));

  const body = document.createElement("div");
  body.className = "card-body";
  const text = extractText(rec.message.content);
  body.innerHTML = renderPlainText(text);
  card.appendChild(body);
  return card;
}

function renderAssistantCard(
  rec: Extract<JsonlRecord, { type: "assistant" }>,
): HTMLElement {
  const card = document.createElement("div");
  card.className = "card card-assistant";
  card.appendChild(cardHeader("Claude", rec.timestamp, rec.message.model));

  const body = document.createElement("div");
  body.className = "card-body";

  const blocks = normalizeBlocks(rec.message.content);
  for (const block of blocks) {
    body.appendChild(renderBlock(block));
  }

  card.appendChild(body);
  return card;
}

function renderBlock(block: ContentBlock): HTMLElement {
  switch (block.type) {
    case "text": {
      const div = document.createElement("div");
      div.className = "block-text";
      div.innerHTML = renderMarkdown(block.text);
      return div;
    }
    case "thinking": {
      const div = document.createElement("div");
      div.className = "block-placeholder block-thinking";
      div.textContent = `▸ 💭 思考 · ${block.thinking.length} 字（M3 展开）`;
      return div;
    }
    case "tool_use": {
      const div = document.createElement("div");
      div.className = "block-placeholder block-tool-use";
      const summary = summarizeInput(block.input);
      div.textContent = `▸ 🔧 ${block.name}  ${summary}`;
      return div;
    }
    case "tool_result": {
      const div = document.createElement("div");
      div.className = "block-placeholder block-tool-result";
      const status = block.is_error ? "✗" : "✓";
      const size = approximateSize(block.content);
      div.textContent = `▾ ${status} ${size}`;
      return div;
    }
  }
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
