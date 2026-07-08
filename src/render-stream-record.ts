/**
 * renderStreamRecord（P5.2c + P5.3）—— B 重构后三 caller 共享的渲染管线。
 *
 * ## 三个 caller
 *
 * 之前 TabManager.onLine / SessionViewer.load / cards/subagent.renderSubagentBody
 * 各自维护一份"renderMessage → markCardUuid → feedBranchFolder → tool-group
 * 合并 → DOM 挂载"逻辑。漂移风险高（SessionViewer 漏 pendingToolResults 是已知
 * 事故 —— P4 修过）。本模块把这条管线收口，三个 caller 通过 sink 接口注入差异：
 * append vs insertBefore、用不用 BranchFolder.recordAdded、要不要触发 userActive 等。
 *
 * ## tool-group 合并算法（P5.3）
 *
 * 之前合并靠"到达顺序连续"维护 tab.pendingToolGroup 状态字段——一旦 batch / live
 * 跨边界 ToolGroup root 在 fragment 里没贴进 DOM 时被后到的 live 消息错误追加
 * 到 fragment 里 → 后续 flush 整体被 prepend 到顶部。
 *
 * 改成基于 timeline 的"邻居判定"：
 * 1. renderMessage 返回 `tool-group` 时 timeline.peekPrev(seq) 看左邻居
 * 2. 若 prev.kind === "tool-group"（且有 toolGroup 实例）→ addToToolGroup 追加 units
 *    到 prev.toolGroup.body，不创建新 entry
 * 3. 否则 buildToolGroup + addToToolGroup + markCardUuid + timeline.insert
 *
 * 关键属性：合并方向永远按 timeline 已排序状态判断 —— 跟到达顺序解耦。
 * 早 seq 的 tool-only 后到（乱序）也会先按 seq 找到正确位置再判邻居。
 *
 * ## state 收口
 *
 * 之前需要 inPrependMode / pendingPrependFragment / source / pendingToolGroup
 * 四个状态字段；本模块只读 timeline，写 timeline。pendingToolGroup 字段被消除。
 */

import { renderMessage, buildToolGroup, addToToolGroup, type JsonlRecord, type RenderContext } from "./cards";
import { extractBranchRecord, type BranchRecord } from "./branching";
import { observeForEnhance } from "./render";
import { applyIntrinsicSize } from "./height-estimate";
import type { RecordTimeline } from "./record-timeline";
import type { JsonlLinePayload } from "./events";

/**
 * caller（TabManager / SessionViewer / Subagent）提供的差异点。
 *
 * 必填：timeline + branch record 处理。
 * 可选：title 路由、user-active 触发、lazy hljs 注册。
 */
export interface StreamSink {
  /** 按 seq 排序插入用 */
  timeline: RecordTimeline;
  /**
   * 接到 branch record（user/assistant/system/attachment 链节点）时调。
   * - TabManager：`tab.branchFolder.recordAdded(rec)` 实时算 mainBranch
   * - SessionViewer：push 进数组，全部 load 完再 setRecordsAndRebuild 一次
   */
  onBranchRecord(rec: BranchRecord): void;
  /** issue #36：queue-operation enqueue 记录（content）——折叠豁免集合。 */
  onQueueOperation?(content: string): void;
  /**
   * 收到 ai-title / custom-title 时调（TabManager 用）。
   * SessionViewer / Subagent 不实现 = 标题不更新。
   */
  onTitleUpdate?: (title: string) => void;
  /**
   * payload 是真用户输入（type=user + 渲染成 card）时调，传入 sessionId。
   * 仅 TabManager 用（触发自动切 Tab 到对应 session）。
   */
  onRealUserInput?: (sessionId: string) => void;
  /**
   * batch 模式时给 element 注册 IntersectionObserver lazy enhance hljs。
   * 仅 TabManager 在 batch 期间用；SessionViewer/Subagent 不需要（默认 eager 渲染）。
   * 默认 = 不调用（lazy 也不需要 observe，反正不是 batch）。
   */
  observeForLazyEnhance?: boolean;
}

/**
 * Batch13-F40(账本 §3 最终形态):collect/render 两段式。
 * - routeMetaAndBranch:元数据路由 + branch 喂送——**收纳(不建卡)与渲染两条路径
 *   共用的单一来源**,消灭 F39 时代 viewer 手工复刻收集路由的 parity 风险。
 * - renderContentRecord:纯渲染段(建卡/tool-group 合并/DOM 挂载)。
 * - renderStreamRecord:两段组合壳,既有 caller 语义零变化。
 */

/**
 * 元数据路由 + branch record 喂送。返回:
 * - "consumed":ai-title / custom-title / queue-operation——无卡可渲,到此为止;
 * - "content":其余记录(含 render 后会 skip 的 attachment/空 user——它们仍占链节点,
 *   branch record 已在本函数喂送,issue #8 链完整性)。
 */
