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
 * - user 记录是谁说的（人 · 斜杠命令 · `!` 输入/输出 · 压缩摘要 · 派给子 agent 的活 · 系统注入 · agent 来话 · 后台通知……，
 *   INVARIANT § 20）：判定只在后端，随记录成品带来（`userText.speaker` ＋ 要显示的 `userText.text`），这里只按它画。
 * - `pendingToolResults`：tool_result 先于 tool_use 到达时先 fallback 渲染，batch 末
 *   `reconcilePendingToolResults` 重新匹配注入。
 */
import { makeYieldToMain } from "../yield-to-main";
import { renderMarkdown, renderPlainText } from "../render";
import { buildSlashCommandCard } from "./slash";
import { buildBashInputCard, buildBashOutputCard } from "./bash";
import { buildCompactSummaryCard } from "./compact";
import { buildAgentBar, buildCoordinatorBar, buildInterruptLine, buildNoticeLine, buildPeerBar } from "./speaker-bar";
import { drawsCard } from "../speaker";
import { buildAgentCard } from "./subagent";
import { buildDiffBody } from "./diff";
import { buildInteractiveCard, settleInteractive } from "./interactive";
import type { Pasted } from "../generated/Pasted";
import type { ToolCard } from "../generated/ToolCard";
import type { ChildRunTag } from "../generated/ChildRunTag";
import type { ToolStep } from "../generated/ToolStep";
import type { StepResult } from "../generated/StepResult";
import { buildStepLine, buildThinkingLine, durBetween, settleStepLine } from "./step-line";
import { buildApiErrorCard, buildApiRetryCard } from "./api-error";
import { LS_KEYS, safeGet, safeSet } from "../local-storage";
import { firstLineOf, formatTimestampShort, jsonPrefix } from "../format";
import { openFileWindow } from "../file-window";
import { resolveRemoteConfigByOrigin } from "../remote-config";
import { toast } from "../kit/toast";
import { isRemoteOrigin, type Origin } from "../ipc/origin";

/**
 * 〔判定只在后端〕记过的一次 tool_use：工具名（标注用、存偏好用）＋ 后端给的卡型（没有 ＝ 普通工具卡）。
 * 界面**不认工具名**：哪个工具画成哪种卡由那台后端的适配层判（`agents/<名>/cards.rs`），随 assistant 记录的 `toolCards` 带来。
 */
export interface ToolUseSeen {
  name: string;
  card: ToolCard | undefined;
  /** 后端给的一行人话（`toolSteps`；老后端没有）与发出那条记录的时刻（结果到了相减成耗时）。 */
  step?: ToolStep;
  at?: string;
}

/** 一条记录带来的、过程那一行要的成品：assistant 的 `toolSteps` · user 的 `toolResults` · 记录时刻。 */
interface StepFacts {
  steps: Readonly<Record<string, ToolStep>>;
  results: Readonly<Record<string, StepResult>>;
  at: string;
}
const NO_FACTS: StepFacts = Object.freeze({ steps: Object.freeze({}), results: Object.freeze({}), at: "" });
function stepFactsOf(rec: JsonlRecord): StepFacts {
  if (rec.type === "assistant") return { steps: rec.toolSteps ?? NO_FACTS.steps, results: NO_FACTS.results, at: rec.timestamp };
  if (rec.type === "user") return { steps: NO_FACTS.steps, results: rec.toolResults ?? NO_FACTS.results, at: rec.timestamp };
  return NO_FACTS;
}

/** 一条记录带来的卡型表（`tool_use.id` → 卡型）；只有 assistant 记录带，别的一律空。 */
type ToolCards = Readonly<Record<string, ToolCard>>;
const NO_CARDS: ToolCards = Object.freeze({});
function toolCardsOf(rec: JsonlRecord): ToolCards {
  return rec.type === "assistant" ? rec.toolCards ?? NO_CARDS : NO_CARDS;
}
/** 派出子运行的那几次调用的通用标签（后端随记录成品带出的 `childRuns`）。 */
type ChildRuns = Readonly<Record<string, ChildRunTag>>;
const NO_RUNS: ChildRuns = Object.freeze({});
function childRunsOf(rec: JsonlRecord): ChildRuns {
  return rec.type === "assistant" ? rec.childRuns ?? NO_RUNS : NO_RUNS;
}

