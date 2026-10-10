/**
 * 卡片渲染的总分发器。
 *
 * `renderMessage(rec, ctx)` 是核心纯函数：给定一条通用记录（`LineRecord`，ts-rs 从后端导出）+ RenderContext，
 * 返回 `RenderResult`（`card` 普通卡 / `tool-group` 工具组单元 / `skip` 不渲染）。
 * 实时 Tab、历史只读视图、subagent 卡三处共用它，保证视觉一致（详 render-stream-record.ts）。
 *
 * 职责边界：
 * - 按记录的类别 `t` + 内容块分发：said 气泡 / reply 卡 / 纯工具 → tool-group /
 *   tool_result 注入到对应 tool_use 折叠条；slash / compact / agent / diff / interactive /
 *   api-error 子卡委派给 cards/ 同级模块。
 * - said 记录是谁说的（人 · 斜杠命令 · `!` 输入/输出 · 压缩摘要 · 派给子 agent 的活 · 系统注入 · agent 来话 · 后台通知……，
 *   INVARIANT § 20）：判定只在后端，随记录成品带来（`who.speaker` ＋ 要显示的 `who.text`），这里只按它画。
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
import { buildBriefCard } from "./brief";
import { buildAgentCard, isRunCard, settleRunCard } from "./subagent";
import { buildDiffBody } from "./diff";
import { buildInteractiveCard, settleInteractive } from "./interactive";
import type { Pasted } from "../generated/Pasted";
import type { ToolCard } from "../generated/ToolCard";
import type { ChildRunTag } from "../generated/ChildRunTag";
import type { ToolStep } from "../generated/ToolStep";
import type { StepResult } from "../generated/StepResult";
import { buildStepLine, buildThinkingLine, durBetween, paintWaiting, settleStepLine } from "./step-line";
import { waitedNow } from "../duration-format";
import type { PendingCall, RetryOutcome } from "../session-reads";
import { buildApiErrorCard, buildApiRetryCard } from "./api-error";
import { buildUnreadLine } from "./unread";
import { LS_KEYS, safeGet, safeSet } from "../local-storage";
import { firstLineOf, jsonPrefix, sizeText } from "../format";
import { openFileWindow } from "../file-window";
import { resolveRemoteConfigByOrigin } from "../remote-config";
import { toast } from "../kit/toast";
import { isRemoteOrigin, type Origin } from "../ipc/origin";

/**
 * 〔判定只在后端〕记过的一次 tool_use：工具名（标注用、存偏好用）＋ 后端给的卡型（没有 ＝ 普通工具卡）。
 * 界面**不认工具名**：哪个工具画成哪种卡由那台后端的适配层判（`agents/<名>/cards.rs`），随 reply 记录的 `cards` 带来。
 */
export interface ToolUseSeen {
  name: string;
  card: ToolCard | undefined;
  /** 后端给的一行人话（`steps`）与发出那条记录的时刻（后端解好的毫秒；结果到了交那一个读口写耗时）。 */
  step?: ToolStep;
  atMs?: number;
}

/** 一条记录带来的、过程那一行要的成品：reply 的 `steps` · said 的 `results` · 记录时刻。 */
interface StepFacts {
  steps: Readonly<Record<string, ToolStep>>;
  results: Readonly<Record<string, StepResult>>;
  atMs: number | undefined;
  /** 这条记录的 `id`（出口省掉的正文展开时按它取回那一行全文，`RenderContext.fullRecord`）。 */
  id: string;
}
const NO_FACTS: StepFacts = Object.freeze({ steps: Object.freeze({}), results: Object.freeze({}), atMs: undefined, id: "" });
function stepFactsOf(rec: LineRecord): StepFacts {
  if (rec.t === "reply") return { steps: rec.steps ?? NO_FACTS.steps, results: NO_FACTS.results, atMs: rec.atMs, id: rec.id };
  if (rec.t === "said") return { steps: NO_FACTS.steps, results: rec.results ?? NO_FACTS.results, atMs: rec.atMs, id: rec.id };
  return NO_FACTS;
}

/** 一条记录里的块（没有块的那几类 ⇒ 空）。 */
function blocksOf(rec: LineRecord): readonly Block[] {
  return rec.t === "reply" || rec.t === "said" ? rec.blocks : [];
}

