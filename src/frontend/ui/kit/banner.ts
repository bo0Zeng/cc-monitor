/**
 * 错误条 · 警告条（C15）：放在出错的那块顶上；`--bg-2` 底、左 3px 色条、16px 图标、一句话 ＋ 右侧动作。
 * 不带关闭 ×：事解决了条由调用方摘掉。
 */
import { icon } from "./icon";
import s from "./banner.module.css";

export type BannerTone = "error" | "warn";

export function banner(tone: BannerTone, text: string, actions: HTMLElement[] = []): HTMLDivElement {
  const b = document.createElement("div");
  b.className = s.banner;
  b.dataset.intent = tone;
  b.setAttribute("role", tone === "error" ? "alert" : "status");
  const t = document.createElement("span");
  t.className = s.bannerText;
  t.textContent = text;
  b.append(icon(tone === "error" ? "error" : "warning"), t, ...actions);
  return b;
}
