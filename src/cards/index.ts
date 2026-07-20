/**
 * 卡片渲染的总分发器。
 *
 * `renderMessage(rec, ctx)` 是核心纯函数：给定一条 JsonlRecord + RenderContext，
 * 返回 `RenderResult`（`card` 普通卡 / `tool-group` 工具组单元 / `skip` 不渲染）。
 * 实时 Tab、历史只读视图、subagent 卡三处共用它，保证视觉一致（详 render-stream-record.ts）。
 *
 * 职责边界：
 * - 本文件持有 Rust `JsonlRecord` 的 TS 镜像类型（ApiMessage / ContentBlock 等）。
 * - 按 record.type + content 形态分发：user 气泡 / assistant 卡 / 纯工具 → tool-group /
 *   tool_result 注入到对应 tool_use 折叠条；slash / compact / agent / diff / interactive /
 *   api-error 子卡委派给 cards/ 同级模块。
 * - `stripInternalNoise` 剥 CLI 注入的非真用户输入（含 ESC 中断标记，INVARIANT § 20）。
 * - `pendingToolResults`：tool_result 先于 tool_use 到达时先 fallback 渲染，batch 末
 *   `reconcilePendingToolResults` 重新匹配注入。
 */
import { renderMarkdown, renderPlainText } from "../render";
import { AGENT_PROFILE } from "../agent-profile";
import { parseSlashCommand, buildSlashCommandCard } from "./slash";
import {
  parseBashInput,
  parseBashOutput,
  buildBashInputCard,
  buildBashOutputCard,
} from "./bash";
import { isCompactSummary, buildCompactSummaryCard } from "./compact";
import { isAgentTool, buildAgentCard } from "./subagent";
import { isDiffTool, buildDiffBody } from "./diff";
import {
  isInteractiveTool,
  buildInteractiveCard,
  markInteractiveAnswer,
} from "./interactive";
import { buildApiErrorCard, buildApiRetryCard } from "./api-error";
import { LS_KEYS, safeGet, safeSet } from "../local-storage";
import { formatTimestampShort } from "../format";
import { openSftpPanel } from "../sftp/panel";
import { resolveRemoteConfigByOrigin } from "../settings/remote-section";
import { showActionFailureToast } from "../error-toast";

// === Rust 端 JsonlRecord 的 TS 镜像 ===

interface ApiMessage {
  role: string;
  content: unknown; // string | ContentBlock[]
  model?: string;
  usage?: Usage;
  /** Batch14-F42：一轮结束判据（assistant 终结记录 = "end_turn"），turn-notify 消费。 */
  stop_reason?: string | null;
}