// === Rust 端 JsonlRecord 的 TS 镜像 ===
//
// **C04c 起不再是「镜像」了：`JsonlRecord` / `ApiMessage` / `Usage` 三个都由 `ts-rs`
// 从 `src/frontend/shell/src/messages.rs` 生成**——那个 enum **就是**线定义
// （wire == `serde_json::to_string(JsonlRecord)`），所以它才是唯一的源。
//
// 手抄版被删掉时暴露的三处漂移（都是**手写版更宽或更窄**，无声地）：
// ① variant 数 **8 vs 12**：手抄版缺 `permission-mode` / `last-prompt` /
//    `file-history-snapshot` / `Unknown`；
// ② 手抄版给 `queue-operation` 声称了一个 Rust 根本没有的 `timestamp?: string`
//    ——线上永远没有它，读到的恒为 `undefined`；
// ③ 手抄的 `Usage` 把 `cache_creation_input_tokens` / `cache_read_input_tokens` 标成
//    optional，而 Rust 侧只有 `#[serde(default)]`、**没有** `skip_serializing_if`
//    ⇒ 线上恒有。
//
// **`ContentBlock` 刻意留手写**（就在下面）：Rust 的 `ApiMessage.content` 是
// `serde_json::Value`，压根没引用 Rust 那个 `ContentBlock` ⇒ 它在线上**不可达**。
// TS 这个是对 `content: unknown` 的**解释模型**，属于前端侧的意图类型，
// 同账本第 4 行「IR 是前端的意图模型，别把它拖过边界」。生成它就是为假想消费者建抽象。
import type { ApiMessage } from "../generated/ApiMessage";
import type { JsonlRecord } from "../generated/JsonlRecord";
import type { Usage } from "../generated/Usage";
import { copyText } from "../copy-table";

// 本文件内部也用这些名字，所以 import + re-export 都要有：
// **只写 `export type { … } from` 不会把名字带进本地作用域**（C02 栽过两次，C04c 又栽一次）。
export type { ApiMessage, JsonlRecord, Usage };

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


/**
 * 渲染上下文，沿调用链向下传递。
 *
 * 字段命名保持稳定 —— 跨模块（cards/subagent、tabs）依赖。
 */
export interface RenderContext {
  /** 父记录路径：派出子运行的那张卡按它（＋ 工具调用 id）读那个子运行的记录 */
  parentPath: string;
  /**
   * Batch9-F29：会话来源（本机 = `LOCAL_ORIGIN`；其余 = 远端机器 label）。
   * 上一版是「`null`/缺省 = 本地」—— 两种「没说」都被当成本机；
   * 现在**必填**：每个造渲染上下文的地方都得说出是哪台机器。
   */
  origin: Origin;
  /**
   * tool_use_id → 那一次 tool_use 的工具名与卡型。tool_use 出现在 assistant 消息，tool_result
   * 出现在下一条 user 消息，跨消息不能就地反查；TabManager（或 subagent 嵌套
   * 渲染）持有这张 Map 跨 renderMessage 调用累积。renderBlock 在 tool_use
   * 时写入，在 tool_result 时读取来标注工具名、挑结果默认怎么画（卡型是后端随记录成品带出的 `toolCards`）。
   */
  toolUseNames: Map<string, ToolUseSeen>;
  /**
   * tool_use_id → tool_use 折叠条 DOM 引用。tool_result 到达时把结果直接
   * append 到对应 tool_use 内部（不创建独立折叠条），实现"展开命令同时
   * 看到参数 + 输出"的合并 UX。
   */
  toolUseElements: Map<string, HTMLElement>;
  /** 派出子运行的那几张卡（父侧工具调用 id → 卡）；不需要按运行表标卡的调用方不给。 */
  runCards?: Map<string, HTMLElement>;
  /** 运行 id ⇒ 运行表里的标签（agent 来话的事件条起名用；宿主不给 ⇒ 用来话自带的名字）。 */
  runLabelOf?: (run: string) => string | undefined;
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
      // 谁说的由后端判好（`userText.speaker`）；不建卡的那几种（系统注入 · agent 来话 · 后台通知 · 中断标记……）
      // 仍在 timeline 里占链节点（同 attachment），只是不建卡。
      const said = rec.userText;
      const speaker = said.speaker;
      if (!drawsCard(speaker.kind)) return { kind: "skip" };
      switch (speaker.kind) {
        case "slashCommand":
          return { kind: "card", element: buildSlashCommandCard(speaker, rec.timestamp, formatTimestampShort) };
        case "bashInput":
          return { kind: "card", element: buildBashInputCard(speaker, rec.timestamp, formatTimestampShort) };
        case "bashOutput":
          return { kind: "card", element: buildBashOutputCard(speaker, rec.timestamp, formatTimestampShort) };
        case "compactSummary":
          if (!said.text) return { kind: "skip" };
          return {
            kind: "card",
            element: buildCompactSummaryCard(said.text, rec.timestamp, formatTimestampShort),
          };
        case "toolResult":
          break;
        case "agentMessage":
          return { kind: "card", element: buildAgentBar(speaker, rec.timestamp, speaker.from ? ctx.runLabelOf?.(speaker.from) : undefined) };
        case "peerSession":
          return { kind: "card", element: buildPeerBar(speaker, rec.timestamp) };
        case "coordinator":
          return { kind: "card", element: buildCoordinatorBar(speaker, rec.timestamp) };
        case "taskNotification":
          return { kind: "card", element: buildNoticeLine(speaker, rec.timestamp) };
        case "interrupt":
          return { kind: "card", element: buildInterruptLine(rec.timestamp) };
        default:
          // 人说的话 · 派给子 agent 的活：用户气泡；没有正文（只有图片之类）不建卡。
          if (!said.text) return { kind: "skip" };
          return { kind: "card", element: buildUserCard(rec, said.text, said.pasted) };
      }

