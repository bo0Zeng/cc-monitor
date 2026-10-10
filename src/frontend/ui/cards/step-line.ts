/**
 * 过程里的一步一行：状态图标 · 工具名 · 主参数（等宽、一行、路径中间省略）· 说明 · 右侧小字。
 *
 * 判定都在后端：主参数与说明是 assistant 记录成品的 `toolSteps`，结果一句的数是 user 记录成品的 `toolResults`；
 * 这里只把它们排成一行、按 id 配对（同一步的两段拼回去）；耗时交那一个读口（`duration-format.ts::spanNow`，两条记录后端解好的 `atMs`）。界面不认入参与结果的结构。
 * 还没有结果的那一步是什么样子（在跑 · 在等你 · 状态不明）读会话事实的 `pending[].state`（[`paintWaiting`]）；事实到之前不画状态，
 * 不按「没有结果」当它在跑。
 */
import type { ToolStep } from "../generated/ToolStep";
import type { StepResult } from "../generated/StepResult";
import { icon, type IconName } from "../kit/icon";
import { spinner } from "../kit/progress";
import { copyText } from "../copy-table";
import { spanNow } from "../duration-format";
import type { StepWait, UnclearWhy } from "../session-reads";

/**
 * 一步的状态。结果到了：成功 · 失败 · 人没批准 · 认不出的工具（只给原文）；还没结果：事实还没到（`pending`，不画）·
 * 在跑 · 在等你（`awaiting`，[`markAwaiting`]）· 状态不明（`unclear`）—— 后三种只照会话事实画。
 */
export type StepState = "pending" | "running" | "unclear" | "ok" | "failed" | "rejected" | "unknown";

/** 还没结果的那几种（会话事实一到就按它重画）。 */
const WAITING = new Set(["pending", "running", "awaiting", "unclear"]);

/** 两条记录之间多久（毫秒时刻，后端解好的 `atMs`；任一缺 ⇒ `null`）⇒ 那一个读口写的字。 */
export function durBetween(from: number | undefined, to: number | undefined): string | null {
  return from === undefined || to === undefined || to < from ? null : spanNow(from, to);
}

/** 结果到了 ⇒ 这一步的状态。 */
export function stateOf(step: ToolStep | undefined, res: StepResult | undefined, isError: boolean): StepState {
  if (res?.rejected) return "rejected";
  if (res ? !res.ok : isError) return "failed";
  if (step && !step.known) return "unknown";
  return "ok";
}

/** 右侧小字：改动 `+38 −6` · 读了几行 · 几个文件 · 否则耗时；失败 `失败 · 41s` · 被拒「未批准」· 认不出「未识别结果 · 原文」。 */
export function stepRight(step: ToolStep | undefined, res: StepResult | undefined, state: StepState, durText: string | null): string {
  const dur = durText ?? "";
  switch (state) {
    case "pending":
    case "running":
      return "";
    case "unclear":
      return copyText("stream.step.unclear");
    case "rejected":
      return copyText("stream.step.rejected");
    case "failed":
      return dur ? copyText("stream.step.failedFor", { dur }) : copyText("stream.step.failed");
    case "unknown":
      return copyText("stream.step.unknown");
    case "ok":
      break;
  }
  if (res?.added !== undefined || res?.removed !== undefined) {
    const parts = [res.added ? `+${res.added}` : "", res.removed ? `−${res.removed}` : ""].filter(Boolean);
    if (parts.length > 0) return parts.join(" ");
  }
  if (step?.path && res?.lines !== undefined) return copyText("stream.step.lines", { n: res.lines });
  if (res?.files !== undefined) return copyText("stream.step.files", { n: res.files });
  return dur;
}

/** 路径中间省略：留开头一段与文件名，中间换成 `…`（只在超长时）。 */
export function middleEllipsis(p: string, max = 72): string {
  if (p.length <= max) return p;
  const slash = p.lastIndexOf("/");
  const tail = slash >= 0 ? p.slice(slash) : p.slice(-Math.floor(max / 2));
  const room = Math.max(8, max - tail.length - 1);
  return `${p.slice(0, room)}…${tail}`;
}

const ICON_OF: Record<Exclude<StepState, "running" | "pending">, IconName> = {
  unclear: "question",
  ok: "check",
  failed: "failed",
  rejected: "close",
  unknown: "question",
};

/**
 * 一步那一行（放进 `<summary>`）。没有 `toolSteps` 那一格（老后端）⇒ 工具名 ＋ 调用方给的一句兜底。
 * `call` ＝ 这一步的调用 id（会话事实按它说这一步还没结果时是什么样子）。
 * 返回的那一行之后由 [`settleStepLine`] 按结果改、由 [`paintWaiting`] 按事实改。
 */
