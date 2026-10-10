/**
 * **派出子运行的那张工具卡**（卡型 `agent` 由那台后端判，随记录成品的 `toolCards` 带来）。
 *
 * 卡头一整行：状态图标 · 类别 · 标签 ｜「窗口已开」· 状态 · 用时 · ↗；点卡头 ⇒ 开那个子运行自己的窗口（发 [`REVEAL_RUN_EVENT`]，
 * 宿主接：主窗口 / agent 窗口）。卡里不就地展开时间线；派出那一方拿到的那次结果（交回的结果 / 报错）收在卡里、可展开
 * （[`settleRunCard`]，结果那一条到了时由渲染管线交来）。
 * 标题用后端给的通用标签（记录成品的 `childRuns`：标签 ＋ 类别），不读工具入参；状态 · 时刻只读运行表（[`markRunCard`]）。
 */
import type { ChildRunTag } from "../generated/ChildRunTag";
import type { RunInfo } from "../generated/RunInfo";
import type { RunState } from "../generated/RunState";
import { runLastText, runStateMark, runStateText } from "../runs";
import { copyText } from "../copy-table";
import { spanNow } from "../duration-format";
import { icon } from "../kit/icon";
import { tag } from "../kit/badge";
import { REVEAL_RUN_EVENT } from "./speaker-bar";
import s from "./run-card.module.css";

/** 卡上记着的：标签 · 运行表说的是哪个子运行（还没对上 ⇒ `null`，点卡头按工具调用 id 找）· 那几格。 */
interface CardState {
  label: string;
  run: string | null;
  head: HTMLElement;
  icon: HTMLElement;
  state: HTMLElement;
  opened: HTMLElement;
  sub: HTMLElement;
}
const cardState = new WeakMap<HTMLElement, CardState>();

/**
 * 构造派出子运行的那张卡。
 * @param toolId    父侧工具调用 id（宿主按它 ⇒ 子运行）
 * @param toolName  工具名（没有标签时的退路，只作显示）
 * @param runTag    后端给的通用标签
 */
export function buildAgentCard(toolId: string, toolName: string, runTag: ChildRunTag | undefined): HTMLElement {
  const card = document.createElement("div");
  card.className = s.runCard;
  card.dataset.role = "run-card";
  card.dataset.tool = toolId;
  const label = runTag && runTag.label.length > 0 ? runTag.label : toolName;

  const head = document.createElement("button");
  head.type = "button";
  head.className = s.runHead;
  head.title = copyText("agentsPanel.row.openHint");
  const ic = document.createElement("span");
  ic.className = s.runIcon;
  const name = document.createElement("span");
  name.className = s.runLabel;
  name.textContent = label;
  const right = document.createElement("span");
  right.className = s.runRight;
  // 「窗口已开」只在开着时挂进来（徽标自带 display，`hidden` 盖不住）。
  const opened = tag(copyText("agentsPanel.row.opened"));
  opened.title = copyText("agentsPanel.row.openedHint");
  opened.dataset.role = "window-open";
  const state = document.createElement("span");
  state.dataset.role = "run-state";
  right.append(state, icon("front", "compact"));
  head.appendChild(ic);
  if (runTag?.kind) head.appendChild(tag(runTag.kind));
  head.append(name, right);
  head.addEventListener("click", () => {
    const run = cardState.get(card)?.run ?? undefined;
    card.dispatchEvent(new CustomEvent(REVEAL_RUN_EVENT, { bubbles: true, detail: { run, tool: toolId } }));
  });
  // 「最近：…」只在跑着时挂进来。
  const sub = document.createElement("div");
  sub.className = s.runSub;
  card.append(head);
  cardState.set(card, { label, run: null, head, icon: ic, state, opened, sub });
  return card;
}

/** 是不是一张派出卡（工具组收着时那一行数「几个子 agent」）。 */
export function isRunCard(el: Element): boolean {
  return cardState.has(el as HTMLElement);
}

/** 子运行的状态收到它那张卡上（运行表给的成品：哪个子运行 · 状态；有整格时再写用时与「最近：…」）。 */
export function markRunCard(card: HTMLElement, run: string, state: RunState, info?: RunInfo): void {
  const st = cardState.get(card);
  if (!st) return;
  st.run = run;
  card.dataset.runState = state;
  st.icon.dataset.state = state;
  st.icon.replaceChildren(runStateMark(state));
  const took =
    info && state !== "running" && state !== "unknown" && info.started_ms !== undefined && info.ended_ms !== undefined
      ? copyText("agentWindow.facts.took", { dur: spanNow(info.started_ms, info.ended_ms) })
      : null;
  st.state.textContent = took ? [runStateText(state), took].join(copyText("kit.text.sep")) : runStateText(state);
  const last = info && state === "running" ? runLastText(info, null) : null;
  if (last === null) st.sub.remove();
  else {
    st.sub.textContent = copyText("agentsPanel.row.last", { last });
    if (!st.sub.isConnected) st.head.after(st.sub);
  }
}

/** 这个子运行的窗口开着没有（卡头「窗口已开」）。 */
export function markRunWindow(card: HTMLElement, open: boolean): void {
  const st = cardState.get(card);
  if (!st) return;
  if (open && !st.opened.isConnected) st.state.before(st.opened);
  else if (!open) st.opened.remove();
  st.head.title = open ? copyText("agentsPanel.row.openedHint") : copyText("agentsPanel.row.openHint");
}

/**
 * 派出那一方拿到的那次结果：交回的结果（几个字）或报错，收在卡里、可展开。是这种卡 ⇒ `true`。
 * `chars` 是核心出的那一格（`results[id].chars`）—— 出口省掉结果正文时 `text` 只是预览，字数不从它数。
 */
export function settleRunCard(card: HTMLElement, text: string, failed: boolean, chars: number): boolean {
  const st = cardState.get(card);
  if (!st) return false;
  for (const old of card.querySelectorAll('[data-role="run-result"]')) old.remove();
  const d = document.createElement("details");
  d.className = s.runResult;
  d.dataset.role = "run-result";
  d.dataset.failed = failed ? "1" : "0";
  const sum = document.createElement("summary");
  sum.textContent = failed ? copyText("runCard.result.failed") : copyText("runCard.result.done", { n: chars });
  d.appendChild(sum);
  // 正文展开时才建（交回的结果可能很长；收着的卡不为它付排版）。
  d.addEventListener("toggle", () => {
    if (!d.open || d.childElementCount > 1) return;
    const body = document.createElement("pre");
    body.className = s.runResultBody;
    body.textContent = text;
    d.appendChild(body);
  });
  card.appendChild(d);
  return true;
}