      // 工具结果回灌：注入到对应 tool_use 折叠条内部，返回 null；
      // 只有找不到匹配 tool_use 的 fallback 才产生独立 element。
      const blocks = normalizeBlocks(rec.message.content).filter(
        (b) => b.type === "tool_result",
      );
      if (blocks.length === 0) return { kind: "skip" };
      const facts = stepFactsOf(rec);
      const units = blocks
        .map((b) => renderBlock(b, ctx, NO_CARDS, NO_RUNS, facts))
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
            reason: rec.apiReason,
            text: extractText(rec.message.content).trim(),
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
      const cards = toolCardsOf(rec);
      const hasInteractive = meaningful.some(
        (b) => b.type === "tool_use" && cards[b.id] === "interactive",
      );
      if (hasText || hasInteractive) {
        return {
          kind: "card",
          element: buildAssistantCard(rec, meaningful, ctx),
        };
      }
      // 全是 thinking / tool_use / tool_result → 工具组成员
      const facts = stepFactsOf(rec);
      const units = meaningful
        .map((b) => renderBlock(b, ctx, cards, childRunsOf(rec), facts))
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
            reason: rec.apiReason,
            retryAttempt: rec.retryAttempt,
            maxRetries: rec.maxRetries,
          }),
        };
      }
      return { kind: "skip" };
    case "ai-title":
    case "custom-title":
      return { kind: "skip" };
    // ★★ P0c：`remove` 那一支是**用户打断时说的那句话在 jsonl 里唯一的存在**。
    //
    // 它没有 `user` 记录、没有 `uuid`、没有 `parentUuid` —— 只有这条 `queue-operation`。
    // 不在这里建卡，用户说的话就整条消失（本会话实测丢了 16 条，全是打断时说的）。
    //
    // ⚠ 走到这里的**只有** `remove` 且后端判为人说的那一格（`routeMetaAndBranch` 把
    // `enqueue`/`dequeue` 与别的来源都判 `"consumed"` 了）。
    case "queue-operation": {
      const text = rec.userText?.speaker.kind === "human" ? rec.userText.text : "";
      if (!text) return { kind: "skip" };
      return { kind: "card", element: buildQueuedUserCard(text, rec.timestamp) };
    }
    default:
      return { kind: "skip" };
  }
}

/** P0c：排队消息的用户卡。
 *
 *  与普通用户卡**刻意长得不一样**：多一个「排队」标记。
 *  理由不是装饰 —— 这条消息在会话链上**没有位置**（无 uuid/parentUuid），
 *  它与前后消息的先后只由 `seq` 保证。让读的人知道「这条是插进来的」，
 *  比让它伪装成一条普通用户消息诚实。 */
function buildQueuedUserCard(text: string, timestamp: string | null): HTMLElement {
  const card = document.createElement("div");
  card.className = "card card-user card-user-queued";
  card.appendChild(cardHeader(copyText("cards.queuedUser.title"), timestamp ?? ""));

  const body = document.createElement("div");
  body.className = "card-body";
  body.innerHTML = renderPlainText(text);
  card.appendChild(body);
  return card;
}

/** 工具组外层折叠卡 —— TabManager 维护一组，连续的 tool-group 都追加进来 */
export interface ToolGroup {
  root: HTMLDetailsElement;
  body: HTMLElement;
  summary: HTMLElement;
  count: number;
  startedAt: string;
}

/** 外层折叠卡 → 它的工具组（结果后到、注入进组里某一条时，要回头改那一组收着时那一行）。 */
const groupOfRoot = new WeakMap<HTMLElement, ToolGroup>();

/** `el` 所在的那个工具组收着时那一行重写一遍（`el` 不在任何组里 ⇒ 不动）。 */
function refreshGroupAround(el: HTMLElement | null): void {
  const root = el?.closest<HTMLElement>(".card-tool-group");
  const group = root ? groupOfRoot.get(root) : undefined;
  if (group) updateToolGroupSummary(group);
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
  groupOfRoot.set(root, group);
  updateToolGroupSummary(group);
  return group;
}

export function addToToolGroup(group: ToolGroup, units: HTMLElement[]): void {
  for (const u of units) group.body.appendChild(u);
  group.count += units.length;
  updateToolGroupSummary(group);
}