interface Usage {
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
      /** issue #36：CC 2.1.x 队列操作记录（enqueue 带 content，折叠豁免用） */
      type: "queue-operation";
      operation?: string;
      content?: string;
      timestamp?: string;
    }
  | {
      type: "user";
      uuid: string;
      timestamp: string;
      message: ApiMessage;
      cwd?: string;
      sessionId?: string;
      /** issue #8: ESC 回退分支检测用。parentUuid → 上一条 jsonl 记录 uuid。 */
      parentUuid?: string;
      /**
       * Claude Code 注入的 meta 消息（skill/command 展开的 prompt、system-reminder、
       * caveat 等）带 isMeta:true —— 不是用户真正输入，renderMessage 跳过建卡。
       */
      isMeta?: boolean;
    }
  | {
      type: "assistant";
      uuid: string;
      timestamp: string;
      message: ApiMessage;
      sessionId?: string;
      /** issue #8: ESC 回退分支检测用 */
      parentUuid?: string;
      /**
       * issue #21: API 最终失败时 CLI 写的合成 assistant 消息（重试耗尽/不可重试）。
       * isApiErrorMessage 是判定主键；error 是机器可读分类（实测皆 string：
       * authentication_failed / invalid_request / server_error / unknown…勿穷举；
       * 后端按 §18 透传 Value 防类型漂移 → 这里 unknown、用 typeof 守卫）；
       * apiErrorStatus 仅 HTTP 类有。报错文本在 message.content[0].text。
       * 镜像 messages.rs::Assistant。
       */
      isApiErrorMessage?: boolean;
      error?: unknown;
      apiErrorStatus?: number;
    }
  | { type: "ai-title"; aiTitle: string; sessionId: string }
  // Claude Code v2.1.x 起的新名字。aiTitle / customTitle 语义一致 ——
  // 都是会话级语义标题（旧 jsonl 用 ai-title，新 jsonl 用 custom-title）。
  | { type: "custom-title"; customTitle: string; sessionId: string }
  | {
      type: "system";
      subtype?: string;
      durationMs?: number;
      messageCount?: number;
      timestamp: string;
      /** issue #8: 部分 system 记录有 uuid+parentUuid 参与 jsonl 链跟踪 */
      uuid?: string;
      parentUuid?: string;
      /**
       * issue #21: subtype="api_error"（API 调用失败将重试）时有。error 对象两种
       * shape（新版有 .formatted 现成文案），unknown 透传、渲染侧防御性取字段。
       * 镜像 messages.rs::System。
       */
      level?: string;
      retryAttempt?: number;
      maxRetries?: number;
      error?: unknown;
    }
  | {
      /**
       * issue #8: attachment 不渲染（renderMessage 返回 skip），但有 uuid+parentUuid
       * 夹在 user→assistant 之间。前端必须收到才能完整算 ESC 回退主线 ——
       * 否则 parent 链断在 attachment 处，下游整段会被错误折叠为"已被回退"。
       */
      type: "attachment";
      uuid: string;
      timestamp: string;
      parentUuid?: string;
    }
  | {
      /**
       * F63 (issue #49)：**看不懂的记录**——后端 `parser.rs::salvage` 抢救出的
       * 原文 + 链上身份。**不是真实 jsonl 里的类型**，是我们自造的信封（故带
       * `cc-monitor-` 前缀防撞未来真类型）。镜像 `messages.rs::Unrecognized`。
       *
       * 同 attachment：**不建卡**（`renderMessage` 落 `default => skip`），但必须
       * 收到——它可能是链上的一环，缺席会让 children 落 `branching.ts` 的孤儿
       * root → 整棵误折叠。有 uuid 才进链（`extractBranchRecord` 已守）。
       *
       * `raw` 是一字节不改的原文：F63 只做**逃生口**，不认领具体新类型
       * （SS-1 账本：留逃生口就够，别建完整统一格式）。将来要用 `pr-link` /
       * `agent-name` / `worktree-state` 这些，从 `raw` 里取，无需再动后端。
       */
      type: "cc-monitor-unrecognized";
      uuid?: string;
      parentUuid?: string;
      timestamp?: string;
      /** 原文里的 `type`（若有）——诊断/记账按它分类 */
      originalType?: string;
      /** 原始 JSON 行，一字节不改 */
      raw: string;
      /** 为什么没认出来：`unknown-type` / `parse-failed: <serde 原文>` */
      reason: string;
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
   * Batch9-F29：会话来源（null/缺省=本地；string=远端机器 label）。subagent
   * 懒加载据此降级——远端会话的 subagent 文件在远端机器，本地 load_subagent
   * 必然失败（此前渲染成报错+永远失败的"重试"按钮，盘点 #2 UX 误导）。
   */
  origin?: string | null;
  /**
   * tool_use_id → tool_name 映射。tool_use 出现在 assistant 消息，tool_result
   * 出现在下一条 user 消息，跨消息不能就地反查；TabManager（或 subagent 嵌套
   * 渲染）持有这张 Map 跨 renderMessage 调用累积。renderBlock 在 tool_use
   * 时写入，在 tool_result 时读取来标注工具名。
   */
  toolUseNames: Map<string, string>;
  /**
   * tool_use_id → tool_use 折叠条 DOM 引用。tool_result 到达时把结果直接
   * append 到对应 tool_use 内部（不创建独立折叠条），实现"展开命令同时
   * 看到参数 + 输出"的合并 UX。
   */
  toolUseElements: Map<string, HTMLElement>;
  /**
   * v2.3.1 (issue #1)：切块场景下 tool_result 可能在 tool_use 之前到达
   * （head 块含 result，older 块才有 tool_use）。此时 injectOrBuildToolResult
   * 走 fallback 路径产生独立卡，并把 block 引用存到这个 map。
   *
   * 全部 chunks 完成后 TabManager 调 reconcileToolResults 重试匹配：
   * tool_use 现在已经有 host → 注入 + 删 fallback 卡。
   *
   * key = tool_use_id；value = {block, fallback element}。
   *
   * P4：改成**必填** —— SessionViewer / Subagent 之前漏传导致 fallback 路径
   * 永久独立卡（fallback 后无人调 reconcile，那条结果再也不会被注入）。
   * 不需要 reconcile 的 caller 也传一个空 Map 即可。
   */
  pendingToolResults: Map<
    string,
    { block: Extract<ContentBlock, { type: "tool_result" }>; element: HTMLElement }
  >;
  /**
   * P5.5 B 重构：lazy hljs 模式（启动 batch 期间用，避免 N 个代码块同步阻塞主线程）。
   * caller（TabManager）在 inBatch 时设 true；SessionViewer / Subagent 默认 false。
   * 传到 renderMarkdown opts.lazy 决定代码块是否走占位 + IntersectionObserver。
   * 默认 false（不传或 undefined 都视作 eager）。
   */
  lazy?: boolean;
}

export type RenderResult =
  | { kind: "skip" }
  /**
   * 普通独立卡片：user / 含 text 或交互等待工具（issue #21）的 assistant /
   * API 报错卡 / system api_error 重试细条。
   */
  | { kind: "card"; element: HTMLElement }
  /**
   * 工具组成员：assistant 消息全部由 thinking/tool_use/tool_result 构成，没 text
   * 也没交互等待工具（issue #21：含 AskUserQuestion/ExitPlanMode 的走 kind:"card"
   * 保持可见）。TabManager 会把连续的 tool-group 合并到同一个外层折叠卡。
   * `units` 是每个块单独的折叠条元素。
   */
  | { kind: "tool-group"; timestamp: string; units: HTMLElement[] };