/**
 * 出口省掉的正文（`omit` 声明：工具入参 · 结果正文）展开那一下取回：按记录 `id` 问宿主要那一行全文，
 * 在里面找同一个块（`pick`）。取的那一下块里先写一行「读取中」，取不到 ⇒ 写原因，下次展开再取。
 */
function fetchOmitted<T>(
  ctx: RenderContext,
  id: string,
  into: HTMLElement,
  pick: (rec: LineRecord) => T | undefined,
  then: (got: T) => void,
): void {
  const note = document.createElement("div");
  note.className = "block-body block-omitted";
  note.textContent = copyText("cards.omitted.loading");
  into.insertBefore(note, into.firstChild);
  const ask = ctx.fullRecord ? ctx.fullRecord(id) : Promise.reject(new Error(copyText("cards.omitted.noSource")));
  void ask
    .then((rec) => {
      const got = rec ? pick(rec) : undefined;
      if (got === undefined) throw new Error(copyText("cards.omitted.gone"));
      note.remove();
      then(got);
    })
    .catch((e: unknown) => {
      note.dataset.failed = "";
      note.textContent = copyText("cards.omitted.failed", { why: e instanceof Error ? e.message : String(e) });
    });
}

/** 一条记录带来的卡型表（`tool_use.id` → 卡型）；只有 reply 记录带，别的一律空。 */
type ToolCards = Readonly<Record<string, ToolCard>>;
const NO_CARDS: ToolCards = Object.freeze({});
function toolCardsOf(rec: LineRecord): ToolCards {
  return rec.t === "reply" ? rec.cards ?? NO_CARDS : NO_CARDS;
}
/** 派出子运行的那几次调用的通用标签（后端随记录成品带出的 `runs`）。 */
type ChildRuns = Readonly<Record<string, ChildRunTag>>;
const NO_RUNS: ChildRuns = Object.freeze({});
function childRunsOf(rec: LineRecord): ChildRuns {
  return rec.t === "reply" ? rec.runs ?? NO_RUNS : NO_RUNS;
}

// `LineRecord`（通用记录）/ `Block`（内容块）由 ts-rs 从后端 `agents/record.rs` 生成（那就是线定义）。
import type { LineRecord } from "../generated/LineRecord";
import type { Block } from "../generated/Block";
import { copyText } from "../copy-table";

// 本文件内部也用这些名字，所以 import + re-export 都要有：只写 `export type { … } from` 不会把名字带进本地作用域。
export type { Block, LineRecord };
/** 代理的回复。 */
type ReplyRecord = Extract<LineRecord, { t: "reply" }>;
/** 一个工具结果块。 */
export type ToolResultBlock = Extract<Block, { type: "tool_result" }>;


/**
 * 渲染上下文，沿调用链向下传递。
 *
 * 字段命名保持稳定 —— 跨模块（cards/subagent、tabs）依赖。
 */