export function routeMetaAndBranch(
  payload: JsonlLinePayload,
  sink: StreamSink,
): "consumed" | "content" {
  const message = payload.message;

  // 1. ai-title / custom-title 路由
  if (message.type === "ai-title") {
    sink.onTitleUpdate?.(message.aiTitle);
    return "consumed";
  }
  if (message.type === "custom-title") {
    sink.onTitleUpdate?.(message.customTitle);
    return "consumed";
  }

  // 1.5 issue #36：queue-operation 路由——enqueue 的 content 喂给折叠豁免集合，
  //     不渲染、无 uuid 不进链。
  if (message.type === "queue-operation") {
    if (message.operation === "enqueue" && message.content) {
      sink.onQueueOperation?.(message.content);
    }
    return "consumed";
  }

  // 2. branch record 提取（user/assistant/system/attachment 都喂；不区分 kind）
  //    issue #8：链完整性 — 即使 render 会 skip 也要 feed（attachment / 空 user 占链节点）
  const branchRec = extractBranchRecord(message);
  if (branchRec) sink.onBranchRecord(branchRec);
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
  if (routeMetaAndBranch(payload, sink) === "consumed") return;
  renderContentRecord(payload, ctx, sink);
}

/**
 * 纯渲染段:建卡 / tool-group 后处理合并 / DOM 挂载 / userActive。
 * 前置:routeMetaAndBranch 已对该 payload 返回 "content"(meta 已消费、branch 已喂)。
 */
export function renderContentRecord(
  payload: JsonlLinePayload,
  ctx: RenderContext,
  sink: StreamSink,
): void {
  const message = payload.message;

  // 3. 渲染
  // Batch13-F40 仪表:真实建卡计数(与收纳计数对照;emitPerfSummary 落盘取证。
  // jsdom 单测无 main.ts,须防 undefined)
  if (window.__ccmPerf) window.__ccmPerf.recordsRendered = (window.__ccmPerf.recordsRendered ?? 0) + 1;
  const result = renderMessage(message, ctx);

  switch (result.kind) {
    case "skip":
      return;

    case "card": {
      // 普通卡：直接 markCardUuid + timeline.insert
      markCardUuid(result.element, message);
      applyIntrinsicSize(result.element); // Batch13-F38：c-v 估高初值
      sink.timeline.insert({
        seq: payload.seq,
        element: result.element,
        kind: "card",
        toolGroup: null,
      });
      if (sink.observeForLazyEnhance) observeForEnhance(result.element);

      // 真用户输入触发回调（让 TabManager 自动切 Tab）
      if (message.type === "user") {
        sink.onRealUserInput?.(payload.session_id);
      }
      return;
    }

    case "tool-group": {
      // P5.3 后处理合并：看左邻居是不是 tool-group → 是则追加 units
      const prev = sink.timeline.peekPrev(payload.seq);
      if (prev && prev.kind === "tool-group" && prev.toolGroup) {
        addToToolGroup(prev.toolGroup, result.units);
        // units 已挂进 prev.toolGroup.body（DOM 内嵌），不入 timeline 新 entry。
        // 若 batch 模式新 units 要 observe lazy hljs：units 是新插入的 DOM
        if (sink.observeForLazyEnhance) {
          for (const u of result.units) observeForEnhance(u);
        }
        return;
      }

      // 否则新建 group 并 insert
      const group = buildToolGroup(result.timestamp);
      addToToolGroup(group, result.units);
      // tool-group root 也写 data-uuid（首条贡献 uuid）让 BranchFolder 把它当卡识别
      markCardUuid(group.root, message);
      applyIntrinsicSize(group.root); // Batch13-F38：折叠组 = summary 常数
      sink.timeline.insert({
        seq: payload.seq,
        element: group.root,
        kind: "tool-group",
        toolGroup: group,
      });
      if (sink.observeForLazyEnhance) observeForEnhance(group.root);
      return;
    }
  }
}

/**
 * issue #8: 给 user/assistant 卡的 root element 写 data-uuid (+ data-parent-uuid)。
 * BranchFolder 用 data-uuid 扫定位 + 主线判定。
 *
 * issue #21: system 卡（api_error 重试细条——目前唯一会渲染成卡的 system）也要
 * mark：它有 uuid+parentUuid 参与 jsonl 链，BranchFolder 把无 data-uuid 的顶层
 * 元素当"断开 run"——不 mark 会把夹着它的 ESC 折叠段劈成两段、细条裸露在折叠外。
 *
 * 跟原 tabs.ts::markCardUuid 等价 —— P5.2c 抽到本文件，三 caller 共用。
 */
function markCardUuid(el: HTMLElement, rec: JsonlRecord): void {
  if (rec.type !== "user" && rec.type !== "assistant" && rec.type !== "system") {
    return;
  }
  if (!rec.uuid) return; // system 的 uuid 是 Option，缺失就不 mark
  el.setAttribute("data-uuid", rec.uuid);
  if (rec.parentUuid) {
    el.setAttribute("data-parent-uuid", rec.parentUuid);
  }
}
