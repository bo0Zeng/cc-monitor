/**
 * 一条记录的渲染管线（renderMessage → markCardId → 工具组合并 → 挂进 DOM），
 * 实时标签页 · 会话查看器 · 子运行卡三处共用，差异经 sink 注入（append / insertBefore · 触不触发 userActive）。
 * 主线外（回退掉的）那几条不在这里判：那台后端给清单（实时帧 `branch` · 冷读 `history-branch`），`BranchFolder` 按卡上的 `data-id` 折。
 *
 * 工具组合并按 timeline 的邻居判：renderMessage 交回 `tool-group` 时 `timeline.peekPrev(seq)` 看左邻居，
 * 是工具组 ⇒ 单元并进它的 body，不建新 entry；否则建组、记 id、插进 timeline。
 * 合并方向只看 timeline 已排好的序、与到达顺序无关（早 seq 的后到也先按 seq 找到位置再判邻居）。
 */

import { renderMessage, buildToolGroup, addToToolGroup, type LineRecord, type RenderContext } from "./cards";
import { observeForEnhance } from "./render";
import { applyIntrinsicSize } from "./height-estimate";
import { saidByHuman } from "./speaker";
import { isRetryBar, mergeRetry } from "./cards/api-error";
import { buildInjectedLine, isNoticeLine, mergeNotice } from "./cards/speaker-bar";
import type { RecordTimeline } from "./record-timeline";
import type { JsonlLinePayload } from "./events";

/**
 * caller（TabManager / SessionViewer / Subagent）提供的差异点。
 *
 * 必填：timeline。
 * 可选：title 路由、user-active 触发、lazy hljs 注册。
 */
export interface StreamSink {
  /** 按 seq 排序插入用 */
  timeline: RecordTimeline;
  /**
   * 收到标题记录（`t: "title"`）时调（TabManager 用）。
   * SessionViewer / Subagent 不实现 = 标题不更新。
   */
  onTitleUpdate?: (title: string) => void;
  /**
   * payload 是真用户输入（人说的 said / queued + 渲染成 card）时调，传入 sessionId。
   * 仅 TabManager 用（触发自动切 Tab 到对应 session）。
   */
  onRealUserInput?: (sessionId: string) => void;
  /**
   * lazy 渲染出来的卡交给哪个滚动容器的 IntersectionObserver 补高亮 / 公式（`render.ts::observeForEnhance`）：
   * 实时 tab = 它的 `.stream`，查看器 = 它自己的滚动容器。`null` / 缺省 = 不 observe（当场高亮的卡没有占位要补）。
   */
  enhanceRoot?: HTMLElement | null;
  /**
   * 一张普通卡建好、记过 id 之后调。只有会话查看器实现：给卡挂「从这一轮建分支」。
   * 工具组卡不触发（不在会话轮次上分支）。
   */
  onCardRendered?: (element: HTMLElement, record: LineRecord) => void;
}

/**
 * 两段式：
 * - routeMeta：元数据路由 —— 收纳（不建卡）与渲染两条路共用这一份；
 * - renderContentRecord：纯渲染（建卡 · 工具组合并 · 挂进 DOM）；
 * - renderStreamRecord：两段串起来。
 */

/** meta 路由要的最小 sink 面：查看器的收集段没有 timeline 也能用（StreamSink 结构兼容）。 */
export type MetaSink = Pick<StreamSink, "onTitleUpdate">;

/**
 * 元数据路由。返回：
 * - "consumed"：标题记录 —— 无卡可渲，到此为止；
 * - "content"：其余记录（含 render 后会 skip 的那几种）。
 */
export function routeMeta(payload: JsonlLinePayload, sink: MetaSink): "consumed" | "content" {
  const record = payload.record;
  if (record.t === "title") {
    sink.onTitleUpdate?.(record.text);
    return "consumed";
  }
  return "content";
}

/**
 * 处理一条 jsonl payload 的完整管线（组合壳）。**调用前 ctx 已 setup**。
 */
export function renderStreamRecord(
  payload: JsonlLinePayload,
  ctx: RenderContext,
  sink: StreamSink,
): void {
  if (routeMeta(payload, sink) === "consumed") return;
  renderContentRecord(payload, ctx, sink);
}