export function buildStepLine(name: string, step: ToolStep | undefined, fallbackArg: string, call?: string): HTMLElement {
  const row = document.createElement("span");
  row.className = "step-line";
  if (call) row.dataset.call = call;
  const ic = document.createElement("span");
  ic.className = "step-icon";
  const tool = document.createElement("span");
  tool.className = "step-tool";
  tool.textContent = step?.tool ?? name;
  const arg = document.createElement("span");
  arg.className = "step-arg";
  const a = step ? (step.arg ?? "") : fallbackArg;
  // 不挂 `title`：展开那一步就看得到全文；属性里的 `<` `>` 各引擎序列化也不一样（秤 2 的 DOM 指纹按引擎对拍）。
  arg.textContent = step?.path ? middleEllipsis(a) : a;
  row.append(ic, tool, arg);
  if (step?.note) {
    const note = document.createElement("span");
    note.className = "step-note";
    note.textContent = step.note;
    row.appendChild(note);
  }
  const right = document.createElement("span");
  right.className = "step-right";
  row.appendChild(right);
  paint(row, "pending", "");
  return row;
}

function paint(row: HTMLElement, state: StepState, right: string): void {
  row.dataset.state = state;
  const ic = row.querySelector<HTMLElement>(".step-icon");
  if (state === "pending") ic?.replaceChildren();
  else ic?.replaceChildren(state === "running" ? spinner() : icon(ICON_OF[state], "compact"));
  const r = row.querySelector<HTMLElement>(".step-right");
  if (r) r.textContent = right;
}

/** 思考那一步：大脑图标 ·「思考」· 第一行斜体预览。 */
export function buildThinkingLine(label: string, preview: string): HTMLElement {
  const row = document.createElement("span");
  row.className = "step-line";
  row.dataset.state = "thinking";
  const ic = document.createElement("span");
  ic.className = "step-icon";
  ic.appendChild(icon("brain", "compact"));
  const tool = document.createElement("span");
  tool.className = "step-tool";
  tool.textContent = label;
  const p = document.createElement("span");
  p.className = "step-preview";
  p.textContent = preview;
  row.append(ic, tool, p);
  return row;
}

/** 结果到了：改那一行的状态图标与右侧小字（同一步再来一次结果就再改一次，幂等）。 */
export function settleStepLine(row: HTMLElement, step: ToolStep | undefined, res: StepResult | undefined, isError: boolean, durText: string | null): StepState {
  row.querySelector(".step-await")?.remove();
  const state = stateOf(step, res, isError);
  paint(row, state, stepRight(step, res, state, durText));
  return state;
}


/**
 * 会话事实说这一步还没结果时是什么样子（`pending[].state`；不在 `pending` 里 ⇒ 状态不明）⇒ 照画。结果已经到了的不动。
 * 在等你：琥珀点 · 说明位「等你批准」（等的不是批准 ⇒「在等你」）· 右侧已等多久。状态不明：悬停说后端给的原因（`why`）。
 */
export function paintWaiting(row: HTMLElement, state: StepWait, waited: string | null, approve: boolean, why: UnclearWhy | null = null): void {
  if (!WAITING.has(row.dataset.state ?? "")) return;
  if (state === "awaiting") {
    markAwaiting(row, waited, approve);
    return;
  }
  row.querySelector(".step-await")?.remove();
  paint(row, state, stepRight(undefined, undefined, state, null));
  const r = row.querySelector<HTMLElement>(".step-right");
  if (!r) return;
  if (state === "unclear" && why) r.title = why === "noWriter" ? copyText("stream.step.unclearNoWriter") : copyText("stream.step.unclearUntracked");
  else r.removeAttribute("title");
}

function markAwaiting(row: HTMLElement, waited: string | null, approve: boolean): void {
  row.dataset.state = "awaiting";
  const dot = document.createElement("span");
  dot.className = "step-await-dot";
  row.querySelector<HTMLElement>(".step-icon")?.replaceChildren(dot);
  let w = row.querySelector<HTMLElement>(".step-await");
  if (!w) {
    w = document.createElement("span");
    w.className = "step-await";
    row.insertBefore(w, row.querySelector(".step-right"));
  }
  w.textContent = approve ? copyText("stream.step.awaiting") : copyText("stream.step.awaitingYou");
  const r = row.querySelector<HTMLElement>(".step-right");
  if (r) r.textContent = waited ?? "";
}

