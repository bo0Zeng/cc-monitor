/**
 * **派出子运行的那张工具卡**（卡型 `agent` 由那台后端判，随记录成品的 `toolCards` 带来）。
 *
 * 标题用后端给的通用标签（记录成品的 `childRuns`：标签 ＋ 类别），不读工具入参；展开就是那个子运行的时间线
 * （`run-timeline.ts`，按运行读、与主运行同一套渲染器）。子运行收场后状态收到这张卡上（`markRunCard`）。
 */
import type { JsonlRecord, RenderContext, RenderResult } from "./index";
import type { ChildRunTag } from "../generated/ChildRunTag";
import type { RunState } from "../generated/RunState";
import { RunTimeline } from "../run-timeline";
import { runStateText } from "../runs";
import { copyText } from "../copy-table";

/** 卡标题：`<类别> · <标签>`（没有类别 ⇒ 只有标签；连标签都没有 ⇒ 工具名）。 */
function titleOf(tag: ChildRunTag | undefined, toolName: string): string {
  const label = tag && tag.label.length > 0 ? tag.label : toolName;
  return tag?.kind ? copyText("runCard.summary.text", { kind: tag.kind, label }) : copyText("runCard.summary.bare", { label });
}

/**
 * 构造派出子运行的那张卡。
 * @param toolId       父侧工具调用 id（后端按它找派出链接 ⇒ 子运行）
 * @param toolName     工具名（没有标签时的退路，只作显示）
 * @param tag          后端给的通用标签
 * @param ctx          含 parentPath（父记录路径）与 origin
 * @param renderChild  主渲染入口（子运行的记录用同一套画）
 */
export function buildAgentCard(
  toolId: string,
  toolName: string,
  tag: ChildRunTag | undefined,
  ctx: RenderContext,
  renderChild: (rec: JsonlRecord, ctx: RenderContext) => RenderResult,
): HTMLElement {
  const d = document.createElement("details");
  d.className = "block-collapsible block-agent";
  const title = titleOf(tag, toolName);
  const state: CardState = { title, run: null, timeline: null };
  cardState.set(d, state);

  const s = document.createElement("summary");
  s.className = "block-summary";
  s.textContent = title;
  d.appendChild(s);

  d.addEventListener("toggle", () => {
    if (!d.open) return;
    // 建过 ⇒ 再展开时续读一次（收起期间子运行可能又做了事；上次读失败的也在这里重试）。
    if (state.timeline) {
      void state.timeline.refresh();
      return;
    }
    const run = state.run;
    const nested: RenderContext = {
      parentPath: ctx.parentPath,
      speaker: ctx.speaker ?? null,
      origin: ctx.origin,
      toolUseNames: new Map(),
      toolUseElements: new Map(),
      pendingToolResults: new Map(),
    };
    const timeline = new RunTimeline({
      origin: ctx.origin,
      parent: ctx.parentPath,
      which: run ? { run, tool: toolId } : { tool: toolId },
      render: (rec) => renderChild(rec, nested),
    });
    state.timeline = timeline;
    d.appendChild(timeline.element);
    void timeline.refresh();
  });
  return d;
}

/** 卡上记着的：标题（收场后在它后面接状态）· 运行表说的是哪个子运行（展开时按它读；还没对上 ⇒ 按工具调用 id 读）· 展开过的时间线。 */
interface CardState {
  title: string;
  run: string | null;
  timeline: RunTimeline | null;
}
const cardState = new WeakMap<HTMLElement, CardState>();

/** 子运行的状态收到它那张卡上（运行表给的成品：哪个子运行 · 状态）。 */
export function markRunCard(card: HTMLElement, run: string, state: RunState): void {
  const st = cardState.get(card);
  if (!st) return;
  const changed = st.run !== run || card.dataset.runState !== state;
  // 展开着的时间线跟着长：在跑 ⇒ 运行表每来一帧续读一次；刚收场 ⇒ 最后读一次收尾巴。
  if (st.timeline && (card as HTMLDetailsElement).open && (state === "running" || changed)) void st.timeline.refresh();
  if (!changed) return;
  st.run = run;
  card.dataset.runState = state;
  const s = card.querySelector(":scope > summary");
  if (s) s.textContent = state === "running" ? st.title : copyText("runCard.summary.state", { title: st.title, state: runStateText(state) });
}