// ───────────────────────────────────────────────────────────────────────────
// 单条渲染成本的采集端（`renderContentRecord` 的墙钟，按记录字节分桶，拆四段，进环形缓冲 cap 5000）。
//
// | 子段 | 覆盖 |
// |---|---|
// | `render`   | `renderMessage()`（O(len) 的活都在这里） |
// | `merge`    | 工具组邻居判定 ＋ `addToToolGroup` ＋ `buildToolGroup` |
// | `estimate` | `markCardId` ＋ `onCardRendered` ＋ `applyIntrinsicSize` |
// | `mount`    | `timeline.insert` ＋ `observeForEnhance` |
//
// `total` 是入口出口真夹的，`total − Σ四段` 是分派开销与调用方回调，不摊进任何一段。
// 默认关：分桶要的字节数只能 `JSON.stringify(record)` 现算（本身就是 O(len)）；探针 `null` 时热路径只多一次布尔判断。
// 开了之后字节数、卡型（`card`：第一个 `card-*` 类名，与 `height-estimate.ts::warnUnknownCard` 同口径）、
// 物化进 DOM 的字符数（`domChars`）都在总时刻取完之后才算，不污染分段读数。
// ───────────────────────────────────────────────────────────────────────────

/** 秤 1 的一条样本。时间单位 ms（`performance.now()` 的差）。 */
export interface RenderCostSample {
  /** `JSON.stringify(record)` 的 UTF-8 字节数 —— 分桶用的那根轴 */
  bytes: number;
  /**
   * 走了哪条分支。**`skip` 也留**：那是"白跑一趟 `renderMessage` 什么也没建"的成本，
   * 从账上抹掉它，长尾里不建卡的那几种就成了免费的。
   */
  branch: "skip" | "card" | "tool-group" | "tool-group-merged";
  /** 落进了哪种卡（第一个 `card-*` 类名；skip 记 `"skip"`）—— 成本轴 */
  card: string;
  /** 这条记录物化进 DOM 的字符数（`textContent` 长度）—— 成本轴的机制那一半 */
  domChars: number;
  /** ① `renderMessage()` */
  render: number;
  /** ② tool-group 合并 / 新建外壳 */
  merge: number;
  /** ③ 估高（markCardId + onCardRendered + applyIntrinsicSize） */
  estimate: number;
  /** ④ DOM 挂载（timeline.insert + observeForEnhance） */
  mount: number;
  /** 入口出口夹出来的总 wall time。`≥ render+merge+estimate+mount` */
  total: number;
}

/** 设计逐字：「push 进环形缓冲（**cap 5000**）」 */
export const RENDER_COST_RING_CAP = 5000;

/** `null` = 探针关（生产默认）。非 null 时是环形缓冲的底层数组。 */
let costRing: RenderCostSample[] | null = null;
/** 环形写指针（`length === CAP` 之后才真的绕回来覆盖最旧的） */
let costRingNext = 0;

/** 打开秤 1 的采集并清空缓冲。**只给测量用**，生产代码不调。 */
export function enableRenderCostProbe(): void {
  costRing = [];
  costRingNext = 0;
}

/** 关掉秤 1 的采集并丢掉缓冲（不丢的话它就是一个 5000 条的常驻账本）。 */
export function disableRenderCostProbe(): void {
  costRing = null;
  costRingNext = 0;
}

/**
 * 按**时间顺序**取出缓冲里的样本（未满时就是 push 顺序；满了之后写指针处是最旧的）。
 * 探针关着时返回空数组 —— 判据那边有一格专门钉"空数组不许被当成绿"。
 */
export function readRenderCostSamples(): RenderCostSample[] {
  const ring = costRing;
  if (!ring) return [];
  if (ring.length < RENDER_COST_RING_CAP) return ring.slice();
  return ring.slice(costRingNext).concat(ring.slice(0, costRingNext));
}

function pushCostSample(ring: RenderCostSample[], s: RenderCostSample): void {
  if (ring.length < RENDER_COST_RING_CAP) ring.push(s);
  else ring[costRingNext] = s;
  costRingNext = (costRingNext + 1) % RENDER_COST_RING_CAP;
}

/**
 * 记录字节数（口径 = 线上那一条通用记录的 JSON）。
 * ⚠ 调用点必须在**总时刻取完之后**，否则它自己的 O(len) 会进读数。
 */
function recordBytes(record: LineRecord): number {
  try {
    return new TextEncoder().encode(JSON.stringify(record)).length;
  } catch {
    // 循环引用之类 —— 不让仪表把渲染搞崩，记 0 让它落进最小桶并在报表里显形
    return 0;
  }
}

/** 卡型：第一个 `card-*` 类名（同 `height-estimate.ts::warnUnknownCard` 的口径）。 */
function cardTypeOf(el: HTMLElement): string {
  return Array.from(el.classList).find((c) => c.startsWith("card-")) ?? `<${el.tagName.toLowerCase()}>`;
}

