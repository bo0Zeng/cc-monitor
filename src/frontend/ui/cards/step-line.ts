/**
 * 过程里的一步一行：状态图标 · 工具名 · 主参数（等宽、一行、路径中间省略）· 说明 · 右侧小字。
 *
 * 判定都在后端：主参数与说明是 assistant 记录成品的 `toolSteps`，结果一句的数是 user 记录成品的 `toolResults`；
 * 这里只把它们排成一行、按 id 配对、把两条记录的时刻相减成耗时。界面不认入参与结果的结构。
 */
import type { ToolStep } from "../generated/ToolStep";
import type { StepResult } from "../generated/StepResult";
import { icon, type IconName } from "../kit/icon";
import { spinner } from "../kit/progress";
import { copyText } from "../copy-table";

/** 一步的状态：在跑 · 成功 · 失败 · 人没批准 · 认不出的工具（结果到了也只给原文）。 */
export type StepState = "running" | "ok" | "failed" | "rejected" | "unknown";

/** 耗时：< 10 秒一位小数（`0.3s`）· < 1 分整秒（`41s`）· 其余 `3m02s`。 */
export function fmtStepDur(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return "";
  if (ms < 10_000) return `${(ms / 1000).toFixed(1)}s`;
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  return `${m}m${String(s % 60).padStart(2, "0")}s`;
}

/** 两个记录时刻相减（任一读不出 ⇒ `null`）。 */
export function durBetween(from: string | undefined, to: string | undefined): number | null {
  if (!from || !to) return null;
  const a = Date.parse(from);
  const b = Date.parse(to);
  return Number.isFinite(a) && Number.isFinite(b) && b >= a ? b - a : null;
}

/** 结果到了 ⇒ 这一步的状态。 */
export function stateOf(step: ToolStep | undefined, res: StepResult | undefined, isError: boolean): StepState {
  if (res?.rejected) return "rejected";
  if (res ? !res.ok : isError) return "failed";
  if (step && !step.known) return "unknown";
  return "ok";
}

/** 右侧小字：改动 `+38 −6` · 读了几行 · 几个文件 · 否则耗时；失败 `失败 · 41s` · 被拒「未批准」· 认不出「未识别结果 · 原文」。 */
export function stepRight(step: ToolStep | undefined, res: StepResult | undefined, state: StepState, durMs: number | null): string {
  const dur = durMs === null ? "" : fmtStepDur(durMs);
  switch (state) {
    case "running":
      return "";
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

const ICON_OF: Record<Exclude<StepState, "running">, IconName> = {
  ok: "check",
  failed: "failed",
  rejected: "close",
  unknown: "question",
};

/**
 * 一步那一行（放进 `<summary>`）。没有 `toolSteps` 那一格（老后端）⇒ 工具名 ＋ 调用方给的一句兜底。
 * 返回的那一行之后由 [`settleStepLine`] 按结果改状态与右侧小字。
 */
export function buildStepLine(name: string, step: ToolStep | undefined, fallbackArg: string): HTMLElement {
  const row = document.createElement("span");
  row.className = "step-line";
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
  paint(row, "running", "");
  return row;
}

function paint(row: HTMLElement, state: StepState, right: string): void {
  row.dataset.state = state;
  const ic = row.querySelector<HTMLElement>(".step-icon");
  ic?.replaceChildren(state === "running" ? spinner() : icon(ICON_OF[state], "compact"));
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
export function settleStepLine(row: HTMLElement, step: ToolStep | undefined, res: StepResult | undefined, isError: boolean, durMs: number | null): StepState {
  const state = stateOf(step, res, isError);
  paint(row, state, stepRight(step, res, state, durMs));
  return state;
}