/** 收着时那一行：个数 · 起始时刻；里面有失败的、有子 agent 就说出来（收着也看得见）。 */
function updateToolGroupSummary(group: ToolGroup): void {
  const count = group.count;
  const since = formatTimestampShort(group.startedAt);
  const failed = group.body.querySelectorAll(":scope > .block-has-error, :scope > .block-tool-result.block-error").length;
  const agents = group.body.querySelectorAll(":scope > .block-agent").length;
  group.summary.textContent =
    failed > 0 && agents > 0
      ? copyText("cards.toolGroup.summaryBoth", { count, failed, agents, since })
      : failed > 0
        ? copyText("cards.toolGroup.summaryFailed", { count, failed, since })
        : agents > 0
          ? copyText("cards.toolGroup.summaryAgents", { count, agents, since })
          : copyText("cards.toolGroup.summary", { count, since });
}

/** 粘贴块超过这么多行就折成一行（「谁说的」稿 A · 10-02 定 12）。 */
export const PASTE_FOLD_LINES = 12;

/**
 * 人说的话（含粘贴进来的块）：粘贴块只露正文（两头的标记不露）；超过 [`PASTE_FOLD_LINES`] 行折成一行「粘贴的内容 · N 行」，点开就地展开。
 * 块的边界、正文那一截与行数都是后端的 `userText.pasted`（UTF-16 下标），界面不认标记的写法。
 */
function fillSaid(body: HTMLElement, text: string, pasted: readonly Pasted[] | undefined): void {
  if (!pasted || pasted.length === 0) {
    body.innerHTML = renderPlainText(text);
    return;
  }
  const plain = (s: string): void => {
    if (!s) return;
    const span = document.createElement("span");
    span.innerHTML = renderPlainText(s);
    body.appendChild(span);
  };
  let at = 0;
  let afterFold = false;
  for (const p of pasted) {
    const fold = p.lines > PASTE_FOLD_LINES;
    // 折起来的那一行自己占一行：紧挨着它的那一个换行不再另起空行。
    let before = text.slice(at, p.start);
    if (afterFold) before = before.replace(/^\r?\n/, "");
    if (fold) before = before.replace(/\r?\n$/, "");
    plain(before);
    afterFold = fold;
    const inner = text.slice(p.bodyStart, p.bodyEnd).replace(/^[\r\n]+|[\r\n]+$/g, "");
    if (fold) {
      const d = document.createElement("details");
      d.className = "paste-fold";
      const s = document.createElement("summary");
      s.textContent = copyText("speaker.paste.fold", { n: p.lines });
      const b = document.createElement("div");
      b.className = "paste-body";
      b.innerHTML = renderPlainText(inner);
      d.append(s, b);
      body.appendChild(d);
    } else plain(inner);
    at = p.end;
  }
  plain(afterFold ? text.slice(at).replace(/^\r?\n/, "") : text.slice(at));
}

