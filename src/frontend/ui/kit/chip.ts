/**
 * chip（C6）：状态栏 / 筛选胶囊，紧凑档。只显示「现在是什么」，有事时才上色；没内容就不画（调用方不建）。
 * 点开浮层的 chip 是触发器：`aria-expanded` 跟着浮层开关。
 *
 * 连接药丸（每台机器的连接状态）：连着时不画；断了 `离线` 可点即重连；重连中至少 800ms；连回来 `已连接` 2 秒后自己走。
 */
import { icon, type IconName } from "./icon";
import { spinner } from "./progress";
import { copyText } from "../copy-table";
import s from "./chip.module.css";

export type ChipTone = "neutral" | "warn" | "error" | "success";

export interface ChipSpec {
  text: string;
  tone?: ChipTone;
  icon?: IconName;
  hint?: string;
  onClick?: () => void;
}

export function chip(spec: ChipSpec): HTMLElement {
  const c = document.createElement(spec.onClick ? "button" : "span");
  if (c instanceof HTMLButtonElement) c.type = "button";
  c.className = s.chip;
  c.dataset.intent = spec.tone ?? "neutral";
  if (spec.icon) c.appendChild(icon(spec.icon, "compact"));
  const t = document.createElement("span");
  t.textContent = spec.text;
  c.appendChild(t);
  if (spec.hint) c.title = spec.hint;
  if (spec.onClick) c.addEventListener("click", spec.onClick);
  return c;
}

/** 浮层开着 / 关着（触发器的样子跟着变，再点一次关由浮层那边管）。 */
export function setChipOpen(c: HTMLElement, open: boolean): void {
  c.setAttribute("aria-expanded", String(open));
}

export type LinkState = "up" | "down" | "connecting" | "restored";

/** 连接药丸的最短停留：重连中 · 连回来。 */
export const PILL_CONNECTING_MIN_MS = 800;
export const PILL_RESTORED_MS = 2000;

/** 连接药丸这一刻该画什么（`null` ＝ 不画）。计时由调用方按上面两个数排。 */
export function connectionPill(machine: string, state: LinkState, reconnect: () => void): HTMLElement | null {
  switch (state) {
    case "up":
      return null;
    case "down":
      return chip({ text: copyText("kit.pill.offline", { machine }), tone: "warn", onClick: reconnect, hint: copyText("kit.pill.reconnectHint") });
    case "connecting": {
      const c = chip({ text: copyText("kit.pill.connecting", { machine }) });
      c.insertBefore(spinner(), c.firstChild);
      return c;
    }
    case "restored":
      return chip({ text: copyText("kit.pill.restored", { machine }), tone: "success" });
  }
}