/** 物化进 DOM 的字符数。⚠ 同 `recordBytes`：调用点必须在**总时刻取完之后**。 */
function domCharsOf(els: readonly Element[]): number {
  let n = 0;
  for (const el of els) n += (el.textContent ?? "").length;
  return n;
}

/**
 * 纯渲染段:建卡 / tool-group 后处理合并 / DOM 挂载 / userActive。
 * 前置:routeMeta 已对该 payload 返回 "content"(meta 已消费)。
 *
 * 秤 1 的仪表夹在本函数的入口与每一条 `return` 之前（见上方那段）。
 * **仪表不改渲染行为**：探针关着时每处只多一次 `probe ?` 布尔判断，
 * 开着时也只是多读几次 `performance.now()`，DOM 产物一个字节都不动
 * （`tests/frontend/ui/scale2-height-truth.vitest.ts` 的 DOM 指纹格钉着这一条）。
 */
export function renderContentRecord(
  payload: JsonlLinePayload,
  ctx: RenderContext,
  sink: StreamSink,
): void {
  const probe = costRing;
  const t0 = probe ? performance.now() : 0;
  const message = payload.record;

  // 3. 渲染
  // 建卡计数（与收纳计数对照，emitPerfSummary 落盘；jsdom 单测没有 main.ts，防 undefined）
  if (window.__ccmPerf) window.__ccmPerf.recordsRendered = (window.__ccmPerf.recordsRendered ?? 0) + 1;
  const result = renderMessage(message, ctx);
  const tRender = probe ? performance.now() : 0;
  markMemberUuids(message, ctx, result); // 在秤 1 的 render 段之外（那一段只夹 renderMessage）

  switch (result.kind) {
    case "skip": {
      // 系统注入（「谁说的」稿 A）：不建卡，另放一条旁注细条——开关关着时 CSS 不露、估高 0；
      // 时间线的邻居查询跳过它（`aside`），相邻合并照它不在时一样合。不进 `renderMessage`（秤 2 的 DOM 指纹不动）。
      const injected = message.t === "said" && message.who.speaker.kind === "system" ? message.who.speaker.body : undefined;
      if (typeof injected === "string" && injected !== "") {
        const strip = buildInjectedLine(injected, message.timeText ?? "");
        markCardId(strip, message);
        sink.timeline.insert({ seq: payload.seq, element: strip, kind: "aside", toolGroup: null });
      }
      if (probe) {
        const total = performance.now() - t0;
        pushCostSample(probe, {
          bytes: recordBytes(message),
          branch: "skip",
          card: "skip",
          domChars: 0,
          render: tRender - t0,
          merge: 0,
          estimate: 0,
          mount: 0,
          total,
        });
      }
      return;
    }

    case "card": {
      // 后台任务通知（「谁说的」稿 A）：左邻居也是通知 ⇒ 并进它（时段取两头、逐条留在展开里）。
      if (isNoticeLine(result.element)) {
        const prev = sink.timeline.peekPrev(payload.seq);
        if (prev && isNoticeLine(prev.element)) {
          mergeNotice(prev.element, result.element);
          return;
        }
      }
      // 重试细条（§5.2.5）：左邻居也是一条还在重试的细条 ⇒ 并进它、原地更新，不另起一条。
      if (isRetryBar(result.element)) {
        const prev = sink.timeline.peekPrev(payload.seq);
        if (prev && isRetryBar(prev.element) && prev.element.dataset.state === "retrying") {
          mergeRetry(prev.element, result.element);
          return;
        }
      }
      // 普通卡：直接 markCardId + timeline.insert
      markCardId(result.element, message);
      sink.onCardRendered?.(result.element, message); // 查看器挂分支按钮
      applyIntrinsicSize(result.element); // content-visibility 的估高初值
      const tEstimate = probe ? performance.now() : 0;
      sink.timeline.insert({
        seq: payload.seq,
        element: result.element,
        kind: "card",
        toolGroup: null,
      });
      if (sink.enhanceRoot) observeForEnhance(result.element, sink.enhanceRoot);
      const tMount = probe ? performance.now() : 0;

      // 真用户输入触发回调（让 TabManager 自动切 Tab）。排队消息也算：用户刚插了话，多半正等着看回应。
      // `userActive` 自带三道闸（设置开关 / 5s 手动保护 / batch 期守卫），不会乱切。
      if ((message.t === "said" || message.t === "queued") && saidByHuman(message.who.speaker.kind)) {
        sink.onRealUserInput?.(payload.session_id);
      }
      if (probe) {
        const total = performance.now() - t0;
        pushCostSample(probe, {
          bytes: recordBytes(message),
          branch: "card",
          card: cardTypeOf(result.element),
          domChars: domCharsOf([result.element]),
          render: tRender - t0,
          merge: 0,
          estimate: tEstimate - tRender,
          mount: tMount - tEstimate,
          total,
        });
      }
      return;
    }

    case "tool-group": {
      // 看左邻居是不是工具组 → 是则并进去
      const prev = sink.timeline.peekPrev(payload.seq);
      if (prev && prev.kind === "tool-group" && prev.toolGroup) {
        addToToolGroup(prev.toolGroup, result.units);
        const tMerge = probe ? performance.now() : 0;
        // units 已挂进 prev.toolGroup.body（DOM 内嵌），不入 timeline 新 entry。
        // 若 batch 模式新 units 要 observe lazy hljs：units 是新插入的 DOM
        if (sink.enhanceRoot) {
          for (const u of result.units) observeForEnhance(u, sink.enhanceRoot);
        }
        const tMount = probe ? performance.now() : 0;
        if (probe) {
          const total = performance.now() - t0;
          pushCostSample(probe, {
            bytes: recordBytes(message),
            branch: "tool-group-merged",
            card: cardTypeOf(prev.toolGroup.root),
            domChars: domCharsOf(result.units),
            render: tRender - t0,
            merge: tMerge - tRender,
            estimate: 0,
            mount: tMount - tMerge,
            total,
          });
        }
        return;
      }

      // 否则新建 group 并 insert
      const group = buildToolGroup(result.time);
      addToToolGroup(group, result.units);
      const tMerge = probe ? performance.now() : 0;
      // tool-group root 也写 data-id（首条贡献 id）让 BranchFolder 把它当卡识别
      markCardId(group.root, message);
      applyIntrinsicSize(group.root); // 折叠组 = summary 常数
      const tEstimate = probe ? performance.now() : 0;
      sink.timeline.insert({
        seq: payload.seq,
        element: group.root,
        kind: "tool-group",
        toolGroup: group,
      });
      if (sink.enhanceRoot) observeForEnhance(group.root, sink.enhanceRoot);
      const tMount = probe ? performance.now() : 0;
      if (probe) {
        const total = performance.now() - t0;
        pushCostSample(probe, {
          bytes: recordBytes(message),
          branch: "tool-group",
          card: cardTypeOf(group.root),
          domChars: domCharsOf([group.root]),
          render: tRender - t0,
          merge: tMerge - tRender,
          estimate: tEstimate - tMerge,
          mount: tMount - tEstimate,
          total,
        });
      }
      return;
    }
  }
}