export function renderMessage(rec: JsonlRecord, ctx: RenderContext): RenderResult {
  switch (rec.type) {
    case "user": {
      // Claude Code 注入的 meta 消息（skill/command 展开 prompt、system-reminder、
      // caveat…）带 isMeta —— 不是用户真正输入，别当 user 气泡渲染。记录仍在 timeline
      // 里保 parent 链（同 attachment），只是不建卡。
      if (rec.isMeta) return { kind: "skip" };
      const rawText = extractText(rec.message.content);
      if (rawText.trim()) {
        // 先剥 Claude Code CLI 注入的 prompt 包装；剩余文本喂给下游识别 +
        // 渲染。剥干净就 skip 整条。
        const text = stripInternalNoise(rawText);
        if (text.length === 0) {
          return { kind: "skip" };
        }
        if (isCompactSummary(text)) {
          return {
            kind: "card",
            element: buildCompactSummaryCard(text, rec.timestamp, formatTimestampShort),
          };
        }
        const slash = parseSlashCommand(text);
        if (slash) {
          return {
            kind: "card",
            element: buildSlashCommandCard(slash, rec.timestamp, formatTimestampShort),
          };
        }
        // Batch4-F16：`!` bash 模式的输入/输出各渲染成终端风格卡；
        // 识别不了一律 fall through 到 user 气泡原样展示（faithful 底线）。
        const bashIn = parseBashInput(text);
        if (bashIn) {
          return {
            kind: "card",
            element: buildBashInputCard(bashIn, rec.timestamp, formatTimestampShort),
          };
        }
        const bashOut = parseBashOutput(text);
        if (bashOut) {
          return {
            kind: "card",
            element: buildBashOutputCard(bashOut, rec.timestamp, formatTimestampShort),
          };
        }
        return { kind: "card", element: buildUserCard(rec, text) };
      }

      // text 为空 → 多半是工具结果回灌（content 全是 tool_result 块）。
      // tool_result 渲染会注入到对应 tool_use 折叠条内部，返回 null；
      // 只有找不到匹配 tool_use 的 fallback 才产生独立 element。
      const blocks = normalizeBlocks(rec.message.content).filter(
        (b) => b.type === "tool_result",
      );
      if (blocks.length === 0) return { kind: "skip" };
      const units = blocks
        .map((b) => renderBlock(b, rec.timestamp, ctx))
        .filter((el): el is HTMLElement => el !== null);
      if (units.length === 0) return { kind: "skip" };
      return {
        kind: "tool-group",
        timestamp: rec.timestamp,
        units,
      };
    }
    case "assistant": {
      // issue #21：API 最终失败的合成消息 → 红色报错卡（此前被当普通回复渲染，
      // 用户误以为 LLM 还在跑）。在 meaningful 过滤前判，避免被 synthetic 过滤吞掉。
      if (rec.isApiErrorMessage) {
        return {
          kind: "card",
          element: buildApiErrorCard({
            timeLabel: formatTimestampShort(rec.timestamp),
            text: extractText(rec.message.content).trim(),
            category: typeof rec.error === "string" ? rec.error : undefined,
            status: rec.apiErrorStatus,
          }),
        };
      }
      const blocks = normalizeBlocks(rec.message.content);
      const meaningful = blocks.filter((b) => {
        if (b.type === "text") {
          // 过滤 `<synthetic>` 包裹的自动应答（claude 内部 "No response
          // requested." 之类），非真回复
          return b.text.trim().length > 0 && !isSyntheticReply(b.text);
        }
        if (b.type === "thinking") return b.thinking.trim().length > 0;
        return true;
      });
      if (meaningful.length === 0) return { kind: "skip" };

      const hasText = meaningful.some((b) => b.type === "text");
      // issue #21：含交互等待工具（AskUserQuestion / ExitPlanMode）的消息走
      // kind:"card"——它们要默认可见，不能折进 card-tool-group（进组判定在
      // message 级，kind:"card" 是唯一的不进组通路）。
      const hasInteractive = meaningful.some(
        (b) => b.type === "tool_use" && isInteractiveTool(b.name),
      );
      if (hasText || hasInteractive) {
        return {
          kind: "card",
          element: buildAssistantCard(rec, meaningful, ctx),
        };
      }
      // 全是 thinking / tool_use / tool_result → 工具组成员
      const units = meaningful
        .map((b) => renderBlock(b, rec.timestamp, ctx))
        .filter((el): el is HTMLElement => el !== null);
      if (units.length === 0) return { kind: "skip" };
      return {
        kind: "tool-group",
        timestamp: rec.timestamp,
        units,
      };
    }
    case "system":
      // issue #21：API 调用失败将重试的中间态 → 细条提示（此前 system 一律 skip
      // → 完全不可见，重试风暴时用户只看到"卡住"）。其余 system 仍 skip。
      if (rec.subtype === "api_error") {
        return {
          kind: "card",
          element: buildApiRetryCard({
            timeLabel: formatTimestampShort(rec.timestamp),
            retryAttempt: rec.retryAttempt,
            maxRetries: rec.maxRetries,
            error: rec.error,
          }),
        };
      }
      return { kind: "skip" };
    case "ai-title":
    case "custom-title":
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
  group.summary.textContent = `🔧 工具调用 · ${group.count} 个 · 自 ${formatTimestampShort(group.startedAt)}`;
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
  // Phase G 审计修：Codex 会话的 jsonl 是 `rollout-*.jsonl`（同后端 `kind_of_path`/`codex_sid_from_rollout`
  // 的路径判据）；记录本身不带 agent kind，故据会话文件名判 agent 给对的卡头——否则 Codex 的每条文本回复
  // 都错标成 "Claude"。非 Codex（含 live Claude 会话、子 agent）恒 "Claude"（rollout- 前缀是 Codex 独有）。
  const agent = /(^|\/)rollout-[^/]*\.jsonl$/.test(ctx.parentPath) ? "Codex" : "Claude";
  card.appendChild(cardHeader(agent, rec.timestamp, rec.message.model));

  const body = document.createElement("div");
  body.className = "card-body";
  for (const block of meaningful) {
    const el = renderBlock(block, rec.timestamp, ctx);
    if (el) body.appendChild(el);
  }
  card.appendChild(body);
  return card;
}

/**
 * 渲染单个 block。返回 null 表示"已合并到现有 DOM，无需追加新 element"——
 * 当前 tool_result 命中已存在的 tool_use 折叠条时直接注入其内部，会返 null。
 */
function renderBlock(
  block: ContentBlock,
  timestamp: string,
  ctx: RenderContext,
): HTMLElement | null {
  switch (block.type) {
    case "text": {
      const div = document.createElement("div");
      div.className = "block-text";
      div.innerHTML = renderMarkdown(block.text, { lazy: ctx.lazy });
      return div;
    }
    case "thinking": {
      return makeCollapsible(
        "block-thinking",
        `💭 思考 · ${block.thinking.length} 字`,
        () => {
          const body = document.createElement("div");
          body.className = "block-body block-body-md";
          body.innerHTML = renderMarkdown(block.thinking, { lazy: ctx.lazy });
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
      // issue #21：交互等待工具 → 默认展开的提问卡 / plan 卡（用户在被等着，
      // 折叠会误以为 LLM 还在输出）。畸形 input throw → 回退通用折叠卡。
      if (isInteractiveTool(block.name)) {
        try {
          const el = buildInteractiveCard(block.name, block.input, {
            lazy: ctx.lazy,
          });
          ctx.toolUseElements.set(block.id, el); // result 回填靶（同 buildToolUseCard）
          return el;
        } catch (e) {
          console.warn("interactive card fallback:", block.name, e);
        }
      }
      return buildToolUseCard(block, ctx);
    }
    case "tool_result": {
      return injectOrBuildToolResult(block, ctx);
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
 * tool_use 折叠条结构：
 *   <details class="block-collapsible block-tool-use">
 *     <summary>🔧 ToolName  input-summary</summary>
 *     <div class="block-body-wrap">
 *       <pre class="block-args">     ← lazy 渲染（首次展开时）
 *       <div class="block-tool-result-inline"> ← 由 injectOrBuildToolResult 追加
 *     </div>
 *   </details>
 */
function buildToolUseCard(
  block: Extract<ContentBlock, { type: "tool_use" }>,
  ctx: RenderContext,
): HTMLElement {
  const summary = summarizeInput(block.input);
  const d = document.createElement("details");
  d.className = "block-collapsible block-tool-use";

  const s = document.createElement("summary");
  s.className = "block-summary";
  s.textContent = `🔧 ${block.name}  ${summary}`;
  d.appendChild(s);

  const wrap = document.createElement("div");
  wrap.className = "block-body-wrap";
  d.appendChild(wrap);

  let argsRendered = false;
  d.addEventListener("toggle", () => {
    if (!d.open || argsRendered) return;
    let bodyEl: HTMLElement | null = null;
    // issue #14：Edit/Write/MultiEdit → 行级 diff 卡；任何异常 / 畸形 / 未知工具
    // 回退现有 prettyJson <pre>（双重 try/catch：这里 + buildDiffBody 内部）。
    if (isDiffTool(block.name)) {
      try {
        bodyEl = buildDiffBody(block.name, block.input);
      } catch {
        bodyEl = null;
      }
    }
    if (!bodyEl) {
      const pre = document.createElement("pre");
      pre.className = "block-body block-body-json block-args";
      pre.textContent = prettyJson(block.input);
      bodyEl = pre;
    }
    // args/diff 始终在 result 之前（injectOrBuildToolResult 把 result append 到 wrap 末尾）
    wrap.insertBefore(bodyEl, wrap.firstChild);
    // F54:远端会话 + 有 file_path 的工具(Read/Write/Edit/…)→ body 顶部插「在 SFTP 打开」可点
    // 链接(会话→文件跳转)。放 body(展开时)而非 summary——summary 会被 tool_result 注入重写。
    const fp = fileInputPath(block.input);
    if (ctx.origin && fp) {
      wrap.insertBefore(buildRemoteFileLink(ctx.origin, fp), wrap.firstChild);
    }
    argsRendered = true;
  });

  ctx.toolUseElements.set(block.id, d);
  return d;
}

/**
 * F54:从 tool_use 输入里取可 SFTP 定位的文件路径。Read/Write/Edit/MultiEdit 用 `file_path`,
 * NotebookEdit 用 `notebook_path`。**只认绝对 POSIX 路径**——相对路径 `parentPath` 会解析到错
 * 目录(定位落空),不给链接。否则 null。导出便于单测。
 */
export function fileInputPath(input: unknown): string | null {
  if (input && typeof input === "object") {
    const obj = input as Record<string, unknown>;
    const fp = obj.file_path ?? obj.notebook_path;
    if (typeof fp === "string" && fp.startsWith("/")) return fp;
  }
  return null;
}

/** F54:远端文件路径可点元素——点击 → 反查主机 cfg → 打开 SFTP 面板定位该文件。 */
function buildRemoteFileLink(origin: string, filePath: string): HTMLElement {
  const el = document.createElement("button");
  el.type = "button";
  el.className = "tool-file-link";
  el.textContent = `📂 在 SFTP 打开：${filePath}`;
  el.title = "在 SFTP 面板定位该远端文件（可预览/下载）";
  el.addEventListener("click", () => void openRemoteFileInSftp(origin, filePath));
  return el;
}

async function openRemoteFileInSftp(origin: string, filePath: string): Promise<void> {
  const cfg = await resolveRemoteConfigByOrigin(origin);
  if (!cfg) {
    showActionFailureToast("打开文件失败", `未找到远端主机配置：${origin}`);
    return;
  }
  openSftpPanel(cfg, filePath);
}

/**
 * tool_result 注入：找到对应 tool_use 折叠条 → 在 `.block-body-wrap` 末尾
 * append 一个 `.block-tool-result-inline` 区块 → 同步更新 summary 加首行预览
 * 与错误标记。返 null 让上层不再追加独立 element。
 *
 * 若找不到对应 tool_use（边界：result 先于 use 到达 / 跨 session 引用），
 * fallback 渲染独立折叠条。
 */
function injectOrBuildToolResult(
  block: Extract<ContentBlock, { type: "tool_result" }>,
  ctx: RenderContext,
): HTMLElement | null {
  const text = renderResultContent(block.content);
  const exitCode = block.is_error ? extractExitCode(text) : null;
  const preview = firstLinePreview(text, 60);
  const toolName = ctx.toolUseNames.get(block.tool_use_id) ?? "tool";
  const errTag = block.is_error
    ? exitCode !== null
      ? ` · exit ${exitCode}`
      : " · error"
    : "";

  const host = ctx.toolUseElements.get(block.tool_use_id);
  if (host) {
    const wrap = host.querySelector(".block-body-wrap");
    if (wrap) {
      // result 自己也是 details，默认收起，避免长输出占满
      let resultEl = wrap.querySelector(
        ".block-tool-result-inline",
      ) as HTMLDetailsElement | null;
      if (!resultEl) {
        resultEl = document.createElement("details");
        resultEl.className =
          "block-tool-result-inline block-collapsible block-nested";
        wrap.appendChild(resultEl);
      } else {
        resultEl.replaceChildren();
      }
      if (block.is_error) {
        resultEl.classList.add("block-error");
        host.classList.add("block-has-error");
      }

      const labelPrefix = block.is_error
        ? exitCode !== null
          ? `Error · exit ${exitCode}`
          : "Error"
        : "Output";
      const sizeHint = approximateSize(block.content);
      const summary = document.createElement("summary");
      summary.className = "block-summary";
      summary.textContent = preview
        ? `${labelPrefix} · ${preview}`
        : `${labelPrefix} · ${sizeHint}`;
      resultEl.appendChild(summary);

      // 渲染模式 toolbar + body (lazy build 首次展开时再实际产生 DOM)
      buildResultBody(resultEl, text, toolName);

      // 同步 tool_use summary：只在原 summary 末尾追加错误标记（不重复加预览，
      // 预览已经在 result 自己的 summary 上）。防止反复追加。
      const summaryEl = host.querySelector(
        ":scope > .block-summary",
      ) as HTMLElement | null;
      if (summaryEl) {
        const base = (summaryEl.textContent ?? "").replace(
          /\s+·\s+(exit\s+\d+|error)$/iu,
          "",
        );
        summaryEl.textContent = `${base}${errTag}`;
      }
      // issue #21：AskUserQuestion 提问卡 → 解析答案、高亮选中项（纯增强，失败无害）
      markInteractiveAnswer(host, text);
    }
    return null;
  }

  // fallback：tool_use 没找到 → 独立折叠条
  const cls = block.is_error
    ? "block-tool-result block-error"
    : "block-tool-result";
  const summaryText = preview
    ? `${toolName}${errTag} · ${preview}`
    : `${toolName}${errTag} · ${approximateSize(block.content)}`;
  const fallback = makeCollapsible(cls, summaryText, () => {
    const container = document.createElement("div");
    buildResultBody(container, text, toolName);
    return container;
  });
  // 标记 + 登记到 pending map，给切块场景的 reconcile 用
  fallback.setAttribute("data-tool-use-id", block.tool_use_id);
  if (ctx.pendingToolResults) {
    ctx.pendingToolResults.set(block.tool_use_id, { block, element: fallback });
  }
  return fallback;
}

/**
 * v2.3.1 (issue #1)：切块完成后调一次。扫所有 pending fallback tool_result，
 * 重试匹配现已渲染的 tool_use 折叠条 → 注入 + 删 fallback。
 *
 * 调用方（TabManager.onBatchEnd）：对每个 Tab 用自己的 ctx 调一次。
 *
 * 不匹配的（真 fallback：tool_use 永远不会来）留着，UI 仍然能看到独立卡。
 */
/**
 * F40b-D 审计:fallback 孤儿单元是**组内实体**(tool_result-only 渲染恒走 tool-group,
 * 单元被并进组 body)——timeline entry 的元素是组 root,不是单元本身。单元出 DOM 后
 * 若组壳已空(该 fallback 是组内唯一单元),连根摘壳并返回 root:root 才是需要
 * timeline.removeByElement 出账的对象,否则留下空组壳(「1 个工具调用」空 details)
 * 且账上挂着一个已离场元素。非空组壳照旧(summary 计数滞后属既有化妆性问题,留档)。
 * 导出仅为单测。
 */
export function removeEmptyToolGroupShell(host: HTMLElement | null): HTMLElement | null {
  if (!host) return null;
  const body = host.querySelector(":scope > .card-tool-group-body");
  if (!body || body.childElementCount > 0) return null;
  host.remove();
  return host;
}

export function reconcilePendingToolResults(ctx: RenderContext): HTMLElement[] {
  if (!ctx.pendingToolResults || ctx.pendingToolResults.size === 0) return [];
  const toDelete: string[] = [];
  // 返回值语义(F40b):**需要从 timeline 出账的元素**——即被连根摘除的空组壳 root。
  // fallback 单元本身从不是 timeline entry(见 removeEmptyToolGroupShell 注释)。
  const removed: HTMLElement[] = [];
  for (const [toolUseId, { block, element }] of ctx.pendingToolResults) {
    if (!ctx.toolUseElements.has(toolUseId)) continue; // 仍然没匹配，保留
    // 已有 host → 重新调注入（injectOrBuildToolResult 走"已 host"分支，返 null）
    const reInjected = injectOrBuildToolResult(block, ctx);
    if (reInjected === null) {
      // 注入成功 → 删除原 fallback;宿主组必须在 remove **之前**取(摘除后 closest 断链)
      const host = element.closest<HTMLElement>(".card-tool-group");
      element.remove();
      toDelete.push(toolUseId);
      const shell = removeEmptyToolGroupShell(host);
      if (shell) removed.push(shell);
    }
  }
  for (const id of toDelete) {
    ctx.pendingToolResults.delete(id);
  }
  return removed;
}

/**
 * 把 tool_result 文本渲染到 host 里，并附 [文本|MD] 切换 toolbar。
 *
 * 性能权衡：
 * - 默认只挂 toolbar + 空占位；首次 host 展开 (details open) 时才实际 build body
 *   （由调用方控制：tool_use 内嵌 result 是 details，外层 tool_use 展开后才被看到；
 *    fallback 的独立 result 也是 details；两者都触发 toggle）
 * - 大 output（> 200KB） 默认只渲染前 N 行 + [显示完整] 按钮，避免一次性
 *   塞几百 K 文本到 pre / marked 解析卡住主线程
 * - 切到 Markdown 后再切回 Text 时复用上次 build 的 pre（少一次重建）
 *
 * 偏好持久：per-tool-name 写 localStorage `cc-monitor.tool-render.<name>`。
 * Read / Grep 类阅读工具默认 MD，Bash 类命令默认 text。
 */
function buildResultBody(
  host: HTMLElement,
  text: string,
  toolName: string,
): void {
  const toolbar = document.createElement("div");
  toolbar.className = "block-result-toolbar";

  const btnText = document.createElement("button");
  btnText.type = "button";
  btnText.className = "block-result-mode is-active";
  btnText.textContent = "文本";
  btnText.title = "原始文本（Pre 格式）";

  const btnMd = document.createElement("button");
  btnMd.type = "button";
  btnMd.className = "block-result-mode";
  btnMd.textContent = "Markdown";
  btnMd.title = "Markdown 渲染（含 LaTeX / 代码高亮）";

  toolbar.append(btnText, btnMd);
  host.appendChild(toolbar);

  const bodyHost = document.createElement("div");
  bodyHost.className = "block-result-body-host";
  host.appendChild(bodyHost);

  let textBodyEl: HTMLElement | null = null;
  let mdBodyEl: HTMLElement | null = null;
  let currentMode: "text" | "md" =
    loadRenderModePreference(toolName) ?? defaultModeForTool(toolName);

  const renderMode = (mode: "text" | "md"): void => {
    if (currentMode === mode && bodyHost.firstChild) return;
    currentMode = mode;
    btnText.classList.toggle("is-active", mode === "text");
    btnMd.classList.toggle("is-active", mode === "md");

    bodyHost.replaceChildren();
    if (mode === "text") {
      if (!textBodyEl) textBodyEl = buildTextBody(text);
      bodyHost.appendChild(textBodyEl);
    } else {
      if (!mdBodyEl) mdBodyEl = buildMarkdownBody(text);
      bodyHost.appendChild(mdBodyEl);
    }
  };

  btnText.addEventListener("click", () => {
    saveRenderModePreference(toolName, "text");
    renderMode("text");
  });
  btnMd.addEventListener("click", () => {
    saveRenderModePreference(toolName, "md");
    renderMode("md");
  });

  // 初次：lazy 渲染——挂 toolbar 但 body 等 details open 才真正 render
  // host 可能是 <details> 也可能是 <div>（fallback 路径），都用 hostNeedsLazy 判断
  const detailsHost = host.closest("details");
  if (detailsHost && !detailsHost.open) {
    // details 未展开 → 等 toggle 时 render
    const onToggle = () => {
      if (!detailsHost.open) return;
      detailsHost.removeEventListener("toggle", onToggle);
      renderMode(currentMode);
    };
    detailsHost.addEventListener("toggle", onToggle);
  } else {
    renderMode(currentMode);
  }
}

/** 大 output 阈值（字节估算）—— 超过先只渲染前 N 行 */
const LARGE_TEXT_BYTES = 200_000;
const LARGE_TEXT_HEAD_LINES = 800;

function buildTextBody(text: string): HTMLElement {
  const pre = document.createElement("pre");
  pre.className = "block-body block-body-result";
  // 大 output 截断：避免一次性塞几百 K 到 DOM
  if (text.length > LARGE_TEXT_BYTES) {
    const head = text.split("\n").slice(0, LARGE_TEXT_HEAD_LINES).join("\n");
    pre.textContent = head;
    const wrap = document.createElement("div");
    wrap.className = "block-body-truncated-wrap";
    wrap.appendChild(pre);
    const expand = document.createElement("button");
    expand.type = "button";
    expand.className = "block-body-show-full";
    const sizeKb = (text.length / 1024).toFixed(0);
    expand.textContent = `显示完整内容 (${sizeKb} KB)`;
    expand.addEventListener(
      "click",
      () => {
        pre.textContent = text;
        expand.remove();
      },
      { once: true },
    );
    wrap.appendChild(expand);
    return wrap;
  }
  pre.textContent = text;
  return pre;
}

function buildMarkdownBody(text: string): HTMLElement {
  const div = document.createElement("div");
  div.className = "block-body block-body-result block-body-md";
  // Read / Grep 类工具输出带行号前缀（`<n>\t...` 或 `<n>:...`），
  // 行首不是 `#` 等 markdown token → marked 不识别。
  // MD 模式先 strip 这些前缀让结构暴露出来。
  const cleaned = stripLineNumberPrefix(text);
  // 大文本 markdown 渲染昂贵——同样做截断 + 显示完整按钮
  if (cleaned.length > LARGE_TEXT_BYTES) {
    const head = cleaned
      .split("\n")
      .slice(0, LARGE_TEXT_HEAD_LINES)
      .join("\n");
    div.innerHTML = renderMarkdown(head);
    const expand = document.createElement("button");
    expand.type = "button";
    expand.className = "block-body-show-full";
    const sizeKb = (cleaned.length / 1024).toFixed(0);
    expand.textContent = `渲染完整内容 (${sizeKb} KB)`;
    expand.addEventListener(
      "click",
      () => {
        div.innerHTML = renderMarkdown(cleaned);
      },
      { once: true },
    );
    const wrap = document.createElement("div");
    wrap.className = "block-body-truncated-wrap";
    wrap.append(div, expand);
    return wrap;
  }
  div.innerHTML = renderMarkdown(cleaned);
  return div;
}

/**
 * 启发式 strip 行号前缀，给 Markdown 渲染用。
 *
 * 支持两种典型格式：
 * - **Read tool**：每行 `<digits>\t<content>` （cat -n 风格）
 * - **Grep tool**：每行 `<digits>:<content>` 或 `<path>:<digits>:<content>`
 *
 * 判断逻辑：如果**绝大多数非空行**（≥ 80%）符合 `^\s*\d+[:\t]` 模式，
 * 视为带行号前缀，整体 strip；否则原样返回。
 *
 * 这样的代价：极少数 markdown 文本本身就是 "1: 标题" 这种列表格式的会被误 strip，
 * 但这种情况渲染效果差异不大（仍然能看），可接受。
 */
function stripLineNumberPrefix(text: string): string {
  const lines = text.split("\n");
  let nonEmpty = 0;
  let withPrefix = 0;
  const PREFIX = /^\s*\d+[:\t]/;
  // Grep "<path>:<n>:<content>" — path 含 `/` 或 `\` 或 `:`，先 strip path 段
  const GREP_PATH = /^[^\s:]+[/\\][^\s:]*?:\d+:/;
  for (const line of lines) {
    if (line.trim() === "") continue;
    nonEmpty += 1;
    if (PREFIX.test(line) || GREP_PATH.test(line)) withPrefix += 1;
  }
  if (nonEmpty < 3) return text; // 太短不判断
  if (withPrefix / nonEmpty < 0.8) return text;

  return lines
    .map((line) => {
      if (line.trim() === "") return line;
      // 优先 strip Grep 完整路径前缀（含或不含 path 段）
      const m1 = line.match(/^[^\s:]+[/\\][^\s:]*?:\d+:(.*)$/);
      if (m1) return m1[1] ?? "";
      const m2 = line.match(/^\s*\d+[:\t](.*)$/);
      if (m2) return m2[1] ?? "";
      return line;
    })
    .join("\n");
}

/** 偏好：哪些 tool 默认 MD 渲染（产生类 markdown 文本的工具）。F-MA：值在 agent-profile.mdTools。 */
function defaultModeForTool(toolName: string): "text" | "md" {
  return AGENT_PROFILE.mdTools.has(toolName) ? "md" : "text";
}

function loadRenderModePreference(toolName: string): "text" | "md" | null {
  const v = safeGet(LS_KEYS.toolRender(toolName));
  if (v === "text" || v === "md") return v;
  return null;
}

function saveRenderModePreference(toolName: string, mode: "text" | "md"): void {
  safeSet(LS_KEYS.toolRender(toolName), mode);
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

/**
 * 剥掉 Claude Code CLI 注入到 user message 里的 prompt 包装：
 * - `<task-notification>...</task-notification>` 后台命令完成通知
 * - `<system-reminder>...</system-reminder>` 各种系统级提醒
 * - `<local-command-caveat>...</local-command-caveat>` 本地命令免责声明
 * - `<local-command-stdout>...</local-command-stdout>` 本地命令（如 /compact）的 stdout
 * - `Continue from where you left off.` / `No response requested.` 样板单行
 * - `[Request interrupted by user...]` 用户 ESC 中断 / 拒绝工具调用时 CLI
 *   注入的 user message。**不是真用户输入**——v2.4.2 issue #2 修
 *   "用户 ESC 中断时 monitor 误以为是真敲键自动拉前" 时新增。
 *
 * 返回剥过后的文本（trim 过）。空字符串表示整条都是 noise，调用方应 skip。
 * 非空时下游 (parseSlashCommand / buildUserCard) 用这份剥过的文本渲染，
 * 这样 `/compact` 后面跟的 stdout 不会拖累 slash 卡片识别。
 */
function stripInternalNoise(text: string): string {
  return text
    .replace(/<task-notification>[\s\S]*?<\/task-notification>/g, "")
    .replace(/<system-reminder>[\s\S]*?<\/system-reminder>/g, "")
    .replace(/<local-command-caveat>[\s\S]*?<\/local-command-caveat>/g, "")
    .replace(/<local-command-stdout>[\s\S]*?<\/local-command-stdout>/g, "")
    .replace(/^continue from where you left off\.?$/gim, "")
    .replace(/^no response requested\.?$/gim, "")
    // v2.4.2 issue #2: `[Request interrupted by user]`（ESC 中断 assistant 流式生成）
    // 和 `[Request interrupted by user for tool use]`（拒绝工具调用）都不是真用户
    // 输入。剥掉让整条 skip → 既不渲染奇怪的"用户中断"卡片，也不触发自动拉前。
    //
    // 注：不用 `gim`——`m` flag 让 `^...$` 锚到每一行，会误吞合法 user 消息中
    // 偶然出现"以该模式开头的行"。CLI 实际把中断标记作为整条 user message 的唯一
    // 文本写入；剥过其他 noise 后，整文本若 trim 完正好是该模式，就归零。
    .replace(/^\[Request interrupted by user[^\]]*\]\s*$/, "")
    .trim();
}

/**
 * 识别 assistant 自动应答（claude 在收到 task-notification 之类时回的 `<synthetic>`
 * 包裹的"无内容应答"），不是真实对话内容。
 */
function isSyntheticReply(text: string): boolean {
  const t = text.trim();
  if (!t.startsWith("<synthetic>")) return false;
  // 简短 synthetic（如 "No response requested."）一律视为内部应答
  return t.length < 300;
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
  t.textContent = formatTimestampShort(timestamp);
  h.appendChild(t);
  if (model) {
    const m = document.createElement("span");
    m.className = "model";
    m.textContent = model;
    h.appendChild(m);
  }
  return h;
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