export interface RenderContext {
  /**
   * 这个会话此刻在等你什么（会话事实 `needs`，后端成品）：在等批准的那一步（`call`）建出来时就画成「在等你批准」。
   * 没有 ⇒ 不画。
   */
  /** 这一步还没结果时的样子（会话事实 `pending[]` 里它那一条的 `state` · `why`）；事实里还没有它 ⇒ `undefined`（不画）。 */
  stepWait?: (call: string) => Pick<PendingCall, "state" | "why"> | undefined;
  /** 一串重试的结局（会话事实 `retries`，按首条重试记录的 `id`）；事实里还没有 ⇒ `undefined`。 */
  retryOutcome?: (id: string) => RetryOutcome | undefined;
  needs?: { kind: string; call: string | null; waitedMs: number | null; receivedAt: number } | null;
  /** 父记录路径：派出子运行的那张卡按它（＋ 工具调用 id）读那个子运行的记录 */
  parentPath: string;
  /**
   * 说话的那一方叫什么（卡头那一格：那一家的短名，后端画像给的 `speakerName`，按会话的那一家取）。
   * `null` ＝ 还不知道是哪一家（标签页的会话事实没到）⇒ 卡头那一格先空着，到了由宿主补上（`cards/speaker.ts`）。
   */
  speaker?: string | null;
  /** 会话来源（本机 = `LOCAL_ORIGIN`；其余 = 远端机器名）。必填：每个造渲染上下文的地方都得说出是哪台机器。 */
  origin: Origin;
  /**
   * tool_use_id → 那一次 tool_use 的工具名与卡型。tool_use 出现在 assistant 消息，tool_result
   * 出现在下一条 user 消息，跨消息不能就地反查；TabManager（或 subagent 嵌套
   * 渲染）持有这张 Map 跨 renderMessage 调用累积。renderBlock 在 tool_use
   * 时写入，在 tool_result 时读取来标注工具名、挑结果默认怎么画（卡型是后端随记录成品带出的 `cards`）。
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
  /**
   * 派活的那段话（子运行记录里那一条）是谁派的：某个 agent ⇒ 它的标签；主会话 ⇒ `null`。不给 ⇒ 当主会话派的。
   */
  briefFrom?: string | null;
  /** 运行 id ⇒ 运行表里的标签（agent 来话的事件条起名用；宿主不给 ⇒ 用来话自带的名字）。 */
  runLabelOf?: (run: string) => string | undefined;
  /**
   * 切块读时 tool_result 可能先于它的 tool_use 到（head 块含 result，older 块才有 use）：
   * 那时 `injectOrBuildToolResult` 先画独立卡，把 block 记在这里（key = 它对的那次工具调用 id）；
   * 全部块读完后 `reconcilePendingToolResults` 再配一次，配上了就注入、删独立卡。
   * 必填：漏传 ⇒ 那条结果永远是独立卡；不需要配的调用方传空 Map。
   */
  pendingToolResults: Map<
    string,
    { block: ToolResultBlock; element: HTMLElement }
  >;
  /**
   * 出口省掉的正文（`omit` 声明：工具入参 · 结果正文）展开时按记录 `id` 取回那一行全文（宿主按骨架里的偏移取，不带声明）。
   * 不给 ⇒ 展开时说取不到（只有记录里真缺那几格时才会问它）。
   */
  fullRecord?: (id: string) => Promise<LineRecord | null>;
  /**
   * 代码块高亮推迟到露出来（占位 ＋ IntersectionObserver）：批量建卡时用，免得 N 个代码块同步卡住主线程。
   * TabManager 在批里设 true；不传 ＝ 当场高亮。
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
  | { kind: "tool-group"; time: string; units: HTMLElement[] };

export function renderMessage(rec: LineRecord, ctx: RenderContext): RenderResult {
  const time = rec.timeText ?? "";
  switch (rec.t) {
    case "said": {
      // 谁说的由后端判好（`who.speaker`）；不建卡的那几种（系统注入 · agent 来话 · 后台通知 · 中断标记……）只是不建卡。
      const said = rec.who;
      const speaker = said.speaker;
      if (!drawsCard(speaker.kind)) return { kind: "skip" };
      switch (speaker.kind) {
        case "slashCommand":
          return { kind: "card", element: buildSlashCommandCard(speaker, time) };
        case "bashInput":
          return { kind: "card", element: buildBashInputCard(speaker, time) };
        case "bashOutput":
          return { kind: "card", element: buildBashOutputCard(speaker, time) };
        case "compactSummary":
          if (!said.text) return { kind: "skip" };
          return {
            kind: "card",
            element: buildCompactSummaryCard(said.text, time),
          };
        case "toolResult":
          break;
        case "agentMessage":
          return { kind: "card", element: buildAgentBar(speaker, time, speaker.from ? ctx.runLabelOf?.(speaker.from) : undefined) };
        case "peerSession":
          return { kind: "card", element: buildPeerBar(speaker, time) };
        case "coordinator":
          return { kind: "card", element: buildCoordinatorBar(speaker, time) };
        case "taskNotification":
          return { kind: "card", element: buildNoticeLine(speaker, rec.at ?? "", time) };
        case "interrupt":
          return { kind: "card", element: buildInterruptLine(time) };
        case "agentTask":
          // 派给子 agent 的活：不是用户说的，画成带抬头的框（谁派的 · 几点派的）。
          if (!said.text) return { kind: "skip" };
          return { kind: "card", element: buildBriefCard(said.text, time, ctx.briefFrom ?? null) };
        default:
          // 人说的话：用户气泡；没有正文（只有图片之类）不建卡。
          if (!said.text) return { kind: "skip" };
          return { kind: "card", element: buildUserCard(time, said.text, said.pasted) };
      }

      // 工具结果回灌：注入到对应 tool_use 折叠条内部，返回 null；
      // 只有找不到匹配 tool_use 的 fallback 才产生独立 element。
      const blocks = rec.blocks.filter((b) => b.type === "tool_result");
      if (blocks.length === 0) return { kind: "skip" };
      const facts = stepFactsOf(rec);
      const units = blocks
        .map((b) => renderBlock(b, ctx, NO_CARDS, NO_RUNS, facts))
        .filter((el): el is HTMLElement => el !== null);
      if (units.length === 0) return { kind: "skip" };
      return { kind: "tool-group", time, units };
    }
    case "reply": {
      // 上游最终失败写的报错 → 红色报错卡（当普通回复画会让人以为还在跑）。
      if (rec.error) {
        return {
          kind: "card",
          element: buildApiErrorCard({
            timeLabel: time,
            reason: rec.error.reason,
            text: textOf(rec.blocks).trim(),
            status: rec.error.status ?? null,
          }),
        };
      }
      // 代理那一侧自动写的应答（不是模型说的）：不建卡。
      if (rec.autoReply) return { kind: "skip" };
      const meaningful = rec.blocks.filter((b) => {
        if (b.type === "text" || b.type === "thinking") return b.text.trim().length > 0;
        return true;
      });
      if (meaningful.length === 0) return { kind: "skip" };

      const hasText = meaningful.some((b) => b.type === "text");
      // issue #21：含交互等待工具（AskUserQuestion / ExitPlanMode）的消息走
      // kind:"card"——它们要默认可见，不能折进 card-tool-group（进组判定在
      // 记录级，kind:"card" 是唯一的不进组通路）。
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
      return { kind: "tool-group", time, units };
    }
    case "retry":
      // 上游失败、将重试的中间态 → 细条提示（不画的话重试风暴时只看到「卡住」）。
      return {
        kind: "card",
        element: buildApiRetryCard({
          timeLabel: time,
          reason: rec.reason,
          retryAttempt: rec.attempt ?? null,
          maxRetries: rec.max ?? null,
          id: rec.id,
          outcome: ctx.retryOutcome?.(rec.id),
        }),
      };
    case "unread":
      // 这一家的这一行核心认不出：一行 warn 细条（字与摘录都是核心写的），默认折起。
      return { kind: "card", element: buildUnreadLine({ text: rec.text, excerpt: rec.excerpt, time }) };
    case "title":
      return { kind: "skip" };
    // 插进正在跑的那一轮的一句（那一轮里没有它自己的 said 记录）：不在这里建卡，那句话就整条消失。
    case "queued": {
      const text = rec.who.speaker.kind === "human" ? rec.who.text : "";
      if (!text) return { kind: "skip" };
      return { kind: "card", element: buildQueuedUserCard(text, time) };
    }
    default:
      return { kind: "skip" };
  }
}

/** 排队消息的用户卡：多一个「排队」标记 —— 这条消息是插进正在跑的那一轮的，先后只由 `seq` 保证，得让人看出它是插进来的。 */
function buildQueuedUserCard(text: string, time: string): HTMLElement {
  const card = document.createElement("div");
  card.className = "card card-user card-user-queued";
  card.appendChild(cardHeader(copyText("cards.queuedUser.title"), time));

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
  /** 第一条的钟面（后端写好的 `timeText`）。 */
  since: string;
}

/** 外层折叠卡 → 它的工具组（结果后到、注入进组里某一条时，要回头改那一组收着时那一行）。 */
const groupOfRoot = new WeakMap<HTMLElement, ToolGroup>();

/** `el` 所在的那个工具组收着时那一行重写一遍（`el` 不在任何组里 ⇒ 不动）。 */
function refreshGroupAround(el: HTMLElement | null): void {
  const root = el?.closest<HTMLElement>(".card-tool-group");
  const group = root ? groupOfRoot.get(root) : undefined;
  if (group) updateToolGroupSummary(group);
}

export function buildToolGroup(since: string): ToolGroup {
  const root = document.createElement("details");
  root.className = "card card-tool-group";

  const summary = document.createElement("summary");
  summary.className = "card-tool-group-summary";
  root.appendChild(summary);

  const body = document.createElement("div");
  body.className = "card-tool-group-body";
  root.appendChild(body);

  const group: ToolGroup = { root, body, summary, count: 0, since };
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
  const since = group.since;
  const failed = group.body.querySelectorAll(":scope > .block-has-error, :scope > .block-tool-result.block-error").length;
  let agents = 0;
  for (const el of group.body.children) if (isRunCard(el)) agents++;
  group.summary.textContent =
    failed > 0 && agents > 0
      ? copyText("cards.toolGroup.summaryBoth", { count, failed, agents, since })
      : failed > 0
        ? copyText("cards.toolGroup.summaryFailed", { count, failed, since })
        : agents > 0
          ? copyText("cards.toolGroup.summaryAgents", { count, agents, since })
          : copyText("cards.toolGroup.summary", { count, since });
}

/** 粘贴块超过这么多行就折成一行。 */
export const PASTE_FOLD_LINES = 12;

/**
 * 人说的话（含粘贴进来的块）：粘贴块只露正文（两头的标记不露）；超过 [`PASTE_FOLD_LINES`] 行折成一行「粘贴的内容 · N 行」，点开就地展开。
 * 块的边界、正文那一截与行数都是后端的 `who.pasted`（UTF-16 下标），界面不认标记的写法。
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
  time: string,
  text: string,
  pasted?: readonly Pasted[],
): HTMLElement {
  const card = document.createElement("div");
  card.className = "card card-user";
  card.appendChild(cardHeader(copyText("cards.user.title"), time));

  const body = document.createElement("div");
  body.className = "card-body";
  fillSaid(body, text, pasted);
  card.appendChild(body);
  return card;
}

function buildAssistantCard(
  rec: ReplyRecord,
  meaningful: Block[],
  ctx: RenderContext,
): HTMLElement {
  const card = document.createElement("div");
  card.className = "card card-assistant";
  // 卡头那一家的名字：会话是哪一家由后端说（标签页 · 历史行的 `agent`），名字取画像里的短名；不按文件名猜。
  card.appendChild(cardHeader(ctx.speaker ?? "", rec.timeText ?? "", rec.model));

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
  block: Block,
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
        buildThinkingLine(copyText("cards.thinking.title"), firstLineOf(block.text.trim(), 160).line),
        () => {
          const body = document.createElement("div");
          body.className = "block-body block-body-md";
          // 展开那一刻才建（点开 ＝ 就在眼前）⇒ 一律当场高亮：批里建的卡早被 `enhanceCard` 标过，这时才长出来的占位没人再补。
          body.innerHTML = renderMarkdown(block.text);
          return body;
        },
      );
    }
    case "tool_use": {
      // 记下 id → 名字与卡型，给下一条消息的 tool_result 反查用
      const card = cards[block.id];
      const step = facts.steps[block.id];
      ctx.toolUseNames.set(block.id, { name: block.name, card, step, atMs: facts.atMs });

      // 卡型是那台后端判的（`cards`）；派出子运行的那次调用 → 派出卡（卡头点了开那个子运行自己的窗口）
      if (card === "agent") {
        const runCard = buildAgentCard(block.id, block.name, runs[block.id]);
        ctx.runCards?.set(block.id, runCard);
        ctx.toolUseElements.set(block.id, runCard); // 派出那一方拿到的那次结果收进这张卡（`settleRunCard`）
        return runCard;
      }
      // issue #21：交互等待工具 → 默认展开的提问卡 / plan 卡（用户在被等着，
      // 折叠会误以为 LLM 还在输出）。内容是核心那一格（`steps[id].ask`）；没有 throw → 回退通用折叠卡。
      if (card === "interactive") {
        try {
          const el = buildInteractiveCard(step?.ask, { lazy: ctx.lazy });
          ctx.toolUseElements.set(block.id, el); // result 回填靶（同 buildToolUseCard）
          return el;
        } catch (e) {
          console.warn("interactive card fallback:", block.name, e);
        }
      }
      return buildToolUseCard(block, ctx, card, step, facts.id);
    }
    case "tool_result": {
      return injectOrBuildToolResult(block, ctx, facts);
    }
    default: {
      // 图片块（只在结果里有意义）· 往后新加的块：不抛异常，给出可识别占位以便诊断。
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
  block: Extract<Block, { type: "tool_use" }>,
  ctx: RenderContext,
  card: ToolCard | undefined,
  step: ToolStep | undefined,
  recordId: string,
): HTMLElement {
  // 入参被出口省掉了（`omit` 声明）⇒ 折起那一行照画（`steps` 是核心给的），展开那一下按记录 id 取回那一块。
  const omitted = !("input" in block);
  const callId = block.id;
  const command = card === "command" && !omitted ? commandOf(block.input) : null;
  const summary = command ? commandSummary(command.command) : omitted ? "" : summarizeInput(block.input);
  const d = document.createElement("details");
  d.className = "block-collapsible block-tool-use";

  // 一步一行（§5.2.3）：主参数与说明是后端给的（`steps`）；没有那一格 ⇒ 工具名 ＋ 入参一句兜底。
  const s = document.createElement("summary");
  s.className = "block-summary block-step";
  const line = buildStepLine(block.name, step, summary, block.id);
  s.appendChild(line);
  d.appendChild(s);
  // 会话事实已经说了这一步是什么样子 ⇒ 建出来就照画；还没说 ⇒ 不画状态，等事实（不按「没有结果」当在跑）。
  const w = ctx.stepWait?.(block.id);
  const n = ctx.needs;
  if (w) paintWaiting(line, w.state, n?.call === block.id ? waitedNow(n, Date.now()) : null, n?.kind === "approve", w.why);

  const wrap = document.createElement("div");
  wrap.className = "block-body-wrap";
  d.appendChild(wrap);

  let argsRendered = false;
  let fetching = false;
  d.addEventListener("toggle", () => {
    if (!d.open || argsRendered || fetching) return;
    if (omitted) {
      fetching = true;
      fetchOmitted(
        ctx,
        recordId,
        wrap,
        (rec) => blocksOf(rec).find((b): b is Extract<Block, { type: "tool_use" }> => b.type === "tool_use" && b.id === callId),
        (full) => {
          fetching = false;
          argsRendered = true;
          buildArgs(full, card === "command" ? commandOf(full.input) : null);
        },
      );
      // 取不到 ⇒ 下次展开再取
      void Promise.resolve().then(() => {
        if (!argsRendered) fetching = false;
      });
      return;
    }
    buildArgs(block, command);
    argsRendered = true;
  });

  const buildArgs = (block: Extract<Block, { type: "tool_use" }>, command: { command: string; description: string | null } | null): void => {
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
    // 远端会话 ＋ 带 file_path 的工具（Read / Write / Edit …）⇒ body 顶上一条「在文件窗口里定位」的链接。
    // 放 body 不放 summary：summary 会被 tool_result 注入重写。
    const fp = fileInputPath(block.input);
    if (isRemoteOrigin(ctx.origin) && fp) {
      wrap.insertBefore(buildRemoteFileLink(ctx.origin, fp), wrap.firstChild);
    }
  };

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
 * tool_use 输入里能在文件窗口定位的路径：Read / Write / Edit / MultiEdit 的 `file_path`、NotebookEdit 的 `notebook_path`。
 * 只认绝对 POSIX 路径（相对路径会解析到错的目录）；否则 null。
 */
export function fileInputPath(input: unknown): string | null {
  if (input && typeof input === "object") {
    const obj = input as Record<string, unknown>;
    const fp = obj.file_path ?? obj.notebook_path;
    if (typeof fp === "string" && fp.startsWith("/")) return fp;
  }
  return null;
}

/** 远端文件路径的可点元素：点了在文件窗口里定位那份文件。 */
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
 * 工具结果文本经手量的计数器（`buildResultBody` 闭包捕获了多少文本）。
 *
 * - 单位是 UTF-16 码元（`String.length`）；是累计流量、不是瞬时驻留（重渲一次再记一次）⇒ 驻留量的上界；不含 DOM 与中间产物。
 * - `produced` 记在 `renderResultContent` 出口，`captured` 记在 `buildResultBody` 入口：`wrap` 找不到时只造不捕获，
 *   fallback 的 `makeCollapsible` 没跑也已捕获 ⇒ 只看一个会高估或低估留存，两个都记。
 * - 纯计数、零 DOM 副作用；每条 tool_result 两次整数加法。
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
  block: ToolResultBlock,
  ctx: RenderContext,
  facts: StepFacts = NO_FACTS,
): HTMLElement | null {
  // 正文被出口省掉了（`omit` 声明）⇒ `null`：结果那一行照画（预览是核心给的），展开那一下按记录 id 取回那一块。
  const text = block.content ? renderResultContent(block.content) : null;
  // 秤 6：点名的那一处累加。纯计数。
  if (text !== null) {
    resultTextLedger.produced += 1;
    resultTextLedger.producedUnits += text.length;
    if (text.length > resultTextLedger.maxUnits) {
      resultTextLedger.maxUnits = text.length;
    }
  }
  const sizeOrNothing = (): string => (block.content ? ` · ${approximateSize(block.content)}` : "");
  const bodyInto = (el: HTMLElement): void => {
    if (text !== null) {
      buildResultBody(el, text, toolName, mdByDefault);
      return;
    }
    const host = el.closest("details") ?? el;
    let done = false;
    const open = (): void => {
      if (done || !(host as HTMLDetailsElement).open) return;
      done = true;
      fetchOmitted(
        ctx,
        facts.id,
        el,
        (rec) =>
          blocksOf(rec).find((b): b is ToolResultBlock => b.type === "tool_result" && b.for === block.for)?.content,
        (content) => buildResultBody(el, renderResultContent(content), toolName, mdByDefault),
      );
    };
    host.addEventListener("toggle", open);
    open();
  };
  const exitCode = facts.results[block.for]?.exitCode ?? null;
  // 首行预览是核心出的一格（`results[id].preview`，截法只在核心一处）：出口省掉结果正文之后照样有。
  const preview = facts.results[block.for]?.preview ?? "";
  const seen = ctx.toolUseNames.get(block.for);
  const toolName = seen?.name ?? "tool";
  // 结果默认怎么画按后端给的卡型（`md`），不按工具名自己判。
  const mdByDefault = seen?.card === "md";
  const errTag = block.isError
    ? exitCode !== null
      ? ` · exit ${exitCode}`
      : " · error"
    : "";

  const host = ctx.toolUseElements.get(block.for);
  // 派出子运行的那次调用：交回的结果 / 报错收进派出卡（可展开）。
  // 正文被省掉了 ⇒ 派出卡里先写核心给的预览（展开那张卡时它自己读那个子运行的记录）。
  if (host && settleRunCard(host, text ?? preview, block.isError === true)) return null;
  // 提问 / 计划答了之后：后端读出了答了什么 ⇒ 卡上写结果，不印 Claude Code 的英文原句。
  const answered = facts.results[block.for];
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
      if (block.isError) {
        resultEl.classList.add("block-error");
        host.classList.add("block-has-error");
        refreshGroupAround(host); // 收着的工具组那一行要说出「1 个失败」
      }

      const labelPrefix = block.isError
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
      summary.textContent = preview ? `${labelPrefix} · ${preview}` : `${labelPrefix}${sizeOrNothing()}`;
      resultEl.appendChild(summary);

      // 渲染模式 toolbar + body (lazy build 首次展开时再实际产生 DOM)
      bodyInto(resultEl);

      // 同步那一步的一行：状态图标与右侧小字（后端给的结果一句 ＋ 两条记录之间多久，交那一个读口）。再来一次结果就再改一次。
      const line = host.querySelector<HTMLElement>(":scope > .block-summary > .step-line");
      if (line) settleStepLine(line, seen?.step, facts.results[block.for], block.isError === true, durBetween(seen?.atMs, facts.atMs));
    }
    return null;
  }

  // fallback：tool_use 没找到 → 独立折叠条
  const cls = block.isError
    ? "block-tool-result block-error"
    : "block-tool-result";
  const summaryText = preview ? `${toolName}${errTag} · ${preview}` : `${toolName}${errTag}${sizeOrNothing()}`;
  const fallback = makeCollapsible(cls, summaryText, () => {
    const container = document.createElement("div");
    bodyInto(container);
    return container;
  });
  // 标记 + 登记到 pending map，给切块场景的 reconcile 用
  fallback.setAttribute("data-tool-use-id", block.for);
  if (ctx.pendingToolResults) {
    ctx.pendingToolResults.set(block.for, { block, element: fallback });
    // 收尾对账再注入时还要那条记录的结果一句（预览 · 退出码 · 「全文」按哪条记录取）
    pendingFacts.set(fallback, facts);
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
 * fallback 单元在工具组里（timeline 记的是组 root，不是单元）：单元摘掉后组空了 ⇒ 连组壳一起摘、交回 root，
 * 由调用方从 timeline 出账（不然留一个空的「1 个工具调用」且账上挂着离场元素）。组里还有别的 ⇒ `null`。
 */
export function removeEmptyToolGroupShell(host: HTMLElement | null): HTMLElement | null {
  if (!host) return null;
  const body = host.querySelector(":scope > .card-tool-group-body");
  if (!body || body.childElementCount > 0) return null;
  host.remove();
  return host;
}

/** 先画成独立卡的结果 ⇒ 它那条记录的结果一句（收尾对账注入时照用；独立卡随 DOM 走，键是它）。 */
const pendingFacts = new WeakMap<HTMLElement, StepFacts>();

export function reconcilePendingToolResults(ctx: RenderContext): HTMLElement[] {
  if (!ctx.pendingToolResults || ctx.pendingToolResults.size === 0) return [];
  const toDelete: string[] = [];
  // 交回需要从 timeline 出账的元素（被连根摘掉的空组壳 root）。
  // fallback 单元本身从不是 timeline entry(见 removeEmptyToolGroupShell 注释)。
  const removed: HTMLElement[] = [];
  for (const [toolUseId, { block, element }] of ctx.pendingToolResults) {
    if (!ctx.toolUseElements.has(toolUseId)) continue; // 仍然没匹配，保留
    // 已有 host → 重新调注入（injectOrBuildToolResult 走"已 host"分支，返 null）
    const reInjected = injectOrBuildToolResult(block, ctx, pendingFacts.get(element));
    if (reInjected === null) {
      // fallback 身上的落点标记（`render-stream-record.ts::markMemberUuids` 记的）跟着搬到注入出来的结果区块上
      const member = element.dataset.memberId;
      if (member) {
        const inline = ctx.toolUseElements.get(toolUseId)?.querySelector<HTMLElement>(".block-tool-result-inline");
        if (inline) inline.dataset.memberId = member;
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
  btnMd.textContent = copyText("cards.result.markdown");
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
  more.textContent = copyText("cards.markdown.showRest", { size: sizeText(restChars) });
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
    expand.textContent = copyText("cards.text.showAll", { size: sizeText(text.length) });
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
    expand.textContent = copyText("cards.text.showAll", { size: sizeText(cleaned.length) });
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

// 哪些工具的结果默认按 Markdown 画由那台后端判（卡型 `md`，随记录成品带来）；这里只记人手动切过的那一种。

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

/** 工具结果的内容块（正文 / 图片）⇒ 一段文字。 */
function renderResultContent(content: readonly Block[]): string {
  const parts: string[] = [];
  for (const item of content) {
    if (item.type === "text") {
      parts.push(item.text);
    } else if (item.type === "image") {
      // 图片是给模型看的 base64，监控视图不展开
      const src = item.source as { media_type?: string } | null;
      parts.push(`[image ${src?.media_type ?? "unknown"}]`);
    } else {
      parts.push(prettyJson(item));
    }
  }
  return parts.join("\n");
}

// === helpers ===

function cardHeader(
  role: string,
  /** 记录的钟面（后端写好的 `timeText`）。 */
  time: string,
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
  t.textContent = time;
  h.appendChild(t);
  if (model) {
    const m = document.createElement("span");
    m.className = "model";
    m.textContent = model;
    h.appendChild(m);
  }
  return h;
}

/** 正文块连成一段（报错卡的正文）。 */
function textOf(blocks: readonly Block[]): string {
  return blocks
    .map((b) => (b.type === "text" ? b.text : ""))
    .filter(Boolean)
    .join("\n");
}

function summarizeInput(input: unknown): string {
  if (input === null || input === undefined) return "";
  if (typeof input === "string") return truncate(input, 60);
  try {
    // 只序列化到够 61 个字为止（`format.ts::jsonPrefix`），结果与 `truncate(JSON.stringify(input), 60)` 逐字相同；大输入不整份序列化。
    return truncate(jsonPrefix(input, 60) as string, 60);
  } catch {
    return "";
  }
}

function truncate(s: string, n: number): string {
  return s.length > n ? copyText("cards.truncate.ellipsis", { text: s.slice(0, n) }) : s;
}

function approximateSize(content: readonly Block[]): string {
  if (content.every((b) => b.type === "text")) {
    return sizeText(textOf(content).length);
  }
  try {
    return sizeText(JSON.stringify(content).length);
  } catch {
    return "";
  }
}
