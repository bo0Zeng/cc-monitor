/**
 * 每台机器的状态成品：排成「点 · 名字旁那个词 · 问题行一句 · 修法按钮」。
 * 哪一态、那一句、给哪几颗修法、按钮上的字都是后端定的（`deploy::machine`）；这里只按那一态挑样子（点 · 语气 · 进度条）。
 */
import type { MachineState } from "../generated/MachineState";
import type { MachineFix } from "../generated/MachineFix";
import type { MachineFixButton } from "../generated/MachineFixButton";
import type { MachineStateKind } from "../generated/MachineStateKind";
import type { MachineDotState } from "../kit/status-dot";
import { button } from "../kit/button";
import { icon } from "../kit/icon";
import { spinner } from "../kit/progress";
import { copyDetailButton } from "../kit/detail";

export type { MachineState, MachineFix };

/** 一台怎么画。`problem` 为空 ＝ 不出问题行。 */
export interface MachineFace {
  dot: MachineDotState;
  /** 名字旁那个词（连着时不说）。 */
  word: string;
  problem: string;
  /** 问题行的样子：出错 · 待办 · 正在做（转圈；`bar` ＝ 装 / 更新那一段带进度条）。 */
  tone: "error" | "warn" | "busy";
  bar: boolean;
  fixes: MachineFixButton[];
  /** 页头「N 台离线 · M 台要更新」那两段各算不算它。 */
  offline: boolean;
  needsUpdate: boolean;
  /** 问题行那句后面跟的复制详情（壳写好的；空 ＝ 不出［复制详情］）。 */
  detail: string;
  /** 问题行下面那一句下一步（壳写好的；空 ＝ 不出那一行）。 */
  next: string;
}

const DOT: Record<MachineStateKind, MachineDotState> = {
  up: "up",
  newer: "up",
  connecting: "unknown",
  installing: "unknown",
  updating: "unknown",
  unknown: "unknown",
  down: "failed",
  host_key_changed: "failed",
  unsupported: "failed",
  needs_update: "needs-you",
  disabled: "exited",
};

export function machineFace(m: MachineState): MachineFace {
  const busy = m.state === "connecting" || m.state === "installing" || m.state === "updating";
  return {
    dot: DOT[m.state],
    word: m.word,
    problem: m.said,
    tone: busy ? "busy" : DOT[m.state] === "needs-you" ? "warn" : "error",
    bar: m.state === "installing" || m.state === "updating",
    fixes: m.fixes,
    offline: m.state === "down" || m.state === "host_key_changed" || m.state === "unsupported",
    needsUpdate: m.state === "needs_update",
    detail: m.detail ?? "",
    next: m.next ?? "",
  };
}

/**
 * 问题行（列表那一行与卡头同一个画法）：一句话 ＋ 修法按钮，壳写了下一步就另起一行跟在下面；正在连 / 装 / 更新时转圈（装 / 更新另有一条走动的进度条）。
 * `face.problem` 为空 ⇒ 收起。
 */
export function paintProblem(el: HTMLElement, face: MachineFace, onFix: (fix: MachineFix) => void): void {
  el.replaceChildren();
  el.hidden = face.problem === "";
  if (face.problem === "") return;
  el.dataset.severity = face.tone;
  const text = document.createElement("span");
  text.textContent = face.problem;
  el.append(face.tone === "busy" ? spinner() : icon(face.tone === "warn" ? "warning" : "error", "compact"), text);
  const copy = face.tone === "busy" ? null : copyDetailButton(face.problem, face.detail);
  if (copy) el.appendChild(copy);
  if (face.bar) {
    const bar = document.createElement("span");
    bar.className = "machine-problem-bar";
    el.appendChild(bar);
  }
  for (const fix of face.fixes) {
    el.appendChild(
      button({
        label: fix.label,
        size: "compact",
        kind: fix.fix === "update" ? "primary" : "secondary",
        onClick: (ev) => {
          ev.stopPropagation();
          onFix(fix.fix);
        },
      }),
    );
  }
  if (face.next !== "") {
    const next = document.createElement("div");
    next.className = "machine-problem-next";
    next.textContent = face.next;
    el.appendChild(next);
  }
}
