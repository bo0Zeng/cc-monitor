/**
 * 拆分按钮：主按钮 ＋ ▾（▾ 开 kit 菜单）。「恢复 ▾」用（历史页查看器头 · 主窗口会话头）。
 *
 * ⚠ 暂定形：规范还没收这一件（设计稿「文件与历史」丙「提给规范」第 6 条：▾ 与主按钮同底、中间一道深色缝）；收了再照规范改。
 * - 两半各是一颗 kit 按钮（主按钮那一档 · 焦点环 · 禁用 / 进行中都照 `button.ts`）；▾ 的菜单项由调用方开菜单时现给（`items()`，可以要先问那台）。
 * - 菜单开着时不出悬停提示。
 */
import { button, type ButtonKind } from "./button";
import { icon } from "./icon";
import { openMenu, type MenuItem } from "./menu";
import s from "./split-button.module.css";

export interface SplitButtonSpec {
  label: string;
  kind?: Extract<ButtonKind, "primary" | "secondary">;
  /** 主按钮的悬停提示（说清点了会怎样）。 */
  hint?: string;
  onClick: () => void;
  /** ▾ 的读屏名与悬停提示。 */
  moreLabel: string;
  /** ▾ 菜单的项（开的那一刻现取）。 */
  items: () => MenuItem[] | Promise<MenuItem[]>;
}

export interface SplitButton {
  root: HTMLElement;
  main: HTMLButtonElement;
  more: HTMLButtonElement;
}

export function splitButton(spec: SplitButtonSpec): SplitButton {
  const root = document.createElement("span");
  root.className = s.split;
  const kind = spec.kind ?? "primary";
  const main = button({ label: spec.label, kind, hint: spec.hint, onClick: () => spec.onClick() });
  main.classList.add(s.splitMain);
  const more = button({
    label: spec.moreLabel,
    kind,
    hint: spec.moreLabel,
    onClick: () => {
      void Promise.resolve(spec.items()).then((items) => {
        if (more.isConnected) openMenu({ el: more, align: "end" }, items, { label: spec.moreLabel });
      });
    },
  });
  more.classList.add(s.splitMore);
  more.setAttribute("aria-haspopup", "menu");
  more.setAttribute("aria-label", spec.moreLabel);
  more.replaceChildren(icon("caretDown", "compact"));
  root.append(main, more);
  return { root, main, more };
}