function buildUserCard(
  rec: Extract<JsonlRecord, { type: "user" }>,
  text: string,
  pasted?: readonly Pasted[],
): HTMLElement {
  const card = document.createElement("div");
  card.className = "card card-user";
  card.appendChild(cardHeader(copyText("cards.user.title"), rec.timestamp));

  const body = document.createElement("div");
  body.className = "card-body";
  fillSaid(body, text, pasted);
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
  // Phase G 审计修：Codex 会话的 jsonl 是 `rollout-*.jsonl`（同 monitor `adapter::kind_of_record_name` / `codex_sid_from_rollout`〔散文墓碑〕
  // 的路径判据）；记录本身不带 agent kind，故据会话文件名判 agent 给对的卡头——否则 Codex 的每条文本回复
  // 都错标成 "Claude"。非 Codex（含 live Claude 会话、子 agent）恒 "Claude"（rollout- 前缀是 Codex 独有）。
  const agent = /(^|\/)rollout-[^/]*\.jsonl$/.test(ctx.parentPath) ? "Codex" : "Claude";
  card.appendChild(cardHeader(agent, rec.timestamp, rec.message.model));

  const body = document.createElement("div");
  body.className = "card-body";
  const cards = toolCardsOf(rec);
  const facts = stepFactsOf(rec);
  for (const block of meaningful) {
    const el = renderBlock(block, ctx, cards, childRunsOf(rec), facts);
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
  ctx: RenderContext,
  cards: ToolCards,
  runs: ChildRuns = NO_RUNS,
  facts: StepFacts = NO_FACTS,
): HTMLElement | null {
  switch (block.type) {
    case "text":
      return buildTextBlock(block.text, ctx.lazy);
    case "thinking": {
      // 思考是一步（§5.2.3）：「思考」＋ 第一行斜体预览，点开看全文。
      return makeCollapsible(
        "block-thinking",
        buildThinkingLine(copyText("cards.thinking.title"), firstLineOf(block.thinking.trim(), 160).line),
        () => {
          const body = document.createElement("div");
          body.className = "block-body block-body-md";
          // 展开那一刻才建（用户点开 = 它就在眼前）⇒ 一律急路。原来沿用建卡时的 `ctx.lazy`：
          // 批期建的卡早被 `enhanceCard` 标过 `enhanced`，之后才长出来的这块 body 里的占位（代码块 / 公式）永远没人补。
          body.innerHTML = renderMarkdown(block.thinking);
          return body;
        },
      );
    }
    case "tool_use": {
      // 记下 id → 名字与卡型，给下一条消息的 tool_result 反查用
      const card = cards[block.id];
      const step = facts.steps[block.id];
      ctx.toolUseNames.set(block.id, { name: block.name, card, step, at: facts.at || undefined });

      // 卡型是那台后端判的（`toolCards`）；派出子运行的那次调用 → 折叠卡，展开是那个子运行的时间线（按运行读）
      if (card === "agent") {
        const runCard = buildAgentCard(block.id, block.name, runs[block.id], ctx, renderMessage);
        ctx.runCards?.set(block.id, runCard);
        return runCard;
      }
      // issue #21：交互等待工具 → 默认展开的提问卡 / plan 卡（用户在被等着，
      // 折叠会误以为 LLM 还在输出）。畸形 input throw → 回退通用折叠卡。
      if (card === "interactive") {
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
      return buildToolUseCard(block, ctx, card, step);
    }
    case "tool_result": {
      return injectOrBuildToolResult(block, ctx, facts);
    }
    default: {
      // 未知 block.type（如 server_tool_use / web_search_tool_result 等扩展类型）
      // 不抛异常，给出可识别占位以便诊断。
      const unknownType = (block as { type?: unknown }).type;
      console.warn("renderBlock: unknown block type", unknownType, block);
      const placeholder = document.createElement("div");
      placeholder.className = "block-unknown";
      placeholder.textContent = copyText("cards.renderBlock.unknownBlock", { type: String(unknownType) });
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
  card: ToolCard | undefined,
  step?: ToolStep,
): HTMLElement {
  const command = card === "command" ? commandOf(block.input) : null;
  const summary = command ? commandSummary(command.command) : summarizeInput(block.input);
  const d = document.createElement("details");
  d.className = "block-collapsible block-tool-use";

  // 一步一行（§5.2.3）：主参数与说明是后端给的（`toolSteps`）；没有那一格（老后端）⇒ 工具名 ＋ 入参一句兜底。
  const s = document.createElement("summary");
  s.className = "block-summary block-step";
  s.appendChild(buildStepLine(block.name, step, summary));
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
    // 写类工具（后端判的卡型 `diff`）→ 行级 diff 卡。
    if (card === "diff") {
      try {
        bodyEl = buildDiffBody(block.name, block.input);
      } catch {
        bodyEl = null;
      }
    }
    // 命令卡（后端判的卡型 `command`）→ 命令本身，说明作一行注；形状不对就照常回退 JSON。
    if (command) {
      const pre = document.createElement("pre");
      pre.className = "block-body block-args";
      pre.textContent = command.description
        ? `${copyText("cards.command.note", { text: command.description })}\n${command.command}`
        : command.command;
      bodyEl = pre;
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
    if (isRemoteOrigin(ctx.origin) && fp) {
      wrap.insertBefore(buildRemoteFileLink(ctx.origin, fp), wrap.firstChild);
    }
    argsRendered = true;
  });

  ctx.toolUseElements.set(block.id, d);
  return d;
}

/**
 * 命令卡（卡型 `command`）的入参：`command` 是那一行命令，`description` 是可缺的一句说明。形状不是这样 ⇒ `null`（照普通卡画）。
 */
function commandOf(input: unknown): { command: string; description: string | null } | null {
  if (!input || typeof input !== "object" || Array.isArray(input)) return null;
  const o = input as Record<string, unknown>;
  if (typeof o.command !== "string" || o.command.length === 0) return null;
  const description = typeof o.description === "string" && o.description.trim() ? o.description.trim() : null;
  return { command: o.command, description };
}

/** 命令卡收着时那一行：命令的第一行（多行或太长 ⇒ 截到 60 字加省略号）。 */
function commandSummary(command: string): string {
  const { line } = firstLineOf(command, 60);
  return command.trim() === line ? line : copyText("cards.truncate.ellipsis", { text: line });
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

/** F54:远端文件路径可点元素——点击 → 反查主机 cfg → 在文件窗口里定位该文件(原先是老 SFTP 面板)。 */
function buildRemoteFileLink(origin: string, filePath: string): HTMLElement {
  const el = document.createElement("button");
  el.type = "button";
  el.className = "tool-file-link";
  el.textContent = copyText("cards.remoteFile.open", { path: filePath });
  el.title = copyText("cards.remoteFile.openHint");
  el.addEventListener("click", () => void openRemoteFileInSftp(origin, filePath));
  return el;
}

async function openRemoteFileInSftp(origin: string, filePath: string): Promise<void> {
  const cfg = await resolveRemoteConfigByOrigin(origin);
  if (!cfg) {
    toast(copyText("cards.remoteFile.openFailed"), copyText("cards.remoteFile.noMachine", { machine: origin }));
    return;
  }
  void openFileWindow(cfg, { revealFile: filePath });
}

/**
 * 秤 6（表第 6 行，验 `§2.8` 与 `§5.5`）的**计数器**。
 *
 * 「`buildResultBody` 闭包持有的文本总量 —— §2.8 的 7 MB 是按均值
 * 推的，不是实测。**怎么知道**：heap snapshot 按 retainer 找；或在 `cards/index.ts:570`
 * 累加 `text.length`」。这里就是那个「累加 `text.length`」。
 *
 * # 它是什么口径（写清楚，否则下一个人会把它读成别的东西）
 *
 * - **单位是 UTF-16 码元**（`String.prototype.length`），不是 UTF-8 字节。
 *   V8 里非 Latin-1 字符串每码元 2 字节 ⇒ 乘 2 才是堆上那一份的量级（§2.8 的
 *   「7 MB ⇒ UTF-16 下 ~14 MB」就是这么换的）。
 * - **它是累计流量，不是瞬时驻留**：同一条 result 被重渲一次（`replaceChildren`
 *   那一支）就再记一次，而旧闭包此刻已经可回收。⇒ 它是**驻留量的上界**。
 * - **它不是进程 RSS**，也不含 DOM 节点、marked/katex 中间产物、`pendingToolResults`
 *   里那份 `block`。它只答一句话：「我们自己的闭包里经手了多少文本」。
 *
 * # 为什么 `produced` 与 `captured` 要分开记
 *
 * `produced` 记在 `renderResultContent` 的出口（文本被造出来的那一刻），
 * `captured` 记在 `buildResultBody` 的入口（文本被闭包捕获的那一刻）。
 * 两者在主路上一一对应，但**有两处会岔开**，而那两处正是「留没留存」的判据：
 * - `wrap` 找不到那一支 ⇒ 造了文本但没人捕获（`produced` 涨、`captured` 不涨）；
 * - fallback 那一支的 `makeCollapsible` 工厂**没跑也捕获**（闭包参数里就有 `text`）
 *   ⇒ `captured` 此刻不涨，但文本**已经被留住了**。
 * ⇒ 只看 `captured` 会低估留存，只看 `produced` 会高估。两个都记，判词才站得住。
 *
 * 纯计数，零 DOM 副作用（硬约束）。生产里也一直在加，成本是
 * 每条 tool_result 两次整数加法。
 */
export interface ResultTextLedger {
  /** `renderResultContent` 出口计数 */
  produced: number;
  /** Σ `text.length`（UTF-16 码元），出口侧 */
  producedUnits: number;
  /** `buildResultBody` 入口计数（= 建了几个持有 `text` 的闭包） */
  captured: number;
  /** Σ `text.length`（UTF-16 码元），闭包侧 */
  capturedUnits: number;
  /** 单条最长的 `text.length` */
  maxUnits: number;
}

export const resultTextLedger: ResultTextLedger = {
  produced: 0,
  producedUnits: 0,
  captured: 0,
  capturedUnits: 0,
  maxUnits: 0,
};

/** 判据侧用：每格开跑前归零。生产不调。 */
export function resetResultTextLedger(): void {
  resultTextLedger.produced = 0;
  resultTextLedger.producedUnits = 0;
  resultTextLedger.captured = 0;
  resultTextLedger.capturedUnits = 0;
  resultTextLedger.maxUnits = 0;
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
  facts: StepFacts = NO_FACTS,
): HTMLElement | null {
  const text = renderResultContent(block.content);
  // 秤 6：点名的那一处累加。纯计数。
  resultTextLedger.produced += 1;
  resultTextLedger.producedUnits += text.length;
  if (text.length > resultTextLedger.maxUnits) {
    resultTextLedger.maxUnits = text.length;
  }
  const exitCode = block.is_error ? extractExitCode(text) : null;
  const preview = firstLinePreview(text, 60);
  const seen = ctx.toolUseNames.get(block.tool_use_id);
  const toolName = seen?.name ?? "tool";
  // 结果默认怎么画按后端给的卡型（`md`），不按工具名自己判。
  const mdByDefault = seen?.card === "md";
  const errTag = block.is_error
    ? exitCode !== null
      ? ` · exit ${exitCode}`
      : " · error"
    : "";

  const host = ctx.toolUseElements.get(block.tool_use_id);
  // 提问 / 计划答了之后（B7）：后端读出了答了什么 ⇒ 卡上写结果，不印 Claude Code 的英文原句。
  const answered = facts.results[block.tool_use_id];
  if (host && answered && (host.classList.contains("block-ask") || host.classList.contains("block-plan"))) {
    settleInteractive(host, answered);
    return null;
  }
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
        refreshGroupAround(host); // 收着的工具组那一行要说出「1 个失败」
      }

      const labelPrefix = block.is_error
        ? exitCode !== null
          ? `Error · exit ${exitCode}`
          : "Error"
        : "Output";
      const summary = document.createElement("summary");
      summary.className = "block-summary";
      // approximateSize 对 617 KB 的 content 整份 JSON.stringify
      //（O(len) 时间 + 一份等长字符串分配），而 preview 非空时这个结果 100% 用不上。
      // 挪进三元的 else 分支 ⇒ 惰性求值。显示结果一字不变。
      // ⚠ 同节还提了「数组 content 改成累加 text.length」——那个会改动显示出来的数字
      //（JSON 括号/引号也计在 `N chars` 里），不是零语义风险，**本次没做**。
      summary.textContent = preview
        ? `${labelPrefix} · ${preview}`
        : `${labelPrefix} · ${approximateSize(block.content)}`;
      resultEl.appendChild(summary);

      // 渲染模式 toolbar + body (lazy build 首次展开时再实际产生 DOM)
      buildResultBody(resultEl, text, toolName, mdByDefault);

      // 同步那一步的一行：状态图标与右侧小字（后端给的结果一句 ＋ 两条记录的时刻相减）。再来一次结果就再改一次。
      const line = host.querySelector<HTMLElement>(":scope > .block-summary > .step-line");
      if (line) settleStepLine(line, seen?.step, facts.results[block.tool_use_id], block.is_error === true, durBetween(seen?.at, facts.at));
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
    buildResultBody(container, text, toolName, mdByDefault);
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
      // fallback 身上的落点标记（`render-stream-record.ts::markMemberUuids` 记的）跟着搬到注入出来的结果区块上
      const member = element.dataset.memberUuid;
      if (member) {
        const inline = ctx.toolUseElements.get(toolUseId)?.querySelector<HTMLElement>(".block-tool-result-inline");
        if (inline) inline.dataset.memberUuid = member;
      }
      // 注入成功 → 删除原 fallback;宿主组必须在 remove **之前**取(摘除后 closest 断链)
      const host = element.closest<HTMLElement>(".card-tool-group");
      element.remove();
      toDelete.push(toolUseId);
      const shell = removeEmptyToolGroupShell(host);
      if (shell) removed.push(shell);
      else refreshGroupAround(host); // 那一条（可能是失败的）搬走了，原来那一组收着时那一行重算
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
  mdByDefault: boolean,
): void {
  // 秤 6（表第 6 行）：本函数下面建的每个闭包（`renderMode` /
  // 两个 click 监听 / `onToggle`）都捕获 `text`，而它们经 DOM 监听器被卡片长期持有。
  // 纯计数，口径见 `resultTextLedger` 的头注。
  resultTextLedger.captured += 1;
  resultTextLedger.capturedUnits += text.length;

  const toolbar = document.createElement("div");
  toolbar.className = "block-result-toolbar";

  const btnText = document.createElement("button");
  btnText.type = "button";
  btnText.className = "block-result-mode is-active";
  btnText.textContent = copyText("cards.result.text");
  btnText.title = copyText("cards.result.textHint");

  const btnMd = document.createElement("button");
  btnMd.type = "button";
  btnMd.className = "block-result-mode";
  btnMd.textContent = "Markdown";
  btnMd.title = copyText("cards.result.markdownHint");

  toolbar.append(btnText, btnMd);
  host.appendChild(toolbar);

  const bodyHost = document.createElement("div");
  bodyHost.className = "block-result-body-host";
  host.appendChild(bodyHost);

  let textBodyEl: HTMLElement | null = null;
  let mdBodyEl: HTMLElement | null = null;
  let currentMode: "text" | "md" =
    loadRenderModePreference(toolName) ?? (mdByDefault ? "md" : "text");

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

/**
 * 〔文本布局补审查出的性能缺陷〕回复正文超过它 ⇒ 建卡时只渲染前一截、下面一颗「显示全部」。
 * marked 同步排整段正文，617 KB 一条 ≈ 8 s 卡住主线程（读数）；
 * 20 000 字 ≈ 真机正文窗口 p99 的十几倍（p99 1 594 字），常态回复碰不到它。点了之后余下的按同样大小分片、一片一跳地渲染。
 */
export const LONG_REPLY_HEAD_CHARS = 20_000;

/**
 * 把 markdown 切成若干片，每片不超过 `max` 字（单个块超长就单独一片）：只在**围栏代码块之外的空行**上切，
 * 代码块不会被劈开。跨空行的列表 / 表格被切开时，后一片的编号从头起 —— 截断显示的代价，认。
 */
export function markdownPieces(md: string, max: number): string[] {
  const out: string[] = [];
  let cur: string[] = [];
  let curLen = 0;
  let inCode = false;
  let block: string[] = [];
  const add = (text: string): void => {
    if (curLen > 0 && curLen + text.length + 1 > max) {
      out.push(cur.join("\n"));
      cur = [];
      curLen = 0;
    }
    cur.push(text);
    curLen += text.length + 1;
  };
  const flushBlock = (): void => {
    if (block.length === 0) return;
    const text = block.join("\n");
    if (text.length <= max) add(text);
    else for (const part of splitOversizeBlock(block, max)) add(part);
    block = [];
  };
  for (const line of md.split("\n")) {
    if (/^\s*(\x60{3}|~{3})/.test(line)) inCode = !inCode;
    block.push(line);
    if (!inCode && line.trim() === "") flushBlock();
  }
  flushBlock();
  if (cur.length > 0) out.push(cur.join("\n"));
  return out;
}

/**
 * 一个比 `max` 还长的块按行切（一行比 `max` 还长就按字硬切）；是围栏代码块的，每一截各自补上开 / 合围栏，渲染出来仍是代码块。
 */
function splitOversizeBlock(lines: string[], max: number): string[] {
  const fence = /^\s*(\x60{3,}|~{3,})/.exec(lines[0] ?? "")?.[1] ?? null;
  const body = fence ? lines.slice(1, lines.length - (/^\s*(\x60{3}|~{3})/.test(lines[lines.length - 1] ?? "") ? 1 : 0)) : lines;
  const open = fence ? `${lines[0]}\n` : "";
  const close = fence ? `\n${fence}` : "";
  const room = Math.max(1, max - open.length - close.length);
  const parts: string[] = [];
  let cur = "";
  const push = (): void => {
    if (cur) parts.push(open + cur + close);
    cur = "";
  };
  for (const line of body) {
    for (let at = 0; at < Math.max(1, line.length); at += room) {
      const seg = line.slice(at, at + room);
      if (cur && cur.length + 1 + seg.length > room) push();
      cur = cur ? `${cur}\n${seg}` : seg;
    }
  }
  push();
  return parts;
}

/** 一个回复正文块：不长就整段渲染；长了只渲染第一片 ＋「显示全部」（点了余下的一片一跳地补，不卡输入）。 */
function buildTextBlock(text: string, lazy: boolean | undefined): HTMLElement {
  const div = document.createElement("div");
  div.className = "block-text";
  if (text.length <= LONG_REPLY_HEAD_CHARS) {
    div.innerHTML = renderMarkdown(text, { lazy });
    return div;
  }
  const pieces = markdownPieces(text, LONG_REPLY_HEAD_CHARS);
  const head = document.createElement("div");
  head.innerHTML = renderMarkdown(pieces[0], { lazy });
  const more = document.createElement("button");
  more.type = "button";
  more.className = "block-body-show-full";
  const restChars = text.length - pieces[0].length;
  more.textContent = copyText("cards.markdown.showRest", { kb: (restChars / 1024).toFixed(0) });
  more.addEventListener(
    "click",
    () => {
      more.disabled = true;
      let i = 1;
      const step = makeYieldToMain(() => {
        const piece = document.createElement("div");
        piece.innerHTML = renderMarkdown(pieces[i]);
        more.before(piece);
        i++;
        if (i < pieces.length) step();
        else more.remove();
      });
      step();
    },
    { once: true },
  );
  div.append(head, more);
  return div;
}
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
    expand.textContent = copyText("cards.text.showAll", { kb: sizeKb });
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
    expand.textContent = copyText("cards.markdown.renderAll", { kb: sizeKb });
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

// `defaultModeForTool`〔散文墓碑〕删：哪些工具的结果默认按 Markdown 画由那台后端判（卡型 `md`，随记录成品带来）。

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
  summaryText: string | HTMLElement,
  bodyFactory: () => HTMLElement,
): HTMLElement {
  const d = document.createElement("details");
  d.className = `block-collapsible ${cls}`;

  const s = document.createElement("summary");
  s.className = "block-summary";
  if (typeof summaryText === "string") s.textContent = summaryText;
  else {
    s.classList.add("block-step");
    s.appendChild(summaryText);
  }
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

/** 第一行非空预览，截到 max 字符（不再整条 `split`，见 `format.ts::firstLineOf`） */
function firstLinePreview(text: string, max: number): string {
  const { line, more } = firstLineOf(text, max);
  if (!more) return line;
  return copyText("cards.truncate.ellipsis", { text: line.slice(0, max - 1) });
}

/** 从 Bash 失败 tool_result 文本里抠 exit code（Claude Code 会把它写成 "Exit code N" 一行） */
function extractExitCode(text: string): number | null {
  const m = text.match(/Exit code (\d+)/);
  return m ? Number(m[1]) : null;
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
  // C04c：`| null` —— `ApiMessage.model` 是 `Option<String>` 且无 skip_serializing_if
  // ⇒ 线上是显式 null。下面的真值判断本来就吃得下 null，只是类型此前没说实话。
  model?: string | null,
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
    // 只序列化到够 61 个字为止（`format.ts::jsonPrefix`），
    // 结果逐字等于原来的 `truncate(JSON.stringify(input), 60)`；Write 一类 617 KB 的输入不再整份序列化。
    return truncate(jsonPrefix(input, 60) as string, 60);
  } catch {
    return "";
  }
}

function truncate(s: string, n: number): string {
  return s.length > n ? copyText("cards.truncate.ellipsis", { text: s.slice(0, n) }) : s;
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
