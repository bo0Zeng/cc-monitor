/**
 * 开关与复选框：开关 ＝ 立刻生效的一个设置；复选框 ＝ 勾一批、最后点一颗按钮才做。不许混用。
 *
 * 等后端答复的开关：拇指先动、旁边转圈；后端拒了拇指退回原位（[`settle`]），出错句由调用方放在行下。
 * 判据：`tests/frontend/ui/kit/components.vitest.ts`「开关 · 复选框」那一节。
 */
import { spinner } from "./progress";
import s from "./switch.module.css";

export interface SwitchSpec {
  label: string;
  /** 标签下一行的说明（小字）。 */
  help?: string;
  on: boolean;
  /** 拨动：返回 Promise 的 ⇒ 等它；`false` / 抛 ⇒ 退回原位。 */
  onChange: (on: boolean) => void | boolean | Promise<boolean | void>;
}

export function toggleSwitch(spec: SwitchSpec): { root: HTMLLabelElement; input: HTMLButtonElement; set(on: boolean): void } {
  const root = document.createElement("label");
  root.className = s.swRow;
  const text = document.createElement("span");
  text.className = s.swText;
  text.textContent = spec.label;
  if (spec.help) {
    const help = document.createElement("span");
    help.className = s.swHelp;
    help.textContent = spec.help;
    text.appendChild(help);
  }
  const sw = document.createElement("button");
  sw.type = "button";
  sw.className = s.swSwitch;
  sw.setAttribute("role", "switch");
  root.append(text, sw);
  const set = (on: boolean): void => sw.setAttribute("aria-checked", String(on));
  set(spec.on);
  sw.addEventListener("click", () => {
    if (sw.dataset.busy === "true" || sw.getAttribute("aria-disabled") === "true") return;
    const before = sw.getAttribute("aria-checked") === "true";
    set(!before);
    let r: ReturnType<SwitchSpec["onChange"]>;
    try {
      r = spec.onChange(!before);
    } catch {
      set(before);
      return;
    }
    if (r === false) return set(before);
    if (r instanceof Promise) {
      sw.dataset.busy = "true";
      const sp = spinner();
      root.appendChild(sp);
      r.then(
        (v) => {
          if (v === false) set(before);
        },
        () => set(before),
      ).finally(() => {
        delete sw.dataset.busy;
        sp.remove();
      });
    }
  });
  return { root, input: sw, set };
}

export function checkbox(label: string, checked: boolean, onChange: (checked: boolean) => void): HTMLLabelElement {
  const root = document.createElement("label");
  root.className = s.swCheck;
  const box = document.createElement("input");
  box.type = "checkbox";
  box.className = s.swBox;
  box.checked = checked;
  box.addEventListener("change", () => onChange(box.checked));
  const t = document.createElement("span");
  t.textContent = label;
  root.append(box, t);
  return root;
}

/** 单选一项（同一组同一个 `name`；选中这一项 ⇒ `onPick`）。 */
export function radio(name: string, label: string, checked: boolean, onPick: () => void): HTMLLabelElement {
  const root = document.createElement("label");
  root.className = s.swCheck;
  const box = document.createElement("input");
  box.type = "radio";
  box.name = name;
  box.className = s.swBox;
  box.checked = checked;
  box.addEventListener("change", () => {
    if (box.checked) onPick();
  });
  const t = document.createElement("span");
  t.textContent = label;
  root.append(box, t);
  return root;
}