/**
 * **落点标记**：一条记录若没有自己的卡（工具单元并进左邻居的工具组 ·
 * 工具结果被注入进它那个工具调用的单元里），就在它真正落下的那一块上记 `data-member-id`，
 * 会话内查找 / 大纲命中它时 `revealCard` 找得到。
 * - 工具组的单元（新建组与并入左邻居两支都记；新建组的外壳另有 `data-id`）；
 * - said 记录里的工具结果块：注入到了哪个工具调用单元的结果区块，就记在那个区块上。
 * 不用 `data-id`：那是 `BranchFolder` 认卡、切折叠段的键。放在管线这一层、不放进 `renderMessage`：
 * 后者的产物是秤 2 金标准的 DOM 指纹，落点是管线的事。
 */
function markMemberUuids(record: LineRecord, ctx: RenderContext, result: ReturnType<typeof renderMessage>): void {
  const id = record.id;
  if (result.kind === "tool-group") {
    for (const u of result.units) if (!u.dataset.memberId) u.dataset.memberId = id;
  }
  if (record.t !== "said") return;
  for (const b of record.blocks) {
    if (b.type !== "tool_result") continue;
    const inline = ctx.toolUseElements.get(b.for)?.querySelector<HTMLElement>(".block-tool-result-inline");
    if (inline) inline.dataset.memberId = id;
  }
}

/**
 * 给卡的 root 写 data-id（记录的 `id`）：BranchFolder 按它认主线外清单里的那几条，轮次 / 跳转 / 查找按它找卡。
 * 重试细条也要写：回退掉的那一段里夹着它时，不写会把折叠段劈成两段。排队那一句不写（它不在哪一轮的链上）。
 */
export function markCardId(el: HTMLElement, rec: LineRecord): void {
  if (rec.t !== "said" && rec.t !== "reply" && rec.t !== "retry") return;
  el.setAttribute("data-id", rec.id);
}
