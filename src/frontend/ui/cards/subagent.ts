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
  d.dataset.runTool = toolId;
  const title = titleOf(tag, toolName);
  d.dataset.runTitle = title;

  const s = document.createElement("summary");
  s.className = "block-summary";
  s.textContent = title;
  d.appendChild(s);

  let timeline: RunTimeline | null = null;
  d.addEventListener("toggle", () => {
    if (!d.open || timeline) return;
    const run = d.dataset.run;
    const nested: RenderContext = {
      parentPath: ctx.parentPath,
      origin: ctx.origin,
      toolUseNames: new Map(),
      toolUseElements: new Map(),
      pendingToolResults: new Map(),
    };
    timeline = new RunTimeline({
      origin: ctx.origin,
      parent: ctx.parentPath,
      which: run ? { run, tool: toolId } : { tool: toolId },
      render: (rec) => renderChild(rec, nested),
    });
    d.appendChild(timeline.element);
    void timeline.refresh();
  });
  return d;
}

/** 子运行的状态收到它那张卡上（运行表给的成品：哪个子运行 · 状态）。 */
export function markRunCard(card: HTMLElement, run: string, state: RunState): void {
  card.dataset.run = run;
  card.dataset.runState = state;
  const s = card.querySelector(":scope > summary");
  const title = card.dataset.runTitle ?? "";
  if (s) s.textContent = state === "running" ? title : copyText("runCard.summary.state", { title, state: runStateText(state) });
}
